use crate::models::config::{OptimizerConfig, ParameterRange};
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use crate::strategies::Strategy;
use rand::Rng;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Individual {
    pub objectives: Vec<f64>,
    pub rank: usize,
    pub crowding_distance: f64,
    pub domination_count: usize,
    pub dominated_solutions: Vec<usize>,
    pub constraint_violation: f64,
    pub parameters: TradingParameters,
    /// Full snapshot of every metric the SimulationResult produced — not
    /// just the ones selected as objectives. Lets the UI show all columns
    /// (total_return, win_rate, max_drawdown, profit_factor, sharpe_ratio,
    /// sortino_ratio, total_trades) even if the run only optimised two.
    pub metrics: HashMap<String, f64>,
}

impl Individual {
    pub fn new(params: TradingParameters) -> Self {
        Self {
            objectives: Vec::new(),
            rank: 0,
            crowding_distance: 0.0,
            domination_count: 0,
            dominated_solutions: Vec::new(),
            constraint_violation: 0.0,
            parameters: params,
            metrics: HashMap::new(),
        }
    }
}

/// Per-generation snapshot emitted via callback so callers (Tauri event
/// bus, DB writer, tests) can react at each step.
#[derive(Debug, Clone)]
pub struct GenerationResult<'a> {
    pub generation: usize,
    pub total_generations: usize,
    pub best_return: f64,
    pub best_win_rate: f64,
    pub front_size: usize,
    /// Reference to the current Pareto front (rank 0) for realtime
    /// rendering. Borrow is short — callback must not retain across calls.
    pub front: &'a [Individual],
}

pub struct Nsga2Optimizer {
    pub config: OptimizerConfig,
}

// ─── get/set parameter by name ───

