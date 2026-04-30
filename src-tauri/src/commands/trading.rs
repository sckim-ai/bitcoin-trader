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
    pub krw_amount: Option<f64>,   // required for buy (Upbit ord_type="price")
    pub volume: Option<f64>,       // required for sell (Upbit ord_type="market")
    pub session_id: Option<i64>,   // None → don't write to live_trades
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

    // Sanity: which side + amount?
    let order = match args.side.as_str() {
        "buy" => {
            let krw = args.krw_amount
                .ok_or("krw_amount required for buy")?;
            if krw < 5_000.0 {
                return Err(format!("Upbit minimum 5,000 KRW (got {:.0})", krw));
            }
            client.place_market_buy(&args.market, krw).await?
        }
        "sell" => {
            let vol = args.volume.ok_or("volume required for sell")?;
            if vol <= 0.0 {
                return Err("volume must be > 0".into());
            }
            client.place_market_sell(&args.market, vol).await?
        }
        other => return Err(format!("invalid side: {other}")),
    };

    // Optionally write to live_trades so the History page picks it up.
    if let Some(sid) = args.session_id {
        // We need a price reference for the booked row. Market buys often
        // return wait/done with executed_volume = 0 in the immediate
        // response; pull current ticker as a best-effort fill price.
        let current_price = client.get_current_price(&args.market).await
            .unwrap_or(0.0);
        let executed = order.executed_volume_f64();
        let fee_rate = 0.0005;
        let now_ts = chrono::Utc::now().to_rfc3339();
        let conn = state.db.lock().map_err(|e| e.to_string())?;

        let booked_volume = if executed > 0.0 {
            executed
        } else if args.side == "buy" {
            // Estimate from KRW / price.
            args.krw_amount.unwrap_or(0.0) / current_price.max(1.0) * (1.0 - fee_rate)
        } else {
            args.volume.unwrap_or(0.0)
        };

        if booked_volume > 0.0 && current_price > 0.0 {
            // For sell, compute pnl against the session's stored buy_price
            // (best effort — manual orders bypass the session's strategy
            // so the attribution may not be perfect).
            let (pnl, pnl_pct) = if args.side == "sell" {
                let buy_price: f64 = conn.query_row(
                    "SELECT COALESCE(current_buy_price, 0) FROM live_sessions WHERE id = ?1",
                    [sid],
                    |r| r.get(0),
                ).unwrap_or(0.0);
                if buy_price > 0.0 {
                    let pnl = (current_price - buy_price) * booked_volume;
                    let pnl_pct = (current_price - buy_price) / buy_price * 100.0;
                    (Some(pnl), Some(pnl_pct))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            let signal_name = if args.side == "buy" { "manual_buy" } else { "manual_sell" };
            crate::db::live_repo::insert_trade(
                &conn, sid, &now_ts, &args.side,
                current_price, booked_volume,
                current_price * booked_volume * fee_rate,
                signal_name, pnl, pnl_pct, true, // is_real=1
            ).map_err(|e| format!("DB insert failed: {e}"))?;
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
