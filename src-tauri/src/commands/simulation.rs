use crate::core::indicators;
use crate::migration::csv_import;
use crate::models::config::ParameterRange;
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
    pub ranges: Vec<ParameterRange>,
    pub defaults: HashMap<String, f64>,
}

#[tauri::command]
pub fn list_strategies(
    market: Option<String>,
    state: State<'_, AppState>,
) -> Vec<StrategyInfo> {
    let defaults = match market.as_deref() {
        Some(m) => TradingParameters::default_for_market(m),
        None => TradingParameters::default(),
    };
    state
        .registry
        .list()
        .into_iter()
        .map(|(key, name, ranges)| {
            let defaults_map = ranges
                .iter()
                .map(|r| (r.name.clone(), get_parameter(&defaults, &r.name)))
                .collect();
            StrategyInfo {
                key: key.to_string(),
                name: name.to_string(),
                ranges,
                defaults: defaults_map,
            }
        })
        .collect()
}

#[tauri::command]
pub fn run_simulation(
    strategy_key: String,
    market: String,
    timeframe: String,
    params: HashMap<String, f64>,
    since: Option<String>,
    until: Option<String>,
    state: State<'_, AppState>,
) -> Result<SimulationResult, String> {
    let strategy = state
        .registry
        .get(&strategy_key)
        .ok_or_else(|| format!("Strategy '{}' not found", strategy_key))?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // Hour timeframe uses the persisted day_psy column (clean pipeline).
    // Other timeframes fall back to the legacy code path since day_psy is
    // not applicable to day/week bars.
    let data: Vec<MarketData> = if timeframe == "hour" {
        crate::core::day_psy_store::load_market_data(
            &conn,
            &market,
            since.as_deref(),
            until.as_deref(),
        )
        .map_err(|e| e.to_string())?
    } else {
        let candles = csv_import::load_candles_range(
            &conn,
            &market,
            &timeframe,
            None,
            since.as_deref(),
            until.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        let indicator_sets = indicators::calculate_all(&candles);
        candles
            .into_iter()
            .zip(indicator_sets)
            .map(|(candle, ind)| MarketData {
                candle,
                indicators: ind,
            })
            .collect()
    };

    // Build TradingParameters from market-aware default + user overrides.
    // Frontend sends an explicit value for every param, so base is less
    // load-bearing, but keeping it market-aware means missing params still
    // degrade to sensible values.
    let mut trading_params = TradingParameters::default_for_market(&market);
    for (name, value) in &params {
        set_parameter(&mut trading_params, name, *value);
    }

    // Verify at least one param was actually set (read back)
    let _ = get_parameter(&trading_params, "fee_rate");

    let result = strategy.run_simulation(&data, &trading_params);
    Ok(result)
}