pub fn get_parameter(params: &TradingParameters, name: &str) -> f64 {
    match name {
        "v3_urgent_buy_volume_lo" => params.v3_urgent_buy_volume_lo,
        "v3_urgent_buy_volume_hi" => params.v3_urgent_buy_volume_hi,
        "v3_urgent_buy_volume_pow" => params.v3_urgent_buy_volume_pow,
        "v3_buy_volume_lo" => params.v3_buy_volume_lo,
        "v3_buy_volume_hi" => params.v3_buy_volume_hi,
        "v3_buy_volume_pow" => params.v3_buy_volume_pow,
        "v3_buy_price_drop_lo" => params.v3_buy_price_drop_lo,
        "v3_buy_price_drop_hi" => params.v3_buy_price_drop_hi,
        "v3_buy_price_drop_pow" => params.v3_buy_price_drop_pow,
        "v3_buy_decay_lo" => params.v3_buy_decay_lo,
        "v3_buy_decay_hi" => params.v3_buy_decay_hi,
        "v3_buy_decay_pow" => params.v3_buy_decay_pow,
        "v3_buy_psy_lo" => params.v3_buy_psy_lo,
        "v3_buy_psy_hi" => params.v3_buy_psy_hi,
        "v3_buy_psy_pow" => params.v3_buy_psy_pow,
        "v3_buy_wait_lo" => params.v3_buy_wait_lo,
        "v3_buy_wait_hi" => params.v3_buy_wait_hi,
        "v3_buy_wait_pow" => params.v3_buy_wait_pow,
        "v3_sell_stop_loss_lo" => params.v3_sell_stop_loss_lo,
        "v3_sell_stop_loss_hi" => params.v3_sell_stop_loss_hi,
        "v3_sell_stop_loss_pow" => params.v3_sell_stop_loss_pow,
        "v3_sell_profit_lo" => params.v3_sell_profit_lo,
        "v3_sell_profit_hi" => params.v3_sell_profit_hi,
        "v3_sell_profit_pow" => params.v3_sell_profit_pow,
        "v3_sell_volume_lo" => params.v3_sell_volume_lo,
        "v3_sell_volume_hi" => params.v3_sell_volume_hi,
        "v3_sell_volume_pow" => params.v3_sell_volume_pow,
        "v3_sell_decay_lo" => params.v3_sell_decay_lo,
        "v3_sell_decay_hi" => params.v3_sell_decay_hi,
        "v3_sell_decay_pow" => params.v3_sell_decay_pow,
        "v3_sell_fixed_sl_lo" => params.v3_sell_fixed_sl_lo,
        "v3_sell_fixed_sl_hi" => params.v3_sell_fixed_sl_hi,
        "v3_sell_fixed_sl_pow" => params.v3_sell_fixed_sl_pow,
        "v3_sell_max_hold_lo" => params.v3_sell_max_hold_lo,
        "v3_sell_max_hold_hi" => params.v3_sell_max_hold_hi,
        "v3_sell_max_hold_pow" => params.v3_sell_max_hold_pow,
        "v3_fee_rate" => params.v3_fee_rate,
        "v3_min_hold_bars" => params.v3_min_hold_bars as f64,
        "v3_volume_lookback" => params.v3_volume_lookback as f64,
        "v31_urgent_buy_tv_lo" => params.v31_urgent_buy_tv_lo,
        "v31_urgent_buy_tv_hi" => params.v31_urgent_buy_tv_hi,
        "v31_urgent_buy_tv_pow" => params.v31_urgent_buy_tv_pow,
        "v31_buy_tv_lo" => params.v31_buy_tv_lo,
        "v31_buy_tv_hi" => params.v31_buy_tv_hi,
        "v31_buy_tv_pow" => params.v31_buy_tv_pow,
        "v31_buy_price_drop_lo" => params.v31_buy_price_drop_lo,
        "v31_buy_price_drop_hi" => params.v31_buy_price_drop_hi,
        "v31_buy_price_drop_pow" => params.v31_buy_price_drop_pow,
        "v31_buy_decay_lo" => params.v31_buy_decay_lo,
        "v31_buy_decay_hi" => params.v31_buy_decay_hi,
        "v31_buy_decay_pow" => params.v31_buy_decay_pow,
        "v31_buy_psy_lo" => params.v31_buy_psy_lo,
        "v31_buy_psy_hi" => params.v31_buy_psy_hi,
        "v31_buy_psy_pow" => params.v31_buy_psy_pow,
        "v31_buy_wait_lo" => params.v31_buy_wait_lo,
        "v31_buy_wait_hi" => params.v31_buy_wait_hi,
        "v31_buy_wait_pow" => params.v31_buy_wait_pow,
        "v31_sell_stop_loss_lo" => params.v31_sell_stop_loss_lo,
        "v31_sell_stop_loss_hi" => params.v31_sell_stop_loss_hi,
        "v31_sell_stop_loss_pow" => params.v31_sell_stop_loss_pow,
        "v31_sell_profit_lo" => params.v31_sell_profit_lo,
        "v31_sell_profit_hi" => params.v31_sell_profit_hi,
        "v31_sell_profit_pow" => params.v31_sell_profit_pow,
        "v31_sell_tv_lo" => params.v31_sell_tv_lo,
        "v31_sell_tv_hi" => params.v31_sell_tv_hi,
        "v31_sell_tv_pow" => params.v31_sell_tv_pow,
        "v31_sell_decay_lo" => params.v31_sell_decay_lo,
        "v31_sell_decay_hi" => params.v31_sell_decay_hi,
        "v31_sell_decay_pow" => params.v31_sell_decay_pow,
        "v31_sell_fixed_sl_lo" => params.v31_sell_fixed_sl_lo,
        "v31_sell_fixed_sl_hi" => params.v31_sell_fixed_sl_hi,
        "v31_sell_fixed_sl_pow" => params.v31_sell_fixed_sl_pow,
        "v31_sell_max_hold_lo" => params.v31_sell_max_hold_lo,
        "v31_sell_max_hold_hi" => params.v31_sell_max_hold_hi,
        "v31_sell_max_hold_pow" => params.v31_sell_max_hold_pow,
        "v31_fee_rate" => params.v31_fee_rate,
        "v31_min_hold_bars" => params.v31_min_hold_bars as f64,
        "v31_volume_lookback" => params.v31_volume_lookback as f64,
        "v31_cutoff_tv_mult" => params.v31_cutoff_tv_mult,
        "v31_urgent_sell_tv_mult" => params.v31_urgent_sell_tv_mult,
        "v31_sell_ready_price_rise" => params.v31_sell_ready_price_rise,
        "v31_sell_wait_max" => params.v31_sell_wait_max as f64,
        _ => 0.0,
    }
}

