use bitcoin_trader_lib::core::engine::run_simulation;
use bitcoin_trader_lib::models::market::{Candle, IndicatorSet, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use chrono::{TimeZone, Utc};

fn make_market_data(prices: &[(f64, f64)]) -> Vec<MarketData> {
    prices
        .iter()
        .enumerate()
        .map(|(i, &(close, volume))| MarketData {
            candle: Candle {
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, i as u32 % 24, 0, 0).unwrap(),
                open: close,
                high: close * 1.01,
                low: close * 0.99,
                close,
                volume,
            },
            indicators: IndicatorSet::default(),
        })
        .collect()
}

#[test]
fn test_no_trades_low_volume() {
    let data = make_market_data(
        &(0..100)
            .map(|_| (100.0, 100.0))
            .collect::<Vec<_>>(),
    );
    let params = TradingParameters::default();
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 0);
}

#[test]
fn test_buy_signal_on_high_volume() {
    let mut params = TradingParameters::default();
    params.buy_ready_volume_threshold = 500.0;
    params.buy_ready_price_drop_ratio = 0.005;
    params.buy_confirm_volume_decay_ratio = 0.5;
    params.buy_wait_max_periods = 10;
    params.max_hold_periods = 5;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 10];
    prices.push((99.0, 600.0)); // triggers ready buy
    prices.push((98.5, 200.0)); // decay confirms
    for _ in 0..10 {
        prices.push((99.0, 100.0));
    }

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert!(result.buy_signals > 0, "expected buy signals, got {}", result.buy_signals);
}

#[test]
fn test_round_trip_trade() {
    let mut params = TradingParameters::default();
    params.buy_ready_volume_threshold = 500.0;
    params.buy_ready_price_drop_ratio = 0.005;
    params.buy_confirm_volume_decay_ratio = 0.5;
    params.buy_wait_max_periods = 10;
    params.fixed_take_profit_pct = 0.03;
    params.fee_rate = 0.001;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    prices.push((99.0, 600.0)); // ready
    prices.push((98.5, 200.0)); // decay → buy
    prices.push((102.0, 100.0));
    prices.push((105.0, 100.0)); // take profit triggers

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert!(
        result.total_trades >= 1,
        "expected at least 1 trade, got {}",
        result.total_trades
    );
    assert!(
        result.total_return > 0.0,
        "expected positive return, got {}",
        result.total_return
    );
}
