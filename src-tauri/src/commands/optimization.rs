use crate::core::indicators;
use crate::core::optimizer::{
    get_parameter, GenerationResult, Individual, Nsga2Optimizer,
};
use crate::migration::csv_import;
use crate::models::config::OptimizerConfig;
use crate::models::market::MarketData;
use crate::state::{AppState, OptimizationHandle};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};

/// Seed population preserved across runs for the "Continue" button. Stored
/// in a process-global so worker threads can read/write it without needing
/// an AppHandle-rooted state reference.
static LAST_SEED: OnceLock<Mutex<Option<Vec<Individual>>>> = OnceLock::new();
fn last_seed() -> &'static Mutex<Option<Vec<Individual>>> {
    LAST_SEED.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParetoSolution {
    pub objectives: Vec<f64>,
    pub parameters: HashMap<String, f64>,
    /// Full metrics for every candidate — includes metrics that weren't
    /// NSGA-II objectives, so the UI can still render them as columns.
    #[serde(default)]
    pub metrics: HashMap<String, f64>,
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
    pub since: Option<String>,
    pub until: Option<String>,
    /// When true, reuse the population from the previous run stored in
    /// `AppState.optimization` instead of re-randomising.
    pub continue_previous: Option<bool>,
}

/// Payload emitted to the frontend at the end of every generation.
#[derive(Debug, Clone, Serialize)]
struct GenerationEvent {
    run_id: i64,
    generation: usize,
    total_generations: usize,
    best_return: f64,
    best_win_rate: f64,
    front_size: usize,
    front: Vec<ParetoSolution>,
}

/// Emitted once when the run finishes (completed, cancelled, or errored).
#[derive(Debug, Clone, Serialize)]
struct CompletionEvent {
    run_id: i64,
    status: String, // "completed" | "cancelled" | "error"
    generations_run: usize,
    final_front_size: usize,
    elapsed_ms: u128,
    error: Option<String>,
}

fn individual_to_solution(ind: &Individual, param_names: &[String]) -> ParetoSolution {
    let mut parameters = HashMap::new();
    for name in param_names {
        parameters.insert(name.clone(), get_parameter(&ind.parameters, name));
    }
    ParetoSolution {
        objectives: ind.objectives.clone(),
        parameters,
        metrics: ind.metrics.clone(),
        rank: ind.rank,
        crowding_distance: ind.crowding_distance,
    }
}