pub fn set_parameter(params: &mut TradingParameters, name: &str, value: f64) {
    match name {
        "v3_urgent_buy_volume_lo" => params.v3_urgent_buy_volume_lo = value,
        "v3_urgent_buy_volume_hi" => params.v3_urgent_buy_volume_hi = value,
        "v3_urgent_buy_volume_pow" => params.v3_urgent_buy_volume_pow = value,
        "v3_buy_volume_lo" => params.v3_buy_volume_lo = value,
        "v3_buy_volume_hi" => params.v3_buy_volume_hi = value,
        "v3_buy_volume_pow" => params.v3_buy_volume_pow = value,
        "v3_buy_price_drop_lo" => params.v3_buy_price_drop_lo = value,
        "v3_buy_price_drop_hi" => params.v3_buy_price_drop_hi = value,
        "v3_buy_price_drop_pow" => params.v3_buy_price_drop_pow = value,
        "v3_buy_decay_lo" => params.v3_buy_decay_lo = value,
        "v3_buy_decay_hi" => params.v3_buy_decay_hi = value,
        "v3_buy_decay_pow" => params.v3_buy_decay_pow = value,
        "v3_buy_psy_lo" => params.v3_buy_psy_lo = value,
        "v3_buy_psy_hi" => params.v3_buy_psy_hi = value,
        "v3_buy_psy_pow" => params.v3_buy_psy_pow = value,
        "v3_buy_wait_lo" => params.v3_buy_wait_lo = value,
        "v3_buy_wait_hi" => params.v3_buy_wait_hi = value,
        "v3_buy_wait_pow" => params.v3_buy_wait_pow = value,
        "v3_sell_stop_loss_lo" => params.v3_sell_stop_loss_lo = value,
        "v3_sell_stop_loss_hi" => params.v3_sell_stop_loss_hi = value,
        "v3_sell_stop_loss_pow" => params.v3_sell_stop_loss_pow = value,
        "v3_sell_profit_lo" => params.v3_sell_profit_lo = value,
        "v3_sell_profit_hi" => params.v3_sell_profit_hi = value,
        "v3_sell_profit_pow" => params.v3_sell_profit_pow = value,
        "v3_sell_volume_lo" => params.v3_sell_volume_lo = value,
        "v3_sell_volume_hi" => params.v3_sell_volume_hi = value,
        "v3_sell_volume_pow" => params.v3_sell_volume_pow = value,
        "v3_sell_decay_lo" => params.v3_sell_decay_lo = value,
        "v3_sell_decay_hi" => params.v3_sell_decay_hi = value,
        "v3_sell_decay_pow" => params.v3_sell_decay_pow = value,
        "v3_sell_fixed_sl_lo" => params.v3_sell_fixed_sl_lo = value,
        "v3_sell_fixed_sl_hi" => params.v3_sell_fixed_sl_hi = value,
        "v3_sell_fixed_sl_pow" => params.v3_sell_fixed_sl_pow = value,
        "v3_sell_max_hold_lo" => params.v3_sell_max_hold_lo = value,
        "v3_sell_max_hold_hi" => params.v3_sell_max_hold_hi = value,
        "v3_sell_max_hold_pow" => params.v3_sell_max_hold_pow = value,
        "v3_fee_rate" => params.v3_fee_rate = value,
        // 정수 파라미터: truncation(`as i32`) 대신 반올림. NSGA-II는 연속 공간에서
        // 섭동하므로 3.9 → 3(trunc)처럼 음의 편향이 생기는 걸 방지. 3.9 → 4(round).
        "v3_min_hold_bars" => params.v3_min_hold_bars = value.round() as i32,
        "v3_volume_lookback" => params.v3_volume_lookback = value.round() as i32,
        "v31_urgent_buy_tv_lo" => params.v31_urgent_buy_tv_lo = value,
        "v31_urgent_buy_tv_hi" => params.v31_urgent_buy_tv_hi = value,
        "v31_urgent_buy_tv_pow" => params.v31_urgent_buy_tv_pow = value,
        "v31_buy_tv_lo" => params.v31_buy_tv_lo = value,
        "v31_buy_tv_hi" => params.v31_buy_tv_hi = value,
        "v31_buy_tv_pow" => params.v31_buy_tv_pow = value,
        "v31_buy_price_drop_lo" => params.v31_buy_price_drop_lo = value,
        "v31_buy_price_drop_hi" => params.v31_buy_price_drop_hi = value,
        "v31_buy_price_drop_pow" => params.v31_buy_price_drop_pow = value,
        "v31_buy_decay_lo" => params.v31_buy_decay_lo = value,
        "v31_buy_decay_hi" => params.v31_buy_decay_hi = value,
        "v31_buy_decay_pow" => params.v31_buy_decay_pow = value,
        "v31_buy_psy_lo" => params.v31_buy_psy_lo = value,
        "v31_buy_psy_hi" => params.v31_buy_psy_hi = value,
        "v31_buy_psy_pow" => params.v31_buy_psy_pow = value,
        "v31_buy_wait_lo" => params.v31_buy_wait_lo = value,
        "v31_buy_wait_hi" => params.v31_buy_wait_hi = value,
        "v31_buy_wait_pow" => params.v31_buy_wait_pow = value,
        "v31_sell_stop_loss_lo" => params.v31_sell_stop_loss_lo = value,
        "v31_sell_stop_loss_hi" => params.v31_sell_stop_loss_hi = value,
        "v31_sell_stop_loss_pow" => params.v31_sell_stop_loss_pow = value,
        "v31_sell_profit_lo" => params.v31_sell_profit_lo = value,
        "v31_sell_profit_hi" => params.v31_sell_profit_hi = value,
        "v31_sell_profit_pow" => params.v31_sell_profit_pow = value,
        "v31_sell_tv_lo" => params.v31_sell_tv_lo = value,
        "v31_sell_tv_hi" => params.v31_sell_tv_hi = value,
        "v31_sell_tv_pow" => params.v31_sell_tv_pow = value,
        "v31_sell_decay_lo" => params.v31_sell_decay_lo = value,
        "v31_sell_decay_hi" => params.v31_sell_decay_hi = value,
        "v31_sell_decay_pow" => params.v31_sell_decay_pow = value,
        "v31_sell_fixed_sl_lo" => params.v31_sell_fixed_sl_lo = value,
        "v31_sell_fixed_sl_hi" => params.v31_sell_fixed_sl_hi = value,
        "v31_sell_fixed_sl_pow" => params.v31_sell_fixed_sl_pow = value,
        "v31_sell_max_hold_lo" => params.v31_sell_max_hold_lo = value,
        "v31_sell_max_hold_hi" => params.v31_sell_max_hold_hi = value,
        "v31_sell_max_hold_pow" => params.v31_sell_max_hold_pow = value,
        "v31_fee_rate" => params.v31_fee_rate = value,
        "v31_min_hold_bars" => params.v31_min_hold_bars = value.round() as i32,
        "v31_volume_lookback" => params.v31_volume_lookback = value.round() as i32,
        "v31_cutoff_tv_mult" => params.v31_cutoff_tv_mult = value,
        "v31_urgent_sell_tv_mult" => params.v31_urgent_sell_tv_mult = value,
        "v31_sell_ready_price_rise" => params.v31_sell_ready_price_rise = value,
        "v31_sell_wait_max" => params.v31_sell_wait_max = value.round() as i32,
        _ => {}
    }
}

