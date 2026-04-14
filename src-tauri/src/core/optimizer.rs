use crate::models::config::{OptimizerConfig, ParameterRange};
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use crate::strategies::Strategy;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct Individual {
    pub objectives: Vec<f64>,
    pub rank: usize,
    pub crowding_distance: f64,
    pub domination_count: usize,
    pub dominated_solutions: Vec<usize>,
    pub constraint_violation: f64,
    pub parameters: TradingParameters,
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub generation: usize,
    pub best_return: f64,
    pub best_win_rate: f64,
    pub front_size: usize,
}

pub struct Nsga2Optimizer {
    pub config: OptimizerConfig,
}

// ─── get/set parameter by name ───

pub fn get_parameter(params: &TradingParameters, name: &str) -> f64 {
    match name {
        "urgent_buy_volume_threshold" => params.urgent_buy_volume_threshold,
        "buy_ready_volume_threshold" => params.buy_ready_volume_threshold,
        "buy_confirm_volume_decay_ratio" => params.buy_confirm_volume_decay_ratio,
        "buy_wait_max_periods" => params.buy_wait_max_periods as f64,
        "buy_confirm_psy_threshold" => params.buy_confirm_psy_threshold,
        "urgent_buy_price_drop_ratio" => params.urgent_buy_price_drop_ratio,
        "buy_ready_price_drop_ratio" => params.buy_ready_price_drop_ratio,
        "urgent_sell_volume_threshold" => params.urgent_sell_volume_threshold,
        "sell_ready_volume_threshold" => params.sell_ready_volume_threshold,
        "sell_confirm_volume_decay_ratio" => params.sell_confirm_volume_decay_ratio,
        "sell_wait_max_periods" => params.sell_wait_max_periods as f64,
        "urgent_sell_profit_ratio" => params.urgent_sell_profit_ratio,
        "sell_ready_price_rise_ratio" => params.sell_ready_price_rise_ratio,
        "trailing_stop_pct" => params.trailing_stop_pct,
        "max_hold_periods" => params.max_hold_periods as f64,
        "fee_rate" => params.fee_rate,
        "fixed_stop_loss_pct" => params.fixed_stop_loss_pct,
        "fixed_take_profit_pct" => params.fixed_take_profit_pct,
        "v1_adaptive_volume_window" => params.v1_adaptive_volume_window as f64,
        "v1_atr_trailing_multiplier" => params.v1_atr_trailing_multiplier,
        "v2_rsi_weight" => params.v2_rsi_weight,
        "v2_macd_weight" => params.v2_macd_weight,
        "v2_bb_weight" => params.v2_bb_weight,
        "v2_buy_score_threshold" => params.v2_buy_score_threshold,
        "v2_sell_score_threshold" => params.v2_sell_score_threshold,
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
        _ => 0.0,
    }
}

pub fn set_parameter(params: &mut TradingParameters, name: &str, value: f64) {
    match name {
        "urgent_buy_volume_threshold" => params.urgent_buy_volume_threshold = value,
        "buy_ready_volume_threshold" => params.buy_ready_volume_threshold = value,
        "buy_confirm_volume_decay_ratio" => params.buy_confirm_volume_decay_ratio = value,
        "buy_wait_max_periods" => params.buy_wait_max_periods = value as i32,
        "buy_confirm_psy_threshold" => params.buy_confirm_psy_threshold = value,
        "urgent_buy_price_drop_ratio" => params.urgent_buy_price_drop_ratio = value,
        "buy_ready_price_drop_ratio" => params.buy_ready_price_drop_ratio = value,
        "urgent_sell_volume_threshold" => params.urgent_sell_volume_threshold = value,
        "sell_ready_volume_threshold" => params.sell_ready_volume_threshold = value,
        "sell_confirm_volume_decay_ratio" => params.sell_confirm_volume_decay_ratio = value,
        "sell_wait_max_periods" => params.sell_wait_max_periods = value as i32,
        "urgent_sell_profit_ratio" => params.urgent_sell_profit_ratio = value,
        "sell_ready_price_rise_ratio" => params.sell_ready_price_rise_ratio = value,
        "trailing_stop_pct" => params.trailing_stop_pct = value,
        "max_hold_periods" => params.max_hold_periods = value as i32,
        "fee_rate" => params.fee_rate = value,
        "fixed_stop_loss_pct" => params.fixed_stop_loss_pct = value,
        "fixed_take_profit_pct" => params.fixed_take_profit_pct = value,
        "v1_adaptive_volume_window" => params.v1_adaptive_volume_window = value as i32,
        "v1_atr_trailing_multiplier" => params.v1_atr_trailing_multiplier = value,
        "v2_rsi_weight" => params.v2_rsi_weight = value,
        "v2_macd_weight" => params.v2_macd_weight = value,
        "v2_bb_weight" => params.v2_bb_weight = value,
        "v2_buy_score_threshold" => params.v2_buy_score_threshold = value,
        "v2_sell_score_threshold" => params.v2_sell_score_threshold = value,
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
        "v3_min_hold_bars" => params.v3_min_hold_bars = value as i32,
        "v3_volume_lookback" => params.v3_volume_lookback = value as i32,
        _ => {}
    }
}

