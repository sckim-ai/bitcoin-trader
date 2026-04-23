use crate::api::upbit::UpbitClient;
use crate::core::indicators;
use crate::models::market::{Candle, MarketData};
use crate::models::trading::TradingParameters;
use crate::notifications::manager::NotificationManager;
use crate::strategies::Strategy;
use chrono::{DateTime, Timelike, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// ─── Event payloads ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTradeLog {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTradeEvent {
    pub side: String,
    pub market: String,
    pub price: f64,
    pub volume: f64,
    pub pnl: Option<f64>,
    pub signal: String,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTradeStatus {
    pub running: bool,
    pub market: String,
    pub strategy: String,
    pub last_signal: String,
    pub last_check: String,
}

// ─── Core auto-trading logic (no Tauri dependency) ───

/// Result of a single trading cycle.
#[derive(Debug, Clone)]
pub struct CycleResult {
    pub signal: String,
    pub trade: Option<AutoTradeEvent>,
    pub logs: Vec<AutoTradeLog>,
}

/// Position info read from / written to DB.
#[derive(Debug, Clone)]
pub struct DbPosition {
    pub status: String,
    pub buy_price: f64,
    pub buy_volume: f64,
    pub buy_psy: f64,
}

/// Calculate seconds until the next hour boundary.
pub fn seconds_until_next_hour() -> u64 {
    let now = Utc::now();
    let secs_into_hour = now.minute() * 60 + now.second();
    let remaining = 3600 - secs_into_hour as u64;
    // If less than 10 seconds to next hour, wait for the hour after
    if remaining < 10 {
        remaining + 3600
    } else {
        remaining
    }
}

/// Calculate split order chunks for a given total amount.
/// Returns a vector of fractions (each chunk's fraction of total).
pub fn calculate_split_orders(total_krw: f64) -> Vec<f64> {
    if total_krw > 500_000.0 {
        // 3-way split
        vec![total_krw / 3.0, total_krw / 3.0, total_krw / 3.0]
    } else {
        vec![total_krw]
    }
}

/// Load position from DB, returning (status, buy_price, buy_volume, buy_psy).
pub fn load_position(conn: &Connection, market: &str) -> DbPosition {
    let result = conn.query_row(
        "SELECT status, COALESCE(buy_price, 0), COALESCE(buy_volume, 0), COALESCE(buy_psy, 0)
         FROM positions WHERE market = ?1 AND user_id = 1",
        [market],
        |row| {
            Ok(DbPosition {
                status: row.get(0)?,
                buy_price: row.get(1)?,
                buy_volume: row.get(2)?,
                buy_psy: row.get(3)?,
            })
        },
    );
    result.unwrap_or(DbPosition {
        status: "idle".to_string(),
        buy_price: 0.0,
        buy_volume: 0.0,
        buy_psy: 0.0,
    })
}

/// Upsert position in DB.
pub fn save_position(
    conn: &Connection,
    market: &str,
    status: &str,
    buy_price: f64,
    buy_volume: f64,
    buy_psy: f64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    conn.execute(
        "INSERT INTO positions (user_id, market, status, buy_price, buy_volume, buy_psy, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(user_id, market) DO UPDATE SET
           status = ?2, buy_price = ?3, buy_volume = ?4, buy_psy = ?5, updated_at = datetime('now')",
        params![market, status, buy_price, buy_volume, buy_psy],
    ).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    Ok(())
}

/// Record a trade in the trades table.
pub fn record_trade(
    conn: &Connection,
    market: &str,
    side: &str,
    price: f64,
    volume: f64,
    fee: f64,
    strategy_key: &str,
    signal_type: &str,
    pnl: Option<f64>,
    pnl_pct: Option<f64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    conn.execute(
        "INSERT INTO trades (user_id, market, side, order_type, price, volume, fee, strategy_key, signal_type, pnl, pnl_pct, executed_at)
         VALUES (1, ?1, ?2, 'limit', ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
        params![market, side, price, volume, fee, strategy_key, signal_type, pnl, pnl_pct],
    ).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    Ok(())
}

/// Reconcile DB position against actual Upbit balance.
/// Returns corrected position status.
pub fn reconcile_position(
    db_pos: &DbPosition,
    actual_coin_balance: f64,
    current_price: f64,
) -> (&'static str, f64, f64) {
    let min_value = 5_000.0; // minimum tradeable KRW value

    match db_pos.status.as_str() {
        "holding" => {
            if actual_coin_balance * current_price < min_value {
                // DB says holding but no coins — correct to idle
                ("idle", 0.0, 0.0)
            } else {
                ("holding", db_pos.buy_price, actual_coin_balance)
            }
        }
        _ => {
            if actual_coin_balance * current_price >= min_value {
                // DB says idle but coins exist — correct to holding
                ("holding", current_price, actual_coin_balance)
            } else {
                ("idle", 0.0, 0.0)
            }
        }
    }
}

/// Fetch candles from Upbit API and convert to MarketData with indicators.
///
/// Live-mode day_psy: build the KST-date map from day candles already
/// persisted in `db` (kept in sync by the background market updater).
/// Missing day data → NaN psy_day, which strategies treat as "skip".
pub async fn fetch_and_prepare_data(
    client: &UpbitClient,
    db: &Arc<Mutex<Connection>>,
    api_market: &str,
    count: u32,
) -> Result<Vec<MarketData>, Box<dyn std::error::Error + Send + Sync>> {
    // DB stores markets in short form ("BTC", "ETH") while Upbit API uses
    // "KRW-<sym>". Derive the DB key from the API market string.
    let db_market = api_market.split('-').nth(1).unwrap_or(api_market);

    let raw_candles = client.get_candles(api_market, "60", count).await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    let mut candles: Vec<Candle> = raw_candles
        .iter()
        .filter_map(|v| {
            let ts_str = v.get("candle_date_time_utc")?.as_str()?;
            let timestamp: DateTime<Utc> = format!("{}Z", ts_str).parse().ok()
                .or_else(|| ts_str.parse().ok())?;
            Some(Candle {
                timestamp,
                open: v.get("opening_price")?.as_f64()?,
                high: v.get("high_price")?.as_f64()?,
                low: v.get("low_price")?.as_f64()?,
                close: v.get("trade_price")?.as_f64()?,
                volume: v.get("candle_acc_trade_volume")?.as_f64()?,
            })
        })
        .collect();

    candles.sort_by_key(|c| c.timestamp);

    // Build day-PSY map from DB-persisted day candles.
    let day_psy_map = {
        let conn = db.lock()
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
        let day_candles = crate::migration::csv_import::load_candles(&conn, db_market, "day", None)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
        indicators::build_day_psy_map(&day_candles)
    };

    let indicator_sets = indicators::calculate_all_with_day_psy(&candles, Some(&day_psy_map));
    let data: Vec<MarketData> = candles
        .into_iter()
        .zip(indicator_sets)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect();

    Ok(data)
}

/// Execute one trading cycle. Pure logic, no Tauri dependency.
/// Returns the cycle result with signal, optional trade event, and log messages.
pub async fn execute_cycle(
    client: &UpbitClient,
    db: &Arc<Mutex<Connection>>,
    market: &str,
    strategy: &dyn Strategy,
    strategy_key: &str,
    params: &TradingParameters,
) -> Result<CycleResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut logs = Vec::new();
    let now = Utc::now().format("%H:%M:%S").to_string();

    logs.push(AutoTradeLog {
        timestamp: now.clone(),
        level: "INFO".into(),
        message: format!("Trading cycle started — {} / {}", market, strategy_key),
    });

    // 1. Fetch current price and balance
    let current_price = client.get_current_price(market).await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    let currency = market.split('-').nth(1).unwrap_or("BTC");
    let coin_balance = client.get_balance(currency).await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    let krw_balance = client.get_balance("KRW").await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    // 2. Position reconciliation
    let db_pos = {
        let conn = db.lock().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
        load_position(&conn, market)
    };

    let (status, buy_price, buy_volume) =
        reconcile_position(&db_pos, coin_balance, current_price);

    if status != db_pos.status.as_str() {
        logs.push(AutoTradeLog {
            timestamp: now.clone(),
            level: "WARN".into(),
            message: format!(
                "Position reconciled: {} → {} (balance: {:.6})",
                db_pos.status, status, coin_balance
            ),
        });
        let conn = db.lock().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
        save_position(&conn, market, status, buy_price, buy_volume, db_pos.buy_psy)?;
    }

    // 3. Fetch candles and calculate indicators
    let data = fetch_and_prepare_data(client, db, market, 200).await?;
    if data.len() < 15 {
        return Ok(CycleResult {
            signal: "insufficient_data".into(),
            trade: None,
            logs: {
                logs.push(AutoTradeLog {
                    timestamp: now,
                    level: "WARN".into(),
                    message: format!("Insufficient data: {} candles (need ≥15)", data.len()),
                });
                logs
            },
        });
    }

    logs.push(AutoTradeLog {
        timestamp: now.clone(),
        level: "INFO".into(),
        message: format!("Data ready: {} candles, price: {:.0}", data.len(), current_price),
    });

    // 4. Generate signal via strategy
    let position_state = crate::core::signals::PositionState {
        position: if status == "holding" { 1 } else { 0 },
        buy_price,
        buy_volume,
        buy_psy: db_pos.buy_psy,
        hold_bars: 0,
        highest_since_buy: if status == "holding" { current_price.max(buy_price) } else { 0.0 },
        entry_rsi: 0.0,
    };

    let signal = strategy.get_latest_signal(&data, params, &position_state);
    let signal_name = format!("{:?}", signal.signal_type);

    logs.push(AutoTradeLog {
        timestamp: now.clone(),
        level: "INFO".into(),
        message: format!("Signal: {} (confidence: {:?})", signal_name, signal.confidence),
    });

    // 5. Execute trade based on signal
    let mut trade_event: Option<AutoTradeEvent> = None;
    let fee_rate = params.v3_fee_rate;

    match signal.signal_type {
        crate::core::signals::SignalType::Buy | crate::core::signals::SignalType::BuyReadyConfirmed => {
            if status != "holding" && krw_balance > 5_000.0 {
                let usable = krw_balance * 0.9995; // reserve for fee
                let chunks = calculate_split_orders(usable);

                logs.push(AutoTradeLog {
                    timestamp: now.clone(),
                    level: "INFO".into(),
                    message: format!("BUY: {:.0} KRW in {} chunk(s)", usable, chunks.len()),
                });

                let mut total_volume = 0.0;
                for (i, chunk_krw) in chunks.iter().enumerate() {
                    let vol = chunk_krw / current_price;
                    match client.place_limit_buy(market, vol, current_price).await {
                        Ok(_) => {
                            total_volume += vol;
                            logs.push(AutoTradeLog {
                                timestamp: now.clone(),
                                level: "INFO".into(),
                                message: format!("  Chunk {}: {:.6} @ {:.0}", i + 1, vol, current_price),
                            });
                        }
                        Err(e) => {
                            logs.push(AutoTradeLog {
                                timestamp: now.clone(),
                                level: "ERROR".into(),
                                message: format!("  Chunk {} failed: {}", i + 1, e),
                            });
                        }
                    }
                    if chunks.len() > 1 && i < chunks.len() - 1 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }

                if total_volume > 0.0 {
                    let psy = data.last().map(|d| d.indicators.psy_hour).unwrap_or(0.0);
                    let conn = db.lock().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
                    save_position(&conn, market, "holding", current_price, total_volume, psy)?;
                    record_trade(
                        &conn, market, "buy", current_price, total_volume,
                        current_price * total_volume * fee_rate,
                        strategy_key, &signal_name, None, None,
                    )?;

                    trade_event = Some(AutoTradeEvent {
                        side: "buy".into(),
                        market: market.into(),
                        price: current_price,
                        volume: total_volume,
                        pnl: None,
                        signal: signal_name.clone(),
                        strategy: strategy_key.into(),
                    });
                }
            }
        }
        crate::core::signals::SignalType::Sell | crate::core::signals::SignalType::SellReadyConfirmed => {
            if status == "holding" && coin_balance * current_price > 5_000.0 {
                let pnl_pct = if buy_price > 0.0 {
                    (current_price - buy_price) / buy_price * 100.0
                } else {
                    0.0
                };

                logs.push(AutoTradeLog {
                    timestamp: now.clone(),
                    level: "INFO".into(),
                    message: format!("SELL: {:.6} @ {:.0} (P/L: {:.2}%)", coin_balance, current_price, pnl_pct),
                });

                match client.place_limit_sell(market, coin_balance, current_price).await {
                    Ok(_) => {
                        let pnl = (current_price - buy_price) * coin_balance;
                        let conn = db.lock().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
                        save_position(&conn, market, "idle", 0.0, 0.0, 0.0)?;
                        record_trade(
                            &conn, market, "sell", current_price, coin_balance,
                            current_price * coin_balance * fee_rate,
                            strategy_key, &signal_name, Some(pnl), Some(pnl_pct),
                        )?;

                        trade_event = Some(AutoTradeEvent {
                            side: "sell".into(),
                            market: market.into(),
                            price: current_price,
                            volume: coin_balance,
                            pnl: Some(pnl_pct),
                            signal: signal_name.clone(),
                            strategy: strategy_key.into(),
                        });
                    }
                    Err(e) => {
                        logs.push(AutoTradeLog {
                            timestamp: now.clone(),
                            level: "ERROR".into(),
                            message: format!("Sell order failed: {}", e),
                        });
                    }
                }
            }
        }
        _ => {
            // Hold / BuyReady / SellReady — no action
        }
    }

    // 6. Send notifications (fire-and-forget)
    if let Some(ref evt) = trade_event {
        let notif_result = {
            let conn = db.lock().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
            NotificationManager::from_db(&conn, 1)
        };
        notif_result
            .notify_trade(&evt.side, &evt.market, evt.price, evt.volume, evt.pnl)
            .await;
    }

    Ok(CycleResult {
        signal: signal_name,
        trade: trade_event,
        logs,
    })
}

/// Run the auto-trading loop until cancelled.
/// This is the Tauri-aware entry point that emits events.
#[cfg(feature = "tauri-app")]
pub async fn run_loop(
    app_handle: tauri::AppHandle,
    db: Arc<Mutex<Connection>>,
    registry: crate::strategies::StrategyRegistry,
    strategy_key: String,
    market: String,
    params: TradingParameters,
    cancel_token: Arc<AtomicBool>,
) {
    use tauri::Emitter;

    let emit_log = |handle: &tauri::AppHandle, level: &str, msg: &str| {
        let log = AutoTradeLog {
            timestamp: Utc::now().format("%H:%M:%S").to_string(),
            level: level.into(),
            message: msg.into(),
        };
        let _ = handle.emit("auto-trade:log", &log);
    };

    emit_log(&app_handle, "INFO", &format!("Auto-trading started: {} / {}", market, strategy_key));
    let _ = app_handle.emit("auto-trade:status", AutoTradeStatus {
        running: true,
        market: market.clone(),
        strategy: strategy_key.clone(),
        last_signal: "starting".into(),
        last_check: Utc::now().format("%H:%M:%S").to_string(),
    });

    let client = match create_client() {
        Ok(c) => c,
        Err(e) => {
            emit_log(&app_handle, "ERROR", &format!("Failed to create API client: {}", e));
            return;
        }
    };

    loop {
        if cancel_token.load(Ordering::Relaxed) {
            emit_log(&app_handle, "INFO", "Auto-trading stopped by user");
            break;
        }

        let strategy = match registry.get(&strategy_key) {
            Some(s) => s,
            None => {
                emit_log(&app_handle, "ERROR", &format!("Strategy '{}' not found", strategy_key));
                break;
            }
        };

        match execute_cycle(&client, &db, &market, strategy, &strategy_key, &params).await {
            Ok(result) => {
                // Emit all logs
                for log in &result.logs {
                    let _ = app_handle.emit("auto-trade:log", log);
                }
                // Emit trade event if any
                if let Some(ref trade) = result.trade {
                    let _ = app_handle.emit("auto-trade:trade", trade);
                    // Refresh position on frontend
                    let pos = {
                        let conn = db.lock().unwrap();
                        load_position(&conn, &market)
                    };
                    let _ = app_handle.emit("auto-trade:position", serde_json::json!({
                        "status": pos.status,
                        "buy_price": pos.buy_price,
                        "buy_volume": pos.buy_volume,
                    }));
                }
                // Update status
                let _ = app_handle.emit("auto-trade:status", AutoTradeStatus {
                    running: true,
                    market: market.clone(),
                    strategy: strategy_key.clone(),
                    last_signal: result.signal,
                    last_check: Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            Err(e) => {
                emit_log(&app_handle, "ERROR", &format!("Cycle error: {} — retrying in 60s", e));
                // Wait 60 seconds before retry
                for _ in 0..60 {
                    if cancel_token.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                continue;
            }
        }

        // Wait until next hour
        let wait_secs = seconds_until_next_hour();
        emit_log(&app_handle, "INFO", &format!("Next cycle in {} seconds", wait_secs));

        // Check cancel every second during wait
        for _ in 0..wait_secs {
            if cancel_token.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    let _ = app_handle.emit("auto-trade:status", AutoTradeStatus {
        running: false,
        market: market.clone(),
        strategy: strategy_key.clone(),
        last_signal: "stopped".into(),
        last_check: Utc::now().format("%H:%M:%S").to_string(),
    });
}

fn create_client() -> Result<UpbitClient, String> {
    let access_key =
        std::env::var("UPBIT_ACCESS_KEY").map_err(|_| "UPBIT_ACCESS_KEY not set".to_string())?;
    let secret_key =
        std::env::var("UPBIT_SECRET_KEY").map_err(|_| "UPBIT_SECRET_KEY not set".to_string())?;
    Ok(UpbitClient::new(access_key, secret_key))
}

// ─── Data auto-update ───

/// Fetch latest candles from Upbit and insert into DB.
/// Returns the number of newly inserted candles.
pub async fn update_market_data(
    client: &UpbitClient,
    conn: &Connection,
    market: &str,
    timeframe: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let interval = match timeframe {
        "hour" => "60",
        "day" => "day",
        "week" => "week",
        _ => "60",
    };

    let raw = client.get_candles(market, interval, 200).await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    let mut count = 0;
    let tx = conn.unchecked_transaction()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO market_data (market, timeframe, timestamp, open, high, low, close, volume)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        ).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

        for v in &raw {
            let ts = match v.get("candle_date_time_utc").and_then(|t| t.as_str()) {
                Some(t) => format!("{}Z", t.trim_end_matches('Z')),
                None => continue,
            };
            let open = v.get("opening_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let high = v.get("high_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let low = v.get("low_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let close = v.get("trade_price").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let volume = v.get("candle_acc_trade_volume").and_then(|x| x.as_f64()).unwrap_or(0.0);

            if close == 0.0 {
                continue;
            }

            let inserted = stmt.execute(params![market, timeframe, ts, open, high, low, close, volume])
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
            count += inserted;
        }
    }
    tx.commit()
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    // Populate day_psy for any hour rows now eligible (day candles present,
    // previous-KST-date PSY computable). Safe to call per-tf sync.
    let _ = crate::core::day_psy_store::refresh_day_psy(conn, market)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    Ok(count)
}

/// Update all markets and timeframes. Returns summary.
pub async fn auto_update_all(
    client: &UpbitClient,
    conn: &Connection,
) -> Result<Vec<(String, String, usize)>, Box<dyn std::error::Error + Send + Sync>> {
    let markets = ["KRW-BTC", "KRW-ETH"];
    let timeframes = ["hour", "day", "week"];
    let mut results = Vec::new();

    for market in &markets {
        for timeframe in &timeframes {
            match update_market_data(client, conn, market, timeframe).await {
                Ok(count) => {
                    results.push((market.to_string(), timeframe.to_string(), count));
                }
                Err(e) => {
                    eprintln!("Update failed for {} {}: {}", market, timeframe, e);
                    results.push((market.to_string(), timeframe.to_string(), 0));
                }
            }
            // Rate limit: 150ms between API calls
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── seconds_until_next_hour ───

    #[test]
    fn test_seconds_until_next_hour_range() {
        let secs = seconds_until_next_hour();
        // Should be between 10 and 7200 (could be 3600+3600 if <10 secs left)
        assert!(secs >= 10);
        assert!(secs <= 7200);
    }

    // ─── calculate_split_orders ───

    #[test]
    fn test_split_orders_small_amount() {
        let chunks = calculate_split_orders(100_000.0);
        assert_eq!(chunks.len(), 1);
        assert!((chunks[0] - 100_000.0).abs() < 0.01);
    }

    #[test]
    fn test_split_orders_large_amount() {
        let chunks = calculate_split_orders(600_000.0);
        assert_eq!(chunks.len(), 3);
        for chunk in &chunks {
            assert!((chunk - 200_000.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_split_orders_boundary() {
        // Exactly 500,000 — should be single order
        let chunks = calculate_split_orders(500_000.0);
        assert_eq!(chunks.len(), 1);

        // Just over 500,000 — should split
        let chunks = calculate_split_orders(500_001.0);
        assert_eq!(chunks.len(), 3);
    }

    // ─── reconcile_position ───

    #[test]
    fn test_reconcile_holding_with_coins() {
        let pos = DbPosition {
            status: "holding".into(),
            buy_price: 50_000_000.0,
            buy_volume: 0.001,
            buy_psy: 45.0,
        };
        let (status, price, vol) = reconcile_position(&pos, 0.001, 50_000_000.0);
        assert_eq!(status, "holding");
        assert!((price - 50_000_000.0).abs() < 0.01);
        assert!((vol - 0.001).abs() < 1e-8);
    }

    #[test]
    fn test_reconcile_holding_without_coins() {
        // DB says holding but no actual coins
        let pos = DbPosition {
            status: "holding".into(),
            buy_price: 50_000_000.0,
            buy_volume: 0.001,
            buy_psy: 45.0,
        };
        let (status, price, vol) = reconcile_position(&pos, 0.0, 50_000_000.0);
        assert_eq!(status, "idle");
        assert!((price - 0.0).abs() < 0.01);
        assert!((vol - 0.0).abs() < 1e-8);
    }

    #[test]
    fn test_reconcile_idle_with_coins() {
        // DB says idle but coins exist
        let pos = DbPosition {
            status: "idle".into(),
            buy_price: 0.0,
            buy_volume: 0.0,
            buy_psy: 0.0,
        };
        let (status, price, _vol) = reconcile_position(&pos, 0.001, 50_000_000.0);
        assert_eq!(status, "holding");
        assert!((price - 50_000_000.0).abs() < 0.01);
    }

    #[test]
    fn test_reconcile_idle_without_coins() {
        let pos = DbPosition {
            status: "idle".into(),
            buy_price: 0.0,
            buy_volume: 0.0,
            buy_psy: 0.0,
        };
        let (status, _, _) = reconcile_position(&pos, 0.0, 50_000_000.0);
        assert_eq!(status, "idle");
    }

    #[test]
    fn test_reconcile_dust_amount_ignored() {
        // Tiny balance below 5,000 KRW should be treated as empty
        let pos = DbPosition {
            status: "holding".into(),
            buy_price: 50_000_000.0,
            buy_volume: 0.001,
            buy_psy: 45.0,
        };
        // 0.00001 BTC * 50M = 500 KRW — dust
        let (status, _, _) = reconcile_position(&pos, 0.00001, 50_000_000.0);
        assert_eq!(status, "idle");
    }

    // ─── DB operations ───

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let schema = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(schema).unwrap();
        let users = include_str!("../../migrations/002_users.sql");
        conn.execute_batch(users).unwrap();
        conn
    }

    #[test]
    fn test_load_position_empty() {
        let conn = setup_db();
        let pos = load_position(&conn, "KRW-BTC");
        assert_eq!(pos.status, "idle");
        assert!((pos.buy_price - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_save_and_load_position() {
        let conn = setup_db();
        save_position(&conn, "KRW-BTC", "holding", 50_000_000.0, 0.001, 45.0).unwrap();

        let pos = load_position(&conn, "KRW-BTC");
        assert_eq!(pos.status, "holding");
        assert!((pos.buy_price - 50_000_000.0).abs() < 0.01);
        assert!((pos.buy_volume - 0.001).abs() < 1e-8);
        assert!((pos.buy_psy - 45.0).abs() < 0.01);
    }

    #[test]
    fn test_save_position_upsert() {
        let conn = setup_db();
        save_position(&conn, "KRW-BTC", "holding", 50_000_000.0, 0.001, 45.0).unwrap();
        save_position(&conn, "KRW-BTC", "idle", 0.0, 0.0, 0.0).unwrap();

        let pos = load_position(&conn, "KRW-BTC");
        assert_eq!(pos.status, "idle");

        // Should only have 1 row
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM positions WHERE market = 'KRW-BTC'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_trade_buy() {
        let conn = setup_db();
        record_trade(
            &conn, "KRW-BTC", "buy", 50_000_000.0, 0.001, 25.0,
            "V0", "Buy", None, None,
        ).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM trades WHERE market = 'KRW-BTC'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_trade_sell_with_pnl() {
        let conn = setup_db();
        record_trade(
            &conn, "KRW-BTC", "sell", 51_000_000.0, 0.001, 25.5,
            "V0", "Sell", Some(1000.0), Some(2.0),
        ).unwrap();

        let (pnl, pnl_pct): (f64, f64) = conn
            .query_row(
                "SELECT pnl, pnl_pct FROM trades WHERE market = 'KRW-BTC' AND side = 'sell'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!((pnl - 1000.0).abs() < 0.01);
        assert!((pnl_pct - 2.0).abs() < 0.01);
    }

    // ─── Full cycle scenarios (DB-only, no API) ───

    #[test]
    fn test_position_lifecycle() {
        let conn = setup_db();
        let market = "KRW-BTC";

        // Start idle
        let pos = load_position(&conn, market);
        assert_eq!(pos.status, "idle");

        // Buy
        save_position(&conn, market, "holding", 50_000_000.0, 0.001, 45.0).unwrap();
        record_trade(&conn, market, "buy", 50_000_000.0, 0.001, 25.0, "V0", "Buy", None, None).unwrap();

        let pos = load_position(&conn, market);
        assert_eq!(pos.status, "holding");

        // Sell
        save_position(&conn, market, "idle", 0.0, 0.0, 0.0).unwrap();
        record_trade(&conn, market, "sell", 51_000_000.0, 0.001, 25.5, "V0", "Sell", Some(1000.0), Some(2.0)).unwrap();

        let pos = load_position(&conn, market);
        assert_eq!(pos.status, "idle");

        // Verify 2 trades recorded
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM trades", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_multiple_markets_independent() {
        let conn = setup_db();

        save_position(&conn, "KRW-BTC", "holding", 50_000_000.0, 0.001, 45.0).unwrap();
        save_position(&conn, "KRW-ETH", "idle", 0.0, 0.0, 0.0).unwrap();

        let btc = load_position(&conn, "KRW-BTC");
        let eth = load_position(&conn, "KRW-ETH");
        assert_eq!(btc.status, "holding");
        assert_eq!(eth.status, "idle");
    }

    // ─── Cancel token ───

    #[test]
    fn test_cancel_token() {
        let token = Arc::new(AtomicBool::new(false));
        assert!(!token.load(Ordering::Relaxed));

        token.store(true, Ordering::Relaxed);
        assert!(token.load(Ordering::Relaxed));
    }
}