// ─── NSGA-II core functions ───

/// Objective-only dominance (all maximized). Returns 1 if a dominates b,
/// -1 if b dominates a, 0 if neither. Used internally by the constrained
/// variant below.
pub fn dominates(a: &[f64], b: &[f64]) -> i32 {
    let mut a_better = false;
    let mut b_better = false;
    for (va, vb) in a.iter().zip(b.iter()) {
        if va > vb {
            a_better = true;
        } else if vb > va {
            b_better = true;
        }
    }
    if a_better && !b_better {
        1
    } else if b_better && !a_better {
        -1
    } else {
        0
    }
}

/// Deb's constrained-dominance:
///   * feasible ( violation == 0 ) always dominates infeasible
///   * among two infeasible, the lower violation dominates
///   * among two feasible, fall back to objective-only dominance
///
/// This is the fix for the NSGA-II symptom where constraints accumulated
/// via `config.min_trades` / `min_win_rate` were silently ignored —
/// infeasible solutions (e.g. 5-trade 100%-winrate) used to flood the
/// Pareto front because plain objective dominance treated them the same
/// as feasible solutions.
pub fn dominates_constrained(a: &Individual, b: &Individual) -> i32 {
    let a_feasible = a.constraint_violation <= 0.0;
    let b_feasible = b.constraint_violation <= 0.0;
    match (a_feasible, b_feasible) {
        (true, false) => 1,
        (false, true) => -1,
        (false, false) => {
            if a.constraint_violation < b.constraint_violation {
                1
            } else if a.constraint_violation > b.constraint_violation {
                -1
            } else {
                0
            }
        }
        (true, true) => dominates(&a.objectives, &b.objectives),
    }
}