// ─── NSGA-II core functions ───

/// Returns 1 if a dominates b, -1 if b dominates a, 0 if neither.
/// All objectives are maximized.
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

/// Fast non-dominated sort. Returns Vec of fronts, each front is Vec of indices.
pub fn fast_non_dominated_sort(individuals: &[Individual]) -> Vec<Vec<usize>> {
    let n = individuals.len();
    let mut domination_count = vec![0usize; n];
    let mut dominated_by: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut fronts: Vec<Vec<usize>> = Vec::new();
    let mut front0: Vec<usize> = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            let d = dominates(&individuals[i].objectives, &individuals[j].objectives);
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

fn evaluate(
    individual: &mut Individual,
    data: &[MarketData],
    strategy: &dyn Strategy,
    config: &OptimizerConfig,
) {
    let result: SimulationResult = strategy.run_simulation(data, &individual.parameters);

    // Objectives: total_return, win_rate (both maximized)
    individual.objectives = vec![result.total_return, result.win_rate];

    // Constraint violation
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

    pub fn run(
        &self,
        data: &[MarketData],
        strategy: &dyn Strategy,
        callback: Option<&dyn Fn(&GenerationResult)>,
    ) -> Vec<Individual> {
        let ranges = strategy.parameter_ranges();
        let pop_size = self.config.population_size;
        let generations = self.config.generations;
        let mut rng = rand::thread_rng();

        // 1. Random initial population
        let mut population: Vec<Individual> = (0..pop_size)
            .map(|_| {
                let mut params = TradingParameters::default();
                for range in &ranges {
                    let val = rng.gen::<f64>() * (range.max - range.min) + range.min;
                    set_parameter(&mut params, &range.name, val);
                }
                Individual::new(params)
            })
            .collect();

        // Evaluate initial population
        for ind in &mut population {
            evaluate(ind, data, strategy, &self.config);
        }

        for gen in 0..generations {
            // Create offspring
            let mut offspring: Vec<Individual> = Vec::with_capacity(pop_size);
            while offspring.len() < pop_size {
                let p1_idx = tournament_select(&population, &mut rng);
                let p2_idx = tournament_select(&population, &mut rng);

                let mut child_params = if rng.gen::<f64>() < self.config.crossover_rate {
                    crossover(
                        &population[p1_idx].parameters,
                        &population[p2_idx].parameters,
                        &ranges,
                        &mut rng,
                    )
                } else {
                    population[p1_idx].parameters.clone()
                };

                mutate(&mut child_params, &ranges, self.config.mutation_rate, &mut rng);

                let mut child = Individual::new(child_params);
                evaluate(&mut child, data, strategy, &self.config);
                offspring.push(child);
            }

            // Combine parent + offspring
            let mut combined: Vec<Individual> = population.into_iter().chain(offspring).collect();

            // Non-dominated sort
            let fronts = fast_non_dominated_sort(&combined);
            let num_objectives = if combined.is_empty() { 2 } else { combined[0].objectives.len() };

            // Assign ranks and crowding distances
            for (rank, front) in fronts.iter().enumerate() {
                for &idx in front {
                    combined[idx].rank = rank;
                }
                calculate_crowding_distance(&mut combined, front, num_objectives);
            }

            // Select next generation
            let mut next_pop: Vec<Individual> = Vec::with_capacity(pop_size);
            for front in &fronts {
                if next_pop.len() + front.len() <= pop_size {
                    for &idx in front {
                        next_pop.push(combined[idx].clone());
                    }
                } else {
                    // Sort remaining front by crowding distance (descending)
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

            // Callback
            if let Some(cb) = callback {
                let best_return = population
                    .iter()
                    .map(|i| i.objectives.first().copied().unwrap_or(0.0))
                    .fold(f64::NEG_INFINITY, f64::max);
                let best_win_rate = population
                    .iter()
                    .map(|i| i.objectives.get(1).copied().unwrap_or(0.0))
                    .fold(f64::NEG_INFINITY, f64::max);
                let front_size = fast_non_dominated_sort(&population)
                    .first()
                    .map(|f| f.len())
                    .unwrap_or(0);

                cb(&GenerationResult {
                    generation: gen,
                    best_return,
                    best_win_rate,
                    front_size,
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
            },
            Individual {
                objectives: vec![5.0, 40.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
            },
            Individual {
                objectives: vec![8.0, 90.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
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
            },
            Individual {
                objectives: vec![5.0, 5.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
            },
            Individual {
                objectives: vec![10.0, 1.0],
                rank: 0,
                crowding_distance: 0.0,
                domination_count: 0,
                dominated_solutions: Vec::new(),
                constraint_violation: 0.0,
                parameters: TradingParameters::default(),
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
        set_parameter(&mut params, "fee_rate", 0.123);
        assert!((get_parameter(&params, "fee_rate") - 0.123).abs() < 1e-10);

        set_parameter(&mut params, "buy_wait_max_periods", 42.0);
        assert_eq!(params.buy_wait_max_periods, 42);
    }
}
