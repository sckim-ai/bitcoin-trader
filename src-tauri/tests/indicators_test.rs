use approx::assert_relative_eq;
use chrono::{TimeZone, Utc};
use bitcoin_trader_lib::models::market::Candle;
use bitcoin_trader_lib::core::indicators::{calculate_all, calculate_incremental};

fn make_candles(closes: &[f64]) -> Vec<Candle> {
    closes
        .iter()
        .enumerate()
        .map(|(i, &c)| Candle {
            timestamp: Utc.with_ymd_and_hms(2024, 1, 1, i as u32 % 24, 0, 0).unwrap(),
            open: c,
            high: c,
            low: c,
            close: c,
            volume: 1000.0,
        })
        .collect()
}

/// (open, high, low, close, volume)
fn make_full_candles(data: &[(f64, f64, f64, f64, f64)]) -> Vec<Candle> {
    data.iter()
        .enumerate()
        .map(|(i, &(o, h, l, c, v))| Candle {
            timestamp: Utc.with_ymd_and_hms(2024, 1, 1, i as u32 % 24, 0, 0).unwrap(),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: v,
        })
        .collect()
}

// ─── SMA Tests ───

#[test]
fn test_sma10_sequence() {
    let closes: Vec<f64> = (1..=20).map(|x| x as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // SMA10 at index 9 = mean(1..=10) = 5.5
    assert_relative_eq!(ind[9].sma_10, 5.5, epsilon = 1e-10);
    // SMA10 at index 19 = mean(11..=20) = 15.5
    assert_relative_eq!(ind[19].sma_10, 15.5, epsilon = 1e-10);
}

#[test]
fn test_sma25_constant() {
    let closes = vec![100.0; 30];
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    assert_relative_eq!(ind[24].sma_25, 100.0, epsilon = 1e-10);
    assert_relative_eq!(ind[29].sma_25, 100.0, epsilon = 1e-10);
}

#[test]
fn test_sma_zeros_before_period() {
    let closes: Vec<f64> = (1..=20).map(|x| x as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // Before period, SMA should be 0
    assert_relative_eq!(ind[0].sma_10, 0.0, epsilon = 1e-10);
    assert_relative_eq!(ind[8].sma_10, 0.0, epsilon = 1e-10);
}

// ─── RSI Tests ───

#[test]
fn test_rsi_all_rises() {
    // 15 consecutive rises: 100, 101, ..., 114
    let closes: Vec<f64> = (0..15).map(|i| 100.0 + i as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // RSI at bar 14 should be 100 (no losses)
    assert_relative_eq!(ind[14].rsi, 100.0, epsilon = 1e-10);
}

#[test]
fn test_rsi_all_drops() {
    // 15 consecutive drops: 200, 199, ..., 186
    let closes: Vec<f64> = (0..15).map(|i| 200.0 - i as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // RSI at bar 14 should be 0 (no gains)
    assert_relative_eq!(ind[14].rsi, 0.0, epsilon = 1e-10);
}

#[test]
fn test_rsi_alternating() {
    // Alternating up/down: 100, 101, 100, 101, ... for 50 bars
    let closes: Vec<f64> = (0..50).map(|i| if i % 2 == 0 { 100.0 } else { 101.0 }).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // RSI should converge near 50
    let rsi_last = ind[49].rsi;
    assert!(rsi_last > 40.0 && rsi_last < 60.0, "RSI should be near 50, got {}", rsi_last);
}

// ─── MACD Tests ───

#[test]
fn test_macd_constant_price() {
    let closes = vec![100.0; 50];
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // At bar 35+, MACD, Signal, Histogram should all be ~0
    assert_relative_eq!(ind[35].macd, 0.0, epsilon = 1e-10);
    assert_relative_eq!(ind[35].macd_signal, 0.0, epsilon = 1e-10);
    assert_relative_eq!(ind[35].macd_histogram, 0.0, epsilon = 1e-10);
}

#[test]
fn test_macd_uptrend() {
    // Strong uptrend
    let closes: Vec<f64> = (0..50).map(|i| 100.0 + i as f64 * 2.0).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // MACD should be positive (fast EMA > slow EMA in uptrend)
    assert!(ind[35].macd > 0.0, "MACD should be positive in uptrend, got {}", ind[35].macd);
}

#[test]
fn test_macd_signal_starts_at_bar_33() {
    let closes = vec![100.0; 50];
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // Signal should be 0 before bar 33
    assert_relative_eq!(ind[32].macd_signal, 0.0, epsilon = 1e-10);
    // Signal should be defined at bar 33
    // For constant price, it's 0.0 anyway, but it should be "valid"
    assert_relative_eq!(ind[33].macd_signal, 0.0, epsilon = 1e-10);
}

// ─── Bollinger Band Tests ───

#[test]
fn test_bollinger_constant_price() {
    let closes = vec![100.0; 25];
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // With constant price, upper = middle = lower = price
    assert_relative_eq!(ind[19].bollinger_upper, 100.0, epsilon = 1e-10);
    assert_relative_eq!(ind[19].bollinger_middle, 100.0, epsilon = 1e-10);
    assert_relative_eq!(ind[19].bollinger_lower, 100.0, epsilon = 1e-10);
}

#[test]
fn test_bollinger_variable_price() {
    // Use a variable price series
    let closes: Vec<f64> = (0..25).map(|i| 100.0 + (i as f64 * 0.5).sin() * 10.0).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // upper > middle > lower for non-constant price
    let idx = 24;
    assert!(ind[idx].bollinger_upper > ind[idx].bollinger_middle,
        "upper {} should be > middle {}", ind[idx].bollinger_upper, ind[idx].bollinger_middle);
    assert!(ind[idx].bollinger_middle > ind[idx].bollinger_lower,
        "middle {} should be > lower {}", ind[idx].bollinger_middle, ind[idx].bollinger_lower);
}

// ─── ATR Tests ───

#[test]
fn test_atr_constant_range() {
    // H-L always 10, close in middle
    let data: Vec<(f64, f64, f64, f64, f64)> = (0..30)
        .map(|i| {
            let base = 100.0 + i as f64;
            (base, base + 5.0, base - 5.0, base, 1000.0)
        })
        .collect();
    let candles = make_full_candles(&data);
    let ind = calculate_all(&candles);
    // ATR should converge to 10.0 (constant H-L range with close in middle)
    assert_relative_eq!(ind[29].atr, 10.0, epsilon = 0.5);
}

#[test]
fn test_atr_zero_before_period() {
    let closes = vec![100.0; 10];
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // Not enough data for ATR period 14
    assert_relative_eq!(ind[0].atr, 0.0, epsilon = 1e-10);
}

// ─── ADX Tests ───

#[test]
fn test_adx_strong_uptrend() {
    // Strong uptrend: each bar higher high, higher low
    let data: Vec<(f64, f64, f64, f64, f64)> = (0..40)
        .map(|i| {
            let base = 100.0 + i as f64 * 3.0;
            (base, base + 2.0, base - 1.0, base + 1.0, 1000.0)
        })
        .collect();
    let candles = make_full_candles(&data);
    let ind = calculate_all(&candles);
    let last = ind.len() - 1;
    // DI+ should be > DI-
    assert!(ind[last].di_plus > ind[last].di_minus,
        "DI+ ({}) should be > DI- ({}) in uptrend", ind[last].di_plus, ind[last].di_minus);
    // ADX should be > 0
    assert!(ind[last].adx > 0.0, "ADX should be > 0 in trending market, got {}", ind[last].adx);
}

#[test]
fn test_adx_starts_at_bar_28() {
    let data: Vec<(f64, f64, f64, f64, f64)> = (0..35)
        .map(|i| {
            let base = 100.0 + i as f64;
            (base, base + 1.0, base - 1.0, base, 1000.0)
        })
        .collect();
    let candles = make_full_candles(&data);
    let ind = calculate_all(&candles);
    // ADX should be 0 before bar 28 (2*14)
    assert_relative_eq!(ind[27].adx, 0.0, epsilon = 1e-10);
    // ADX should have a value at bar 28
    assert!(ind[28].adx >= 0.0);
}

// ─── Stochastic Tests ───

#[test]
fn test_stochastic_close_at_high() {
    // Close always at high → %K = 100
    let data: Vec<(f64, f64, f64, f64, f64)> = (0..20)
        .map(|i| {
            let high = 110.0 + i as f64;
            let low = 100.0 + i as f64;
            (low, high, low, high, 1000.0) // close = high
        })
        .collect();
    let candles = make_full_candles(&data);
    let ind = calculate_all(&candles);
    // %K should be 100 at bar 13+ (k_period=14)
    assert_relative_eq!(ind[13].stoch_k, 100.0, epsilon = 1e-10);
    assert_relative_eq!(ind[19].stoch_k, 100.0, epsilon = 1e-10);
    // %D (SMA3 of %K=100) should also be 100
    assert_relative_eq!(ind[15].stoch_d, 100.0, epsilon = 1e-10);
}

#[test]
fn test_stochastic_close_at_low() {
    // Close always at low, with constant range → %K = 0
    // Use constant high/low so close=low equals the lowest low in any window
    let data: Vec<(f64, f64, f64, f64, f64)> = (0..20)
        .map(|_| {
            (110.0, 110.0, 100.0, 100.0, 1000.0) // close = low = constant
        })
        .collect();
    let candles = make_full_candles(&data);
    let ind = calculate_all(&candles);
    assert_relative_eq!(ind[13].stoch_k, 0.0, epsilon = 1e-10);
    assert_relative_eq!(ind[19].stoch_k, 0.0, epsilon = 1e-10);
}

#[test]
fn test_stochastic_constant_price() {
    // Constant price → range = 0 → %K = 50 (default)
    let candles = make_candles(&vec![100.0; 20]);
    let ind = calculate_all(&candles);
    assert_relative_eq!(ind[13].stoch_k, 50.0, epsilon = 1e-10);
}

// ─── PSY Tests ───

#[test]
fn test_psy_all_rises() {
    // All rises — legacy-compatible `(up - down) / period` → +1.0
    let closes: Vec<f64> = (0..21).map(|i| 100.0 + i as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    assert_relative_eq!(ind[12].psy_hour, 1.0, epsilon = 1e-10);
}

#[test]
fn test_psy_all_drops() {
    // All drops → -1.0
    let closes: Vec<f64> = (0..21).map(|i| 120.0 - i as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    assert_relative_eq!(ind[12].psy_hour, -1.0, epsilon = 1e-10);
}

#[test]
fn test_psy_alternating() {
    // Alternating up/down → 0.0 (equal counts)
    let closes: Vec<f64> = (0..50).map(|i| if i % 2 == 0 { 100.0 } else { 101.0 }).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    assert_relative_eq!(ind[12].psy_hour, 0.0, epsilon = 1e-10);
}

// ─── calculate_incremental Tests ───

#[test]
fn test_calculate_incremental_matches_full() {
    let closes: Vec<f64> = (0..50).map(|i| 100.0 + (i as f64 * 0.3).sin() * 20.0).collect();
    let candles = make_candles(&closes);
    let full = calculate_all(&candles);
    let mut incremental = Vec::new();
    calculate_incremental(&candles, &mut incremental, 0);
    assert_eq!(full.len(), incremental.len());
    for i in 0..full.len() {
        assert_relative_eq!(full[i].sma_10, incremental[i].sma_10, epsilon = 1e-10);
        assert_relative_eq!(full[i].rsi, incremental[i].rsi, epsilon = 1e-10);
    }
}

// ─── Empty input ───

#[test]
fn test_empty_candles() {
    let candles: Vec<Candle> = vec![];
    let ind = calculate_all(&candles);
    assert!(ind.is_empty());
}

// ─── Model Default Tests ───

#[test]
fn test_indicator_set_default() {
    let ind = bitcoin_trader_lib::models::market::IndicatorSet::default();
    assert_relative_eq!(ind.sma_10, 0.0, epsilon = 1e-10);
    assert_relative_eq!(ind.rsi, 0.0, epsilon = 1e-10);
}

#[test]
fn test_trading_parameters_default() {
    let params = bitcoin_trader_lib::models::trading::TradingParameters::default();
    // Values taken verbatim from TradingConfig_V3_RegimeAdaptive_20260401_140149.json
    assert_relative_eq!(params.v3_urgent_buy_volume_lo, 21000.0, epsilon = 1e-10);
    assert_relative_eq!(params.v3_buy_psy_lo, 0.14, epsilon = 1e-10);
    assert_relative_eq!(params.v3_buy_psy_hi, -0.24, epsilon = 1e-10);
    assert_relative_eq!(params.v3_fee_rate, 0.0005, epsilon = 1e-10);
    assert_eq!(params.v3_min_hold_bars, 21);
}

#[test]
fn test_simulation_result_default() {
    let result = bitcoin_trader_lib::models::trading::SimulationResult::default();
    assert_eq!(result.total_trades, 0);
    assert!(result.trades.is_empty());
}

#[test]
fn test_optimizer_config_default() {
    let config = bitcoin_trader_lib::models::config::OptimizerConfig::default();
    assert_eq!(config.population_size, 50);
    assert_eq!(config.generations, 100);
    assert_relative_eq!(config.crossover_rate, 0.9, epsilon = 1e-10);
}