/// Fast non-dominated sort using **constrained dominance**. Returns Vec of
/// fronts, each front is Vec of indices.
pub fn fast_non_dominated_sort(individuals: &[Individual]) -> Vec<Vec<usize>> {
    let n = individuals.len();
    let mut domination_count = vec![0usize; n];
    let mut dominated_by: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut fronts: Vec<Vec<usize>> = Vec::new();
    let mut front0: Vec<usize> = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            let d = dominates_constrained(&individuals[i], &individuals[j]);
            if d == 1 {
                dominated_by[i].push(j);
                domination_count[j] += 1;
            } else if d == -1 {
                dominated_by[j].push(i);
                domination_count[i] += 1;
            }
        }
        if domination_count[i] == 0 {
            front0.push(i);
        }
    }

    fronts.push(front0);

    let mut current_front = 0;
    while !fronts[current_front].is_empty() {
        let mut next_front: Vec<usize> = Vec::new();
        for &i in &fronts[current_front] {
            for &j in &dominated_by[i] {
                domination_count[j] -= 1;
                if domination_count[j] == 0 {
                    next_front.push(j);
                }
            }
        }
        if next_front.is_empty() {
            break;
        }
        fronts.push(next_front);
        current_front += 1;
    }

    fronts
}

/// Calculate crowding distance for a single front.
pub fn calculate_crowding_distance(individuals: &mut [Individual], front: &[usize], num_objectives: usize) {
    let n = front.len();
    if n <= 2 {
        for &idx in front {
            individuals[idx].crowding_distance = f64::INFINITY;
        }
        return;
    }

    for &idx in front {
        individuals[idx].crowding_distance = 0.0;
    }

    for m in 0..num_objectives {
        // Sort front indices by objective m
        let mut sorted: Vec<usize> = front.to_vec();
        sorted.sort_by(|&a, &b| {
            individuals[a].objectives[m]
                .partial_cmp(&individuals[b].objectives[m])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Boundary gets infinity
        individuals[sorted[0]].crowding_distance = f64::INFINITY;
        individuals[sorted[n - 1]].crowding_distance = f64::INFINITY;

        let obj_min = individuals[sorted[0]].objectives[m];
        let obj_max = individuals[sorted[n - 1]].objectives[m];
        let range = obj_max - obj_min;
        if range <= 0.0 {
            continue;
        }

        for k in 1..(n - 1) {
            let next_val = individuals[sorted[k + 1]].objectives[m];
            let prev_val = individuals[sorted[k - 1]].objectives[m];
            individuals[sorted[k]].crowding_distance += (next_val - prev_val) / range;
        }
    }
}

fn tournament_select(individuals: &[Individual], rng: &mut impl Rng) -> usize {
    let a = rng.gen_range(0..individuals.len());
    let b = rng.gen_range(0..individuals.len());

    // Better rank wins; tie broken by crowding distance
    if individuals[a].rank < individuals[b].rank {
        a
    } else if individuals[b].rank < individuals[a].rank {
        b
    } else if individuals[a].crowding_distance > individuals[b].crowding_distance {
        a
    } else {
        b
    }
}

fn crossover(
    p1: &TradingParameters,
    p2: &TradingParameters,
    ranges: &[ParameterRange],
    rng: &mut impl Rng,
) -> TradingParameters {
    let mut child = p1.clone();
    let alpha: f64 = rng.gen();
    for range in ranges {
        let v1 = get_parameter(p1, &range.name);
        let v2 = get_parameter(p2, &range.name);
        let blended = alpha * v1 + (1.0 - alpha) * v2;
        let clamped = blended.clamp(range.min, range.max);
        set_parameter(&mut child, &range.name, clamped);
    }
    child
}

fn mutate(
    params: &mut TradingParameters,
    ranges: &[ParameterRange],
    mutation_rate: f64,
    rng: &mut impl Rng,
) {
    for range in ranges {
        if rng.gen::<f64>() < 0.2 {
            // 20% chance per param
            let current = get_parameter(params, &range.name);
            let delta = (range.max - range.min) * mutation_rate;
            let perturbation = (rng.gen::<f64>() * 2.0 - 1.0) * delta;
            let new_val = (current + perturbation).clamp(range.min, range.max);
            set_parameter(params, &range.name, new_val);
        }
    }
}

/// Map objective key to a maximize-always value (invert minimization ones so
/// NSGA-II domination can stay uniform — larger is better for every entry).
fn objective_value(name: &str, result: &SimulationResult) -> f64 {
    match name {
        "total_return" => result.total_return,
        "win_rate" => result.win_rate,
        "profit_factor" => {
            if result.profit_factor.is_finite() { result.profit_factor } else { 0.0 }
        }
        "total_trades" => result.total_trades as f64,
        "sharpe_ratio" => result.sharpe_ratio,
        "sortino_ratio" => result.sortino_ratio,
        // Minimization objectives: negate so "higher = better" holds uniformly.
        "max_drawdown" => -result.max_drawdown,
        _ => 0.0,
    }
}

fn evaluate(
    individual: &mut Individual,
    data: &[MarketData],
    strategy: &dyn Strategy,
    config: &OptimizerConfig,
) {
    let result: SimulationResult = strategy.run_simulation(data, &individual.parameters);

    // Full metrics map — always populated so the UI can render every
    // objective column regardless of which were selected for NSGA-II.
    individual.metrics.insert("total_return".into(), result.total_return);
    individual.metrics.insert("win_rate".into(), result.win_rate);
    individual.metrics.insert(
        "profit_factor".into(),
        if result.profit_factor.is_finite() { result.profit_factor } else { 0.0 },
    );
    individual.metrics.insert("total_trades".into(), result.total_trades as f64);
    individual.metrics.insert("sharpe_ratio".into(), result.sharpe_ratio);
    individual.metrics.insert("sortino_ratio".into(), result.sortino_ratio);
    // Stored as raw (positive) — the UI only negates inside dominance math.
    individual.metrics.insert("max_drawdown".into(), result.max_drawdown);

    // Objectives — respect the user-selected list; fall back to the default
    // 2-tuple (return, win_rate) when none are configured.
    if config.objectives.is_empty() {
        individual.objectives = vec![result.total_return, result.win_rate];
    } else {
        individual.objectives = config
            .objectives
            .iter()
            .map(|n| objective_value(n, &result))
            .collect();
    }

    // Constraint violation (non-negative). Deb's constrained dominance uses
    // this to treat feasible solutions as strictly better than infeasible.
    let mut violation = 0.0;
    if result.win_rate < config.min_win_rate {
        violation += config.min_win_rate - result.win_rate;
    }
    if result.total_trades < config.min_trades {
        violation += (config.min_trades - result.total_trades) as f64;
    }
    if result.total_return < config.min_return {
        violation += config.min_return - result.total_return;
    }
    individual.constraint_violation = violation;
}

impl Nsga2Optimizer {
    pub fn new(config: OptimizerConfig) -> Self {
        Self { config }
    }

    /// Legacy-compatible single-shot run (no cancel, no seed).
    pub fn run(
        &self,
        data: &[MarketData],
        strategy: &dyn Strategy,
        callback: Option<&dyn Fn(&GenerationResult)>,
    ) -> Vec<Individual> {
        self.run_advanced(data, strategy, None, 0, None, callback)
    }

    /// Full-featured run:
    ///   * `seed_population` — optional warm start (Continue); when absent
    ///     a random population of `config.population_size` is created.
    ///   * `start_generation` — generation index to start from (for progress
    ///     reporting & for Continue to keep counting where the last run left off).
    ///   * `cancel_token` — poll between generations; partial Pareto is
    ///     returned when flipped.
    ///   * `callback` — per-generation snapshot (UI events, DB writes).
    ///
    /// Evaluation of both initial and offspring populations uses rayon for
    /// per-individual parallelism — ~4–8× speedup on modern desktop CPUs.
    pub fn run_advanced(
        &self,
        data: &[MarketData],
        strategy: &dyn Strategy,
        seed_population: Option<Vec<Individual>>,
        start_generation: usize,
        cancel_token: Option<Arc<AtomicBool>>,
        callback: Option<&dyn Fn(&GenerationResult)>,
    ) -> Vec<Individual> {
        let ranges = strategy.parameter_ranges();
        let pop_size = self.config.population_size;
        let generations = self.config.generations;
        let total_generations = start_generation + generations;

        let cancelled = || {
            cancel_token
                .as_ref()
                .map(|t| t.load(Ordering::Relaxed))
                .unwrap_or(false)
        };

        // 1. Initial population — seed if provided, else random + evaluate
        let mut population: Vec<Individual> = if let Some(mut seed) = seed_population {
            // Pad or truncate to pop_size
            let mut rng = rand::thread_rng();
            while seed.len() < pop_size {
                let mut params = TradingParameters::default();
                for range in &ranges {
                    let val = rng.gen::<f64>() * (range.max - range.min) + range.min;
                    set_parameter(&mut params, &range.name, val);
                }
                seed.push(Individual::new(params));
            }
            seed.truncate(pop_size);
            // Re-evaluate seeded individuals — they may have been evaluated
            // against a different market window and their objectives could
            // be stale. This is cheap compared to generations×pop_size.
            seed.par_iter_mut()
                .for_each(|ind| evaluate(ind, data, strategy, &self.config));
            seed
        } else {
            let mut rng = rand::thread_rng();
            let mut pop: Vec<Individual> = (0..pop_size)
                .map(|_| {
                    let mut params = TradingParameters::default();
                    for range in &ranges {
                        let val = rng.gen::<f64>() * (range.max - range.min) + range.min;
                        set_parameter(&mut params, &range.name, val);
                    }
                    Individual::new(params)
                })
                .collect();
            pop.par_iter_mut()
                .for_each(|ind| evaluate(ind, data, strategy, &self.config));
            pop
        };

        for gen in start_generation..total_generations {
            if cancelled() {
                break;
            }

            // Build offspring — crossover/mutation done sequentially (uses rng
            // state) but evaluation is parallel.
            let offspring_params: Vec<TradingParameters> = {
                let mut rng = rand::thread_rng();
                (0..pop_size)
                    .map(|_| {
                        let p1_idx = tournament_select(&population, &mut rng);
                        let p2_idx = tournament_select(&population, &mut rng);
                        let mut child = if rng.gen::<f64>() < self.config.crossover_rate {
                            crossover(
                                &population[p1_idx].parameters,
                                &population[p2_idx].parameters,
                                &ranges,
                                &mut rng,
                            )
                        } else {
                            population[p1_idx].parameters.clone()
                        };
                        mutate(&mut child, &ranges, self.config.mutation_rate, &mut rng);
                        child
                    })
                    .collect()
            };
            let mut offspring: Vec<Individual> = offspring_params
                .into_par_iter()
                .map(|params| {
                    let mut ind = Individual::new(params);
                    evaluate(&mut ind, data, strategy, &self.config);
                    ind
                })
                .collect();

            // Combine, non-dominated sort, crowding distance
            let mut combined: Vec<Individual> = population.into_iter()
                .chain(offspring.drain(..))
                .collect();
            let fronts = fast_non_dominated_sort(&combined);
            let num_objectives = if combined.is_empty() { 2 } else { combined[0].objectives.len() };
            for (rank, front) in fronts.iter().enumerate() {
                for &idx in front {
                    combined[idx].rank = rank;
                }
                calculate_crowding_distance(&mut combined, front, num_objectives);
            }

            // Environmental selection
            let mut next_pop: Vec<Individual> = Vec::with_capacity(pop_size);
            for front in &fronts {
                if next_pop.len() + front.len() <= pop_size {
                    for &idx in front {
                        next_pop.push(combined[idx].clone());
                    }
                } else {
                    let mut sorted_front: Vec<usize> = front.clone();
                    sorted_front.sort_by(|&a, &b| {
                        combined[b]
                            .crowding_distance
                            .partial_cmp(&combined[a].crowding_distance)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let remaining = pop_size - next_pop.len();
                    for &idx in sorted_front.iter().take(remaining) {
                        next_pop.push(combined[idx].clone());
                    }
                    break;
                }
            }
            population = next_pop;

            // Per-generation callback — emit Pareto front + best metrics
            if let Some(cb) = callback {
                let pareto: Vec<Individual> =
                    population.iter().filter(|i| i.rank == 0).cloned().collect();
                let best_return = pareto
                    .iter()
                    .map(|i| i.objectives.first().copied().unwrap_or(f64::NEG_INFINITY))
                    .fold(f64::NEG_INFINITY, f64::max);
                let best_win_rate = pareto
                    .iter()
                    .map(|i| i.objectives.get(1).copied().unwrap_or(f64::NEG_INFINITY))
                    .fold(f64::NEG_INFINITY, f64::max);
                cb(&GenerationResult {
                    generation: gen + 1,
                    total_generations,
                    best_return,
                    best_win_rate,
                    front_size: pareto.len(),
                    front: &pareto,
                });
            }
        }

        population
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dominates() {
        // A(10, 80) vs B(5, 40) -> A dominates
        assert_eq!(dominates(&[10.0, 80.0], &[5.0, 40.0]), 1);
        // B vs A -> B is dominated
        assert_eq!(dominates(&[5.0, 40.0], &[10.0, 80.0]), -1);
        // A(10, 80) vs C(8, 90) -> neither dominates
        assert_eq!(dominates(&[10.0, 80.0], &[8.0, 90.0]), 0);
    }

    #[test]
    fn test_non_dominated_sort() {
        let individuals = vec![
            Individual {
                objectives: vec![10.0, 80.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
                metrics: HashMap::new(),
            },
            Individual {
                objectives: vec![5.0, 40.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
                metrics: HashMap::new(),
            },
            Individual {
                objectives: vec![8.0, 90.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
                metrics: HashMap::new(),
            },
        ];

        let fronts = fast_non_dominated_sort(&individuals);
        // Front 0: A(10,80) and C(8,90) are non-dominated
        assert_eq!(fronts[0].len(), 2);
        // Front 1: B(5,40) is dominated by both
        assert_eq!(fronts[1].len(), 1);
        assert!(fronts[0].contains(&0)); // A
        assert!(fronts[0].contains(&2)); // C
        assert!(fronts[1].contains(&1)); // B
    }

    #[test]
    fn test_crowding_distance_boundary() {
        let mut individuals = vec![
            Individual {
                objectives: vec![1.0, 10.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
                metrics: HashMap::new(),
            },
            Individual {
                objectives: vec![5.0, 5.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
                metrics: HashMap::new(),
            },
            Individual {
                objectives: vec![10.0, 1.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
                metrics: HashMap::new(),
            },
        ];

        let front: Vec<usize> = vec![0, 1, 2];
        calculate_crowding_distance(&mut individuals, &front, 2);

        // Boundary individuals should have infinity
        assert!(individuals[0].crowding_distance.is_infinite());
        assert!(individuals[2].crowding_distance.is_infinite());
        // Middle individual should have finite distance
        assert!(individuals[1].crowding_distance.is_finite());
        assert!(individuals[1].crowding_distance > 0.0);
    }

    #[test]
    fn test_get_set_parameter_roundtrip() {
        let mut params = TradingParameters::default();
        set_parameter(&mut params, "v3_fee_rate", 0.123);
        assert!((get_parameter(&params, "v3_fee_rate") - 0.123).abs() < 1e-10);

        set_parameter(&mut params, "v3_min_hold_bars", 42.0);
        assert_eq!(params.v3_min_hold_bars, 42);
    }
}
