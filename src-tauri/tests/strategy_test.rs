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
fn test_registry_has_v0_through_v5() {
    let registry = StrategyRegistry::new();
    assert!(registry.get("V0").is_some(), "V0 should be registered");
    assert!(registry.get("V1").is_some(), "V1 should be registered");
    assert!(registry.get("V2").is_some(), "V2 should be registered");
    assert!(registry.get("V3").is_some(), "V3 should be registered");
    assert!(registry.get("V4").is_some(), "V4 should be registered");
    assert!(registry.get("V5").is_some(), "V5 should be registered");

    let list = registry.list();
    assert!(list.len() >= 6, "should have at least 6 strategies");
}

#[test]
fn test_v0_runs_without_panic_on_flat_data() {
    let registry = StrategyRegistry::new();
    let v0 = registry.get("V0").unwrap();
    let data = make_flat_data(100);
    let params = TradingParameters::default();
    let result = v0.run_simulation(&data, &params);
    // Flat data with low volume should produce no trades
    assert_eq!(result.total_trades, 0);
}

#[test]
fn test_v1_runs_without_panic() {
    let registry = StrategyRegistry::new();
    let v1 = registry.get("V1").unwrap();
    let data = make_flat_data(100);
    let params = TradingParameters::default();
    let _result = v1.run_simulation(&data, &params);
}

#[test]
fn test_v2_runs_without_panic() {
    let registry = StrategyRegistry::new();
    let v2 = registry.get("V2").unwrap();
    let data = make_flat_data(100);
    let params = TradingParameters::default();
    let _result = v2.run_simulation(&data, &params);
}

#[test]
fn test_v3_runs_without_panic() {
    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();
    let data = make_flat_data(100);
    let params = TradingParameters::default();
    let _result = v3.run_simulation(&data, &params);
}

#[test]
fn test_v4_runs_without_panic() {
    let registry = StrategyRegistry::new();
    let v4 = registry.get("V4").unwrap();
    let data = make_flat_data(100);
    let params = TradingParameters::default();
    let _result = v4.run_simulation(&data, &params);
}

#[test]
fn test_v5_runs_without_panic() {
    let registry = StrategyRegistry::new();
    let v5 = registry.get("V5").unwrap();
    let data = make_flat_data(100);
    let params = TradingParameters::default();
    let _result = v5.run_simulation(&data, &params);
}

#[test]
fn test_parameter_ranges_non_empty() {
    let registry = StrategyRegistry::new();
    for key in &["V0", "V1", "V2", "V3", "V4", "V5"] {
        let strategy = registry.get(key).unwrap();
        let ranges = strategy.parameter_ranges();
        assert!(
            !ranges.is_empty(),
            "{} should have non-empty parameter ranges",
            key
        );
    }
}

#[test]
fn test_strategy_names_and_descriptions() {
    let registry = StrategyRegistry::new();
    for key in &["V0", "V1", "V2", "V3", "V4", "V5"] {
        let strategy = registry.get(key).unwrap();
        assert!(!strategy.name().is_empty());
        assert!(!strategy.description().is_empty());
    }
}
