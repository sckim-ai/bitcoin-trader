use bitcoin_trader_lib::core::optimizer::{
    calculate_crowding_distance, dominates, fast_non_dominated_sort, Individual, Nsga2Optimizer,
};
use bitcoin_trader_lib::models::config::OptimizerConfig;
use bitcoin_trader_lib::models::market::{Candle, IndicatorSet, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use bitcoin_trader_lib::strategies::StrategyRegistry;
use chrono::{TimeZone, Utc};

fn make_flat_data(n: usize) -> Vec<MarketData> {
    (0..n)
        .map(|i| MarketData {
            candle: Candle {
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 100.0,
            },
            indicators: IndicatorSet::default(),
        })
        .collect()
}

#[test]
fn test_non_dominated_sort_basic() {
    let individuals = vec![
        Individual::new_with_objectives(vec![10.0, 80.0]),
        Individual::new_with_objectives(vec![5.0, 40.0]),
        Individual::new_with_objectives(vec![8.0, 90.0]),
    ];

    let fronts = fast_non_dominated_sort(&individuals);
    // Front 0: A(10,80) and C(8,90) are non-dominated by each other
    assert_eq!(fronts[0].len(), 2, "front 0 should have 2 members");
    assert_eq!(fronts[1].len(), 1, "front 1 should have 1 member");
    assert!(fronts[0].contains(&0));
    assert!(fronts[0].contains(&2));
    assert!(fronts[1].contains(&1));
}

#[test]
fn test_dominates_function() {
    assert_eq!(dominates(&[10.0, 80.0], &[5.0, 40.0]), 1);
    assert_eq!(dominates(&[5.0, 40.0], &[10.0, 80.0]), -1);
    assert_eq!(dominates(&[10.0, 80.0], &[8.0, 90.0]), 0);
    assert_eq!(dominates(&[5.0, 5.0], &[5.0, 5.0]), 0);
}

#[test]
fn test_crowding_distance_boundary_infinity() {
    let mut individuals = vec![
        Individual::new_with_objectives(vec![1.0, 10.0]),
        Individual::new_with_objectives(vec![5.0, 5.0]),
        Individual::new_with_objectives(vec![10.0, 1.0]),
    ];

    let front = vec![0, 1, 2];
    calculate_crowding_distance(&mut individuals, &front, 2);

    assert!(individuals[0].crowding_distance.is_infinite());
    assert!(individuals[2].crowding_distance.is_infinite());
    assert!(individuals[1].crowding_distance.is_finite());
    assert!(individuals[1].crowding_distance > 0.0);
}

#[test]
fn test_optimizer_runs_without_panic() {
    let data = make_flat_data(50);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 10,
        generations: 3,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let result = optimizer.run(&data, strategy, None);
    assert_eq!(result.len(), 10, "should return population_size individuals");
}

#[test]
fn test_optimizer_with_callback() {
    let data = make_flat_data(50);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 10,
        generations: 3,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);

    // Use a counter cell since closure needs mutable state
    let counter = std::cell::Cell::new(0usize);
    let _result = optimizer.run(&data, strategy, Some(&|_gen_result| {
        counter.set(counter.get() + 1);
    }));
    let callback_count = counter.get();

    assert_eq!(callback_count, 3, "callback should be called once per generation");
}

// Helper to create Individual with objectives
trait IndividualTestHelper {
    fn new_with_objectives(objectives: Vec<f64>) -> Self;
}

impl IndividualTestHelper for Individual {
    fn new_with_objectives(objectives: Vec<f64>) -> Self {
        Individual {
            objectives,
            rank: 0,
            crowding_distance: 0.0,
            domination_count: 0,
            dominated_solutions: Vec::new(),
            constraint_violation: 0.0,
            parameters: TradingParameters::default(),
        }
    }
}
