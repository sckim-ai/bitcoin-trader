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
    let access_key =
        std::env::var("UPBIT_ACCESS_KEY").map_err(|_| "UPBIT_ACCESS_KEY not set".to_string())?;
    let secret_key =
        std::env::var("UPBIT_SECRET_KEY").map_err(|_| "UPBIT_SECRET_KEY not set".to_string())?;
    Ok(UpbitClient::new(access_key, secret_key))
}

/// Public ticker/candle endpoints don't need auth — keys default to empty if unset.
fn create_public_client() -> UpbitClient {
    let access_key = std::env::var("UPBIT_ACCESS_KEY").unwrap_or_default();
    let secret_key = std::env::var("UPBIT_SECRET_KEY").unwrap_or_default();
    UpbitClient::new(access_key, secret_key)
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
