use crate::api::upbit::UpbitClient;
use crate::models::trading::TradingParameters;
use crate::services::auto_trader;
use crate::state::{AppState, AutoTradingHandle};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub status: String,
    pub buy_price: f64,
    pub buy_volume: f64,
    pub pnl_pct: f64,
}

fn create_client() -> Result<UpbitClient, String> {
    // Resolution order: OS keyring → env var → error.
    // See commands::upbit_keys::load_upbit_keys.
    crate::commands::upbit_keys::upbit_client_or_err()
}

/// Public ticker/candle endpoints don't need auth — empty keys are fine.
/// Still tries keyring/env first so an existing UpbitClient instance is reused
/// transparently for both public and authed endpoints.
fn create_public_client() -> UpbitClient {
    let (access, secret, _) = crate::commands::upbit_keys::load_upbit_keys();
    UpbitClient::new(access.unwrap_or_default(), secret.unwrap_or_default())
}

#[tauri::command]
pub async fn get_current_price(market: String) -> Result<f64, String> {
    let client = create_public_client();
    client
        .get_current_price(&market)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_balance(currency: String) -> Result<f64, String> {
    let client = create_client()?;
    client
        .get_balance(&currency)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn manual_buy(market: String, volume: f64, price: f64) -> Result<String, String> {
    let client = create_client()?;
    let result = client
        .place_limit_buy(&market, volume, price)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.to_string())
}

#[tauri::command]
pub async fn manual_sell(market: String, volume: f64, price: f64) -> Result<String, String> {
    let client = create_client()?;
    let result = client
        .place_limit_sell(&market, volume, price)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.to_string())
}

// ─── Manual market orders (post-4A, user-triggered) ────────────────────────
//
// Distinct from the auto-cycle orders (signal="real_buy"/"real_sell" etc).
// signal="manual_buy"/"manual_sell" so the history page + CSV can separate
// "user pressed a button" from "the strategy decided".
//
// Booking policy: if `session_id` is provided AND the session is in real
// mode, attribute the trade to that session (so live_return updates and the
// position state reconciles next cycle). Otherwise just place the order
// without DB attribution — useful for one-off testing where the user doesn't
// want to mix manual fills with a session's track record.

#[derive(serde::Deserialize)]
pub struct ManualOrderArgs {
    pub market: String,            // "KRW-ETH" etc.
    pub side: String,              // "buy" | "sell"
    /// "market" or "limit". market=즉시 체결, limit=지정가 등록(미체결 가능).
    #[serde(default = "default_ord_type")]
    pub ord_type: String,
    pub krw_amount: Option<f64>,   // buy + (market or limit)
    pub volume: Option<f64>,       // sell + any, or buy+limit override
    /// limit 주문일 때만 사용. 매수: 지정 가격, 매도: 호가.
    pub limit_price: Option<f64>,
    pub session_id: Option<i64>,   // None → don't write to live_trades
}

fn default_ord_type() -> String {
    "market".into()
}

#[derive(serde::Serialize)]
pub struct ManualOrderResult {
    pub uuid: String,
    pub state: String,
    pub executed_volume: f64,
    pub side: String,
    pub market: String,
}

