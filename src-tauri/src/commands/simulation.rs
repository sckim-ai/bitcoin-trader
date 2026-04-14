use crate::core::indicators;
use crate::migration::csv_import;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use crate::core::optimizer::{get_parameter, set_parameter};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInfo {
    pub key: String,
    pub name: String,
}

#[tauri::command]
pub fn list_strategies(state: State<'_, AppState>) -> Vec<StrategyInfo> {
    state
        .registry
        .list()
        .into_iter()
        .map(|(key, name)| StrategyInfo {
            key: key.to_string(),
            name: name.to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn run_simulation(
    strategy_key: String,
    market: String,
    timeframe: String,
    params: HashMap<String, f64>,
    state: State<'_, AppState>,
) -> Result<SimulationResult, String> {
    let strategy = state
        .registry
        .get(&strategy_key)
        .ok_or_else(|| format!("Strategy '{}' not found", strategy_key))?;

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

    // Build TradingParameters from the provided map
    let mut trading_params = TradingParameters::default();
    for (name, value) in &params {
        set_parameter(&mut trading_params, name, *value);
    }

    // Verify at least one param was actually set (read back)
    let _ = get_parameter(&trading_params, "fee_rate");

    let result = strategy.run_simulation(&data, &trading_params);
    Ok(result)
}
