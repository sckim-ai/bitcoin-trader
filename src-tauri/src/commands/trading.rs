use crate::api::upbit::UpbitClient;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
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

#[tauri::command]
pub async fn get_current_price(market: String) -> Result<f64, String> {
    let client = create_client()?;
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