/// Kick off an NSGA-II run on a background thread. Returns the `run_id`
/// immediately; the UI should listen to `opt:gen` / `opt:done` events for
/// progress and final results.
#[tauri::command]
pub async fn start_optimization(
    strategy_key: String,
    market: String,
    timeframe: String,
    config: OptimizationConfig,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<i64, String> {
    // Reject if a run is already active — one-at-a-time keeps DB writes
    // and event streams unambiguous.
    {
        let guard = state.optimization.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("Optimization already running. Cancel it first.".into());
        }
    }

    let registry_key = strategy_key.clone();
    let _strategy = state
        .registry
        .get(&registry_key)
        .ok_or_else(|| format!("Strategy '{}' not found", registry_key))?;

    // Load market data up front (inside the Tauri command) so we can
    // release the DB lock before spawning work.
    let data: Vec<MarketData> = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        if timeframe == "hour" {
            crate::core::day_psy_store::load_market_data(
                &conn,
                &market,
                config.since.as_deref(),
                config.until.as_deref(),
            )
            .map_err(|e| e.to_string())?
        } else {
            let candles = csv_import::load_candles_range(
                &conn,
                &market,
                &timeframe,
                None,
                config.since.as_deref(),
                config.until.as_deref(),
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
        }
    };
    if data.is_empty() {
        return Err("No market data available for the selected range".into());
    }

    let opt_config = OptimizerConfig {
        population_size: config.population_size.unwrap_or(50),
        generations: config.generations.unwrap_or(100),
        crossover_rate: config.crossover_rate.unwrap_or(0.9),
        mutation_rate: config.mutation_rate.unwrap_or(0.1),
        objectives: config.objectives.clone().unwrap_or_default(),
        min_win_rate: config.min_win_rate.unwrap_or(0.0),
        min_trades: config.min_trades.unwrap_or(0),
        min_return: config.min_return.unwrap_or(0.0),
    };

    // Persist the run record upfront so even cancelled/errored runs are
    // recoverable from DB history.
    let run_id: i64 = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let objectives_json = serde_json::to_string(&opt_config.objectives).unwrap_or_default();
        let constraints_json = serde_json::to_string(&serde_json::json!({
            "min_win_rate": opt_config.min_win_rate,
            "min_trades": opt_config.min_trades,
            "min_return": opt_config.min_return,
            "since": config.since,
            "until": config.until,
            "market": market,
            "timeframe": timeframe,
        }))
        .unwrap_or_default();
        conn.execute(
            "INSERT INTO optimization_runs
             (strategy_key, population_size, generations, objectives, constraints, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
            params![
                registry_key,
                opt_config.population_size as i64,
                opt_config.generations as i64,
                objectives_json,
                constraints_json,
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };

    // Continue button: pull the last run's final population out of the
    // process-global cache. `None` if this is a fresh start or the previous
    // run never completed.
    let seed_population: Option<Vec<Individual>> = if config.continue_previous.unwrap_or(false) {
        last_seed().lock().ok().and_then(|mut g| g.take())
    } else {
        // Non-continue starts clear the cache so stale data doesn't linger.
        if let Ok(mut g) = last_seed().lock() {
            *g = None;
        }
        None
    };

    let cancel_token = Arc::new(AtomicBool::new(false));
    let last_population = Arc::new(Mutex::new(None::<Vec<Individual>>));
    let last_generation = Arc::new(Mutex::new(0usize));
    {
        let mut guard = state.optimization.lock().map_err(|e| e.to_string())?;
        *guard = Some(OptimizationHandle {
            cancel_token: cancel_token.clone(),
            run_id: Some(run_id),
            last_population: last_population.clone(),
            last_generation: last_generation.clone(),
        });
    }

    // Background task — DB handle gets re-opened inside to avoid holding
    // the main lock for the whole run. Uses a dedicated Connection.
    let db_path = crate::db::paths::local_db_path();
    let registry = crate::strategies::StrategyRegistry::new();

    tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        let strategy = registry.get(&registry_key).expect("strategy registered");
        let ranges = strategy.parameter_ranges();
        let param_names: Vec<String> = ranges.iter().map(|r| r.name.clone()).collect();

        let worker_db = rusqlite::Connection::open(&db_path).ok();
        let cancel_for_cb = cancel_token.clone();
        let app_for_cb = app_handle.clone();
        let param_names_for_cb = param_names.clone();
        let last_population_for_cb = last_population.clone();
        let last_generation_for_cb = last_generation.clone();

        let callback = |gr: &GenerationResult| {
            // 0. Maintain the `best_return` cache on optimization_runs so
            //    the Recent Runs listing stays fast (no correlated subquery).
            if let Some(conn) = worker_db.as_ref() {
                if gr.best_return.is_finite() {
                    let _ = conn.execute(
                        "UPDATE optimization_runs
                         SET best_return = COALESCE(MAX(best_return, ?1), ?1)
                         WHERE id = ?2",
                        params![gr.best_return, run_id],
                    );
                }
            }

            // 1. DB: write top-K individuals of the current Pareto front.
            if let Some(conn) = worker_db.as_ref() {
                // Keep inserts small — only rank-0 members.
                for ind in gr.front.iter().take(50) {
                    let params_map: HashMap<String, f64> = param_names_for_cb
                        .iter()
                        .map(|n| (n.clone(), get_parameter(&ind.parameters, n)))
                        .collect();
                    let params_json = serde_json::to_string(&params_map).unwrap_or_default();
                    let metrics_json = serde_json::to_string(&ind.metrics).unwrap_or_default();
                    let total_return = ind.metrics.get("total_return").copied().unwrap_or(0.0);
                    let win_rate = ind.metrics.get("win_rate").copied().unwrap_or(0.0);
                    let max_drawdown = ind.metrics.get("max_drawdown").copied().unwrap_or(0.0);
                    let total_trades = ind.metrics.get("total_trades").copied().unwrap_or(0.0) as i64;
                    let _ = conn.execute(
                        "INSERT INTO optimization_results
                         (run_id, generation, rank, parameters, total_return, win_rate,
                          max_drawdown, total_trades, crowding_distance, metrics_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![
                            run_id,
                            gr.generation as i64,
                            ind.rank as i64,
                            params_json,
                            total_return,
                            win_rate,
                            max_drawdown,
                            total_trades,
                            if ind.crowding_distance.is_finite() { ind.crowding_distance } else { 0.0 },
                            metrics_json,
                        ],
                    );
                }
            }

            // 2. Frontend event
            let front: Vec<ParetoSolution> = gr
                .front
                .iter()
                .map(|ind| individual_to_solution(ind, &param_names_for_cb))
                .collect();
            let _ = app_for_cb.emit(
                "opt:gen",
                GenerationEvent {
                    run_id,
                    generation: gr.generation,
                    total_generations: gr.total_generations,
                    best_return: gr.best_return,
                    best_win_rate: gr.best_win_rate,
                    front_size: gr.front_size,
                    front,
                },
            );

            // 3. Snapshot for Continue
            if let Ok(mut lp) = last_population_for_cb.lock() {
                *lp = Some(gr.front.to_vec());
            }
            if let Ok(mut lg) = last_generation_for_cb.lock() {
                *lg = gr.generation;
            }

            // 4. Early-exit signalling (optimizer itself polls each gen)
            let _ = cancel_for_cb; // token read inside optimizer
        };

        let optimizer = Nsga2Optimizer::new(opt_config.clone());
        let final_pop = optimizer.run_advanced(
            &data,
            strategy,
            seed_population,
            0,
            Some(cancel_token.clone()),
            Some(&callback),
        );

        let cancelled = cancel_token.load(Ordering::Relaxed);
        let status = if cancelled { "cancelled" } else { "completed" };
        let generations_run = last_generation.lock().map(|g| *g).unwrap_or(0);
        let pareto: Vec<ParetoSolution> = final_pop
            .iter()
            .filter(|i| i.rank == 0)
            .map(|ind| individual_to_solution(ind, &param_names))
            .collect();
        let final_front_size = pareto.len();

        // Store for Continue button — hold on to the final full population.
        if let Ok(mut lp) = last_population.lock() {
            *lp = Some(final_pop.clone());
        }
        // Mirror into the process-global cache so "Continue" can reseed
        // even after the AppState handle is cleared below.
        if let Ok(mut seed) = last_seed().lock() {
            *seed = Some(final_pop);
        }

        // Update run record
        if let Some(conn) = worker_db.as_ref() {
            let _ = conn.execute(
                "UPDATE optimization_runs SET status = ?1, completed_at = datetime('now') WHERE id = ?2",
                params![status, run_id],
            );
        }

        // Emit completion
        let _ = app_handle.emit(
            "opt:done",
            CompletionEvent {
                run_id,
                status: status.into(),
                generations_run,
                final_front_size,
                elapsed_ms: started.elapsed().as_millis(),
                error: None,
            },
        );

        // Clear the running-handle so a new run (or Continue) is accepted.
        // `.map()` consumes the Result immediately so the MutexGuard temporary
        // doesn't outlive the `state` binding (borrow-check issue otherwise).
        {
            let state = app_handle.state::<AppState>();
            let _ = state.optimization.lock().map(|mut g| *g = None);
        }
    });

    Ok(run_id)
}

/// Signal the active optimization to stop at the next generation boundary.
#[tauri::command]
pub fn cancel_optimization(state: State<'_, AppState>) -> Result<bool, String> {
    let guard = state.optimization.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = guard.as_ref() {
        handle.cancel_token.store(true, Ordering::Relaxed);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Delete a single optimization run and all its stored generations.
/// Rejects the delete if `run_id` is currently running (the UI should
/// disable the button in that case, but we guard server-side too).
#[tauri::command]
pub fn delete_optimization_run(
    state: State<'_, AppState>,
    run_id: i64,
) -> Result<(), String> {
    // Refuse to delete the active run — would leave the OptimizationHandle
    // pointing to dropped DB rows and produce misleading events.
    {
        let guard = state.optimization.lock().map_err(|e| e.to_string())?;
        if let Some(h) = guard.as_ref() {
            if h.run_id == Some(run_id) {
                return Err("Cannot delete a currently-running optimization.".into());
            }
        }
    }
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM optimization_results WHERE run_id = ?1",
        params![run_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM optimization_runs WHERE id = ?1",
        params![run_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Basic introspection — is a run active right now?
#[tauri::command]
pub fn get_optimization_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.optimization.lock().map_err(|e| e.to_string())?;
    Ok(match guard.as_ref() {
        Some(h) => {
            let gen = h.last_generation.lock().map(|g| *g).unwrap_or(0);
            serde_json::json!({
                "running": true,
                "run_id": h.run_id,
                "last_generation": gen,
            })
        }
        None => serde_json::json!({ "running": false }),
    })
}

/// List past optimization runs for the history panel.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationRunSummary {
    pub id: i64,
    pub strategy_key: String,
    pub population_size: i64,
    pub generations: i64,
    pub objectives: String,
    /// Serialized JSON produced at insert time — contains `market`,
    /// `timeframe`, `since`, `until`, and the min_* thresholds. The UI uses
    /// it to re-hydrate the form when loading a past run so that
    /// "Apply → Simulation" carries the same context the run was built with.
    pub constraints: Option<String>,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub best_return: Option<f64>,
}

#[tauri::command]
pub fn list_optimization_runs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<OptimizationRunSummary>, String> {
    let limit = limit.unwrap_or(50);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // `best_return` is now a cached column on optimization_runs — updated
    // by the generation callback every time a new peak is observed. This
    // turns the listing query from a 400ms+ correlated subquery into a
    // plain indexed read against the runs table.
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.strategy_key, r.population_size, r.generations,
                    r.objectives, r.constraints, r.status, r.started_at,
                    r.completed_at, r.best_return
             FROM optimization_runs r
             ORDER BY r.id DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(OptimizationRunSummary {
                id: row.get(0)?,
                strategy_key: row.get(1)?,
                population_size: row.get(2)?,
                generations: row.get(3)?,
                objectives: row.get(4)?,
                constraints: row.get(5)?,
                status: row.get(6)?,
                started_at: row.get(7)?,
                completed_at: row.get(8)?,
                best_return: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let out: Vec<OptimizationRunSummary> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// A single generation's Pareto front plus the run's true maximum stored
/// generation. The UI draws the chart from `solutions` and uses
/// `max_generation` to drive the scrub slider's upper bound — loading only
/// one generation at a time makes the initial "Load" response sub-second
/// even when the run has ~250k stored solutions across 5000 generations.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationGenerationView {
    pub generation: i64,
    pub max_generation: i64,
    pub solutions: Vec<ParetoSolution>,
}

/// Fetch one generation's Pareto front on demand.
/// * `generation = None` → returns the latest (MAX) generation's front. The
///   UI calls this on Load.
/// * `generation = Some(g)` → returns generation `g`'s front. The UI calls
///   this as the slider scrubs to an un-cached generation.
#[tauri::command]
pub fn get_optimization_run_generation(
    state: State<'_, AppState>,
    run_id: i64,
    generation: Option<i64>,
) -> Result<OptimizationGenerationView, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let max_gen: Option<i64> = conn
        .query_row(
            "SELECT MAX(generation) FROM optimization_results WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let Some(max_gen) = max_gen else {
        return Ok(OptimizationGenerationView {
            generation: 0,
            max_generation: 0,
            solutions: Vec::new(),
        });
    };

    let target_gen = generation.unwrap_or(max_gen);

    // Read the run's selected objectives so we can reconstruct the
    // objectives vec in its original order (legacy rows had objectives[2..]
    // in memory but never persisted — we only recover the first two from
    // DB columns, but at least label them correctly).
    let objectives_list: Vec<String> = conn
        .query_row(
            "SELECT objectives FROM optimization_runs WHERE id = ?1",
            params![run_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let mut stmt = conn
        .prepare(
            "SELECT rank, parameters, total_return, win_rate, crowding_distance,
                    metrics_json, total_trades, max_drawdown
             FROM optimization_results WHERE run_id = ?1 AND generation = ?2
             ORDER BY total_return DESC",
        )
        .map_err(|e| e.to_string())?;
    let solutions: Vec<ParetoSolution> = stmt
        .query_map(params![run_id, target_gen], |row| {
            let rank: i64 = row.get(0)?;
            let params_json: String = row.get(1)?;
            let total_return: f64 = row.get(2)?;
            let win_rate: f64 = row.get(3)?;
            let crowding: f64 = row.get(4)?;
            let metrics_json: Option<String> = row.get(5)?;
            let total_trades: i64 = row.get(6).unwrap_or(0);
            let max_drawdown: f64 = row.get(7).unwrap_or(0.0);

            let parameters: HashMap<String, f64> =
                serde_json::from_str(&params_json).unwrap_or_default();

            let mut metrics: HashMap<String, f64> = HashMap::new();
            metrics.insert("total_return".into(), total_return);
            metrics.insert("win_rate".into(), win_rate);
            metrics.insert("total_trades".into(), total_trades as f64);
            metrics.insert("max_drawdown".into(), max_drawdown);
            if let Some(blob) = metrics_json.as_deref() {
                if let Ok(full) = serde_json::from_str::<HashMap<String, f64>>(blob) {
                    for (k, v) in full {
                        metrics.insert(k, v);
                    }
                }
            }

            let objectives: Vec<f64> = if objectives_list.is_empty() {
                vec![total_return, win_rate]
            } else {
                objectives_list
                    .iter()
                    .map(|name| metrics.get(name).copied().unwrap_or(0.0))
                    .collect()
            };

            Ok(ParetoSolution {
                objectives,
                parameters,
                metrics,
                rank: rank as usize,
                crowding_distance: crowding,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(OptimizationGenerationView {
        generation: target_gen,
        max_generation: max_gen,
        solutions,
    })
}
