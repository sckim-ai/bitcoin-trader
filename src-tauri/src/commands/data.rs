use crate::core::indicators;
use crate::migration::csv_import;
use crate::models::market::{Candle, MarketData};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn load_csv_data(
    csv_path: String,
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&csv_path);
    csv_import::import_csv(&conn, path, &market, &timeframe).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_candles(
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<Vec<Candle>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    csv_import::load_candles(&conn, &market, &timeframe).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_market_data(
    market: String,
    timeframe: String,
    state: State<'_, AppState>,
) -> Result<Vec<MarketData>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let candles =
        csv_import::load_candles(&conn, &market, &timeframe).map_err(|e| e.to_string())?;
    let indicator_sets = indicators::calculate_all(&candles);

    let data: Vec<MarketData> = candles
        .into_iter()
        .zip(indicator_sets)
        .map(|(candle, ind)| MarketData {
            candle,
            indicators: ind,
        })
        .collect();

    Ok(data)
}