#[tauri::command]
pub async fn manual_market_order(
    args: ManualOrderArgs,
    state: State<'_, AppState>,
) -> Result<ManualOrderResult, String> {
    let client = create_client()?;

    // 4 cases: (market, buy) / (market, sell) / (limit, buy) / (limit, sell).
    // Limit-buy derives volume from KRW / target_price so the user only enters
    // a single amount in either case (UI symmetry with market-buy).
    let order = match (args.side.as_str(), args.ord_type.as_str()) {
        ("buy", "market") => {
            let krw = args.krw_amount.ok_or("krw_amount required for market buy")?;
            if krw < 5_000.0 {
                return Err(format!("Upbit minimum 5,000 KRW (got {:.0})", krw));
            }
            client.place_market_buy(&args.market, krw).await?
        }
        ("sell", "market") => {
            let vol = args.volume.ok_or("volume required for market sell")?;
            if vol <= 0.0 { return Err("volume must be > 0".into()); }
            client.place_market_sell(&args.market, vol).await?
        }
        ("buy", "limit") => {
            let price = args.limit_price.ok_or("limit_price required for limit buy")?;
            if price <= 0.0 { return Err("limit_price must be > 0".into()); }
            // volume can be supplied directly OR derived from krw_amount/price.
            let volume = match (args.volume, args.krw_amount) {
                (Some(v), _) if v > 0.0 => v,
                (_, Some(krw)) if krw > 0.0 => (krw / price * 1e8).floor() / 1e8,
                _ => return Err("limit buy: provide either volume or krw_amount".into()),
            };
            // Sanity: notional must clear Upbit minimum.
            if volume * price < 5_000.0 {
                return Err(format!(
                    "limit buy notional too small: {:.8} × {:.0} = {:.0} KRW (min 5,000)",
                    volume, price, volume * price
                ));
            }
            client.place_limit_buy_typed(&args.market, volume, price).await?
        }
        ("sell", "limit") => {
            let vol = args.volume.ok_or("volume required for limit sell")?;
            let price = args.limit_price.ok_or("limit_price required for limit sell")?;
            if vol <= 0.0 || price <= 0.0 {
                return Err("limit sell: volume and limit_price must be > 0".into());
            }
            client.place_limit_sell_typed(&args.market, vol, price).await?
        }
        (side, ord) => return Err(format!("invalid (side, ord_type) = ({}, {})", side, ord)),
    };

    // Optionally write to live_trades so the History page picks it up.
    // Limit orders that come back wait (unfilled) are NOT booked — sitting
    // in Upbit's book, not yet a real fill. User can verify via Upbit app.
    // Market orders typically return done immediately, but if the immediate
    // response has executed_volume=0 (still settling), we still book using
    // estimated values so the user sees the trade in history.
    if let Some(sid) = args.session_id {
        let executed = order.executed_volume_f64();
        let is_limit_unfilled = args.ord_type == "limit" && !order.is_done();
        if is_limit_unfilled {
            // Limit registered but not filled — leave for user to track on Upbit.
        } else {
            // For limit-done we know the fill price; for market we use ticker
            // as an estimate (Upbit market response often omits avg price).
            let current_price = client.get_current_price(&args.market).await
                .unwrap_or(0.0);
            let booked_price = if args.ord_type == "limit" {
                args.limit_price.unwrap_or(current_price)
            } else {
                current_price
            };
            let fee_rate = 0.0005;
            let booked_volume = if executed > 0.0 {
                executed
            } else if args.side == "buy" {
                let krw = args.krw_amount.unwrap_or(0.0);
                krw / booked_price.max(1.0) * (1.0 - fee_rate)
            } else {
                args.volume.unwrap_or(0.0)
            };

            if booked_volume > 0.0 && booked_price > 0.0 {
                let now_ts = chrono::Utc::now().to_rfc3339();
                let conn = state.db.lock().map_err(|e| e.to_string())?;

                let (pnl, pnl_pct) = if args.side == "sell" {
                    let buy_price: f64 = conn.query_row(
                        "SELECT COALESCE(current_buy_price, 0) FROM live_sessions WHERE id = ?1",
                        [sid],
                        |r| r.get(0),
                    ).unwrap_or(0.0);
                    if buy_price > 0.0 {
                        let pnl = (booked_price - buy_price) * booked_volume;
                        // 분수형(0.0064 = 0.64%). live_trades.pnl_pct는 paper 전략과
                        // session_engine real_sell, pending_order_tracker late_sell이
                        // 모두 분수형이므로 manual_sell도 동일 단위로 통일.
                        // 차트(CandleChart.tsx)가 ×100해서 표시.
                        let pnl_pct = (booked_price - buy_price) / buy_price;
                        (Some(pnl), Some(pnl_pct))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };

                // signal: manual_buy / manual_sell / manual_buy_limit / manual_sell_limit
                let signal_name = match (args.side.as_str(), args.ord_type.as_str()) {
                    ("buy", "limit") => "manual_buy_limit",
                    ("sell", "limit") => "manual_sell_limit",
                    ("buy", _) => "manual_buy",
                    ("sell", _) => "manual_sell",
                    _ => "manual_other",
                };
                crate::db::live_repo::insert_trade(
                    &conn, sid, &now_ts, &args.side,
                    booked_price, booked_volume,
                    booked_price * booked_volume * fee_rate,
                    signal_name, pnl, pnl_pct, true, // is_real=1
                ).map_err(|e| format!("DB insert failed: {e}"))?;
            }
        }
    }

    Ok(ManualOrderResult {
        uuid: order.uuid.clone(),
        state: order.state.clone(),
        executed_volume: order.executed_volume_f64(),
        side: args.side,
        market: args.market,
    })
}

#[tauri::command]
pub fn get_position(market: String, state: State<'_, AppState>) -> Result<PositionInfo, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let result = conn.query_row(
        "SELECT status, COALESCE(buy_price, 0), COALESCE(buy_volume, 0) FROM positions WHERE market = ?1 AND user_id = 1",
        [&market],
        |row| {
            Ok(PositionInfo {
                status: row.get(0)?,
                buy_price: row.get(1)?,
                buy_volume: row.get(2)?,
                pnl_pct: 0.0, // calculated client-side with current price
            })
        },
    );

    match result {
        Ok(info) => Ok(info),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(PositionInfo {
            status: "idle".to_string(),
            buy_price: 0.0,
            buy_volume: 0.0,
            pnl_pct: 0.0,
        }),
        Err(e) => Err(e.to_string()),
    }
}

