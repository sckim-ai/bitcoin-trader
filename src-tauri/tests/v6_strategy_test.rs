use bitcoin_trader_lib::models::market::{Candle, IndicatorSet, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use bitcoin_trader_lib::strategies::hybrid_adaptive_v6::HybridAdaptiveV6Strategy;
use bitcoin_trader_lib::strategies::{Strategy, StrategyRegistry};
use chrono::{Duration, TimeZone, Utc};

fn make_candle(
    ts_hour: i64,
    close: f64,
    volume: f64,
    rsi: f64,
    psy_hour: f64,
    psy_day: f64,
    atr: f64,
    adx: f64,
) -> MarketData {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::hours(ts_hour);
    MarketData {
        candle: Candle {
            timestamp: ts,
            open: close,
            high: close * 1.01,
            low: close * 0.99,
            close,
            volume,
        },
        indicators: IndicatorSet {
            rsi,
            psy_hour,
            psy_day,
            atr,
            adx,
            ..Default::default()
        },
    }
}

#[test]
fn v6_registered_in_registry() {
    let reg = StrategyRegistry::new();
    assert!(reg.get("V3").is_some());
    assert!(reg.get("V3.1").is_some());
    assert!(reg.get("V5").is_some());
    assert!(reg.get("V6").is_some(), "V6 must be registered");
    assert_eq!(
        reg.get("V6").unwrap().name(),
        "Hybrid Adaptive TV+PSY+ATR (V6)"
    );
}

#[test]
fn v6_parameter_ranges_include_hybrid_inputs() {
    let s = HybridAdaptiveV6Strategy;
    let ranges = s.parameter_ranges();
    let names: Vec<&str> = ranges.iter().map(|r| r.name.as_str()).collect();

    assert!(names.contains(&"v31_buy_tv_lo"));
    assert!(names.contains(&"v31_cutoff_tv_mult"));
    assert!(names.contains(&"v5_buy_psy_hour_lo"));
    assert!(names.contains(&"v5_sell_psy_day_hi"));
    assert!(names.contains(&"v6_atr_stop_mult"));
    assert!(names.contains(&"v6_atr_trail_mult"));
    assert!(names.contains(&"v6_min_adx"));
    assert!(
        !names.contains(&"v31_buy_psy_lo"),
        "V6 replaces single day-PSY with V5 dual PSY"
    );
}

#[test]
fn v6_urgent_buy_can_exit_on_atr_stop() {
    let mut data: Vec<MarketData> = (0..40)
        .map(|i| make_candle(i, 4_000_000.0, 100.0, 50.0, 0.0, 0.0, 50_000.0, 30.0))
        .collect();
    data.push(make_candle(
        40,
        3_720_000.0,
        100_000.0,
        30.0,
        -1.0,
        -1.0,
        50_000.0,
        30.0,
    ));
    data.push(make_candle(
        41,
        3_600_000.0,
        500.0,
        35.0,
        1.0,
        1.0,
        50_000.0,
        30.0,
    ));
    data.extend(
        (42..80).map(|i| make_candle(i, 3_600_000.0, 500.0, 35.0, 1.0, 1.0, 50_000.0, 30.0)),
    );

    let mut params = TradingParameters::default();
    params.v31_min_hold_bars = 1;
    params.v6_atr_stop_mult = 1.0;
    params.v6_atr_trail_mult = 0.0;

    let s = HybridAdaptiveV6Strategy;
    let result = s.run_simulation(&data, &params);
    assert!(
        result.buy_signals > 0,
        "urgent trade-value drop should trigger a V6 buy"
    );
    assert_eq!(result.total_trades, 1, "ATR stop should close the position");
    assert_eq!(result.last_position, 0);
}

#[test]
fn v6_dual_psy_blocks_decay_buy() {
    let mut data: Vec<MarketData> = (0..35)
        .map(|i| make_candle(i, 1000.0, 100.0, 50.0, 0.0, 0.0, 10.0, 30.0))
        .collect();
    data.push(make_candle(35, 970.0, 2_000.0, 50.0, 0.0, 0.0, 10.0, 30.0));
    data.push(make_candle(36, 960.0, 10.0, 50.0, 0.0, 0.0, 10.0, 30.0));
    data.extend((37..80).map(|i| make_candle(i, 960.0, 10.0, 50.0, 0.0, 0.0, 10.0, 30.0)));

    let mut params = TradingParameters::default();
    params.v31_urgent_buy_tv_lo = 1.0e15;
    params.v31_urgent_buy_tv_hi = 1.0e15;
    params.v31_buy_tv_lo = 1.0;
    params.v31_buy_tv_hi = 1.0;
    params.v31_buy_price_drop_lo = 1.01;
    params.v31_buy_price_drop_hi = 1.01;
    params.v31_buy_decay_lo = 0.5;
    params.v31_buy_decay_hi = 0.5;
    params.v31_buy_wait_lo = 10.0;
    params.v31_buy_wait_hi = 10.0;
    params.v5_buy_psy_hour_lo = -10.0;
    params.v5_buy_psy_hour_hi = -10.0;
    params.v5_buy_psy_day_lo = -10.0;
    params.v5_buy_psy_day_hi = -10.0;

    let s = HybridAdaptiveV6Strategy;
    let result = s.run_simulation(&data, &params);
    assert!(result.buy_signals > 0, "ready-buy should be observed");
    assert_eq!(
        result.last_position, 0,
        "impossible dual PSY must block entry"
    );
    assert_eq!(result.total_trades, 0);
}
