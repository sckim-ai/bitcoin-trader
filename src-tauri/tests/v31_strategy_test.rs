use bitcoin_trader_lib::models::market::{Candle, IndicatorSet, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use bitcoin_trader_lib::strategies::{regime_adaptive_v31::RegimeAdaptiveV31Strategy, Strategy, StrategyRegistry};
use chrono::{Duration, TimeZone, Utc};

fn make_candle(ts_hour: i64, close: f64, volume: f64, rsi: f64, psy: f64) -> MarketData {
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
            psy_day: psy,
            ..Default::default()
        },
    }
}

#[test]
fn test_v31_registered_in_registry() {
    let reg = StrategyRegistry::new();
    assert!(reg.get("V3.1").is_some(), "V3.1 must be registered");
    assert!(reg.get("V3").is_some(), "V3 must still be registered");
}

#[test]
fn test_v31_parameter_ranges_use_v31_prefix() {
    let s = RegimeAdaptiveV31Strategy;
    let ranges = s.parameter_ranges();
    assert!(!ranges.is_empty());
    for r in &ranges {
        assert!(
            r.name.starts_with("v31_"),
            "parameter {} does not use v31_ prefix",
            r.name
        );
    }
    // 신규 4개 파라미터 노출 검증
    let names: Vec<&str> = ranges.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"v31_cutoff_tv_mult"));
    assert!(names.contains(&"v31_urgent_sell_tv_mult"));
    assert!(names.contains(&"v31_sell_ready_price_rise"));
    assert!(names.contains(&"v31_sell_wait_max"));
}

#[test]
fn test_v31_flat_data_no_panic() {
    let data: Vec<MarketData> = (0..100)
        .map(|i| make_candle(i as i64, 4_000_000.0, 100.0, 50.0, 0.0))
        .collect();
    let params = TradingParameters::default();
    let s = RegimeAdaptiveV31Strategy;
    let result = s.run_simulation(&data, &params);
    assert_eq!(result.total_trades, 0, "flat data should produce no trades");
}

#[test]
fn test_v31_triggers_on_trade_value_spike() {
    // 시나리오: 100시간 플랫 → 1시간 거래대금 스파이크 + 가격 하락 → buy 기대
    let mut data: Vec<MarketData> = (0..100)
        .map(|i| make_candle(i as i64, 4_000_000.0, 100.0, 50.0, -0.3))
        .collect();

    // spike bar: close 0.93x (drop), volume 1000x → tv 930x — urgent_buy_tv 초과
    data.push(make_candle(100, 3_720_000.0, 100_000.0, 30.0, -0.3));
    // 후속: 반등
    for i in 1..60 {
        data.push(make_candle(100 + i, 4_200_000.0, 200.0, 55.0, 0.1));
    }

    let mut params = TradingParameters::default();
    // 테스트 단순화를 위해 min_hold_bars 짧게
    params.v31_min_hold_bars = 3;

    let s = RegimeAdaptiveV31Strategy;
    let result = s.run_simulation(&data, &params);
    assert!(result.buy_signals > 0, "trade-value spike should generate buy signal");
}