// ─── Auto-trading commands ───

#[tauri::command]
pub async fn start_auto_trading(
    market: String,
    strategy_key: String,
    account_id: i64,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Check if already running
    {
        let guard = state.auto_trading.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("Auto-trading is already running. Stop it first.".into());
        }
    }

    // Validate strategy exists
    state
        .registry
        .get(&strategy_key)
        .ok_or_else(|| format!("Strategy '{}' not found", strategy_key))?;

    let cancel_token = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel_token.clone();

    // Store handle
    {
        let mut guard = state.auto_trading.lock().map_err(|e| e.to_string())?;
        *guard = Some(AutoTradingHandle {
            cancel_token: cancel_token.clone(),
            market: market.clone(),
            strategy_key: strategy_key.clone(),
        });
    }

    // Create a new DB connection for the background task
    let db_path = dirs_db_path();
    let conn = crate::db::schema::initialize(&db_path)
        .map_err(|e| format!("Failed to open DB for auto-trading: {}", e))?;
    let db = Arc::new(Mutex::new(conn));

    let params = TradingParameters::default();
    let market_clone = market.clone();
    let strategy_key_clone = strategy_key.clone();

    // Spawn the auto-trading loop — creates its own StrategyRegistry (Send-safe)
    tauri::async_runtime::spawn(async move {
        let registry = crate::strategies::StrategyRegistry::new();
        auto_trader::run_loop(
            app_handle,
            db,
            registry,
            strategy_key_clone,
            market_clone,
            params,
            cancel_clone,
            account_id,
        )
        .await;
    });

    Ok(format!("Auto-trading started: {} / {}", market, strategy_key))
}

#[tauri::command]
pub async fn stop_auto_trading(state: State<'_, AppState>) -> Result<String, String> {
    let mut guard = state.auto_trading.lock().map_err(|e| e.to_string())?;
    match guard.take() {
        Some(handle) => {
            handle.cancel_token.store(true, Ordering::Relaxed);
            Ok(format!(
                "Auto-trading stop requested: {} / {}",
                handle.market, handle.strategy_key
            ))
        }
        None => Err("Auto-trading is not running".into()),
    }
}

#[tauri::command]
pub fn get_auto_trading_status(
    state: State<'_, AppState>,
) -> Result<auto_trader::AutoTradeStatus, String> {
    let guard = state.auto_trading.lock().map_err(|e| e.to_string())?;
    match &*guard {
        Some(handle) => Ok(auto_trader::AutoTradeStatus {
            running: !handle.cancel_token.load(Ordering::Relaxed),
            market: handle.market.clone(),
            strategy: handle.strategy_key.clone(),
            last_signal: String::new(),
            last_check: String::new(),
        }),
        None => Ok(auto_trader::AutoTradeStatus {
            running: false,
            market: String::new(),
            strategy: String::new(),
            last_signal: String::new(),
            last_check: String::new(),
        }),
    }
}

fn dirs_db_path() -> std::path::PathBuf {
    let mut path = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    path.push("bitcoin-trader");
    std::fs::create_dir_all(&path).ok();
    path.push("bitcoin_trader.db");
    path
}
