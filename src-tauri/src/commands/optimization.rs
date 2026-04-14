use crate::core::indicators;
use crate::core::optimizer::{get_parameter, Nsga2Optimizer};
use crate::migration::csv_import;
use crate::models::config::OptimizerConfig;
use crate::models::market::MarketData;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParetoSolution {
    pub objectives: Vec<f64>,
    pub parameters: HashMap<String, f64>,
    pub rank: usize,
    pub crowding_distance: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizationConfig {
    pub population_size: Option<usize>,
    pub generations: Option<usize>,
    pub crossover_rate: Option<f64>,
    pub mutation_rate: Option<f64>,
    pub objectives: Option<Vec<String>>,
    pub min_win_rate: Option<f64>,
    pub min_trades: Option<usize>,
    pub min_return: Option<f64>,
}

#[tauri::command]
pub fn start_optimization(
    strategy_key: String,
    market: String,
    timeframe: String,
    config: OptimizationConfig,
    state: State<'_, AppState>,
) -> Result<Vec<ParetoSolution>, String> {
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

    let opt_config = OptimizerConfig {
        population_size: config.population_size.unwrap_or(50),
        generations: config.generations.unwrap_or(100),
        crossover_rate: config.crossover_rate.unwrap_or(0.9),
        mutation_rate: config.mutation_rate.unwrap_or(0.1),
        objectives: config.objectives.unwrap_or_default(),
        min_win_rate: config.min_win_rate.unwrap_or(0.0),
        min_trades: config.min_trades.unwrap_or(0),
        min_return: config.min_return.unwrap_or(0.0),
    };

    let optimizer = Nsga2Optimizer::new(opt_config);
    let population = optimizer.run(&data, strategy, None);

    // Extract parameter names from strategy's ranges
    let ranges = strategy.parameter_ranges();
    let param_names: Vec<String> = ranges.iter().map(|r| r.name.clone()).collect();

    // Convert Individual -> ParetoSolution
    let solutions: Vec<ParetoSolution> = population
        .into_iter()
        .filter(|ind| ind.rank == 0) // Only Pareto front
        .map(|ind| {
            let mut parameters = HashMap::new();
            for name in &param_names {
                parameters.insert(name.clone(), get_parameter(&ind.parameters, name));
            }
            ParetoSolution {
                objectives: ind.objectives,
                parameters,
                rank: ind.rank,
                crowding_distance: ind.crowding_distance,
            }
        })
        .collect();

    Ok(solutions)
}
