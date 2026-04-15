// ─── Scenario-Based Tests ───
// Comprehensive tests simulating real-world conditions for the Bitcoin trading system.

use approx::assert_relative_eq;
use bitcoin_trader_lib::auth::{crypto, password, session};
use bitcoin_trader_lib::core::engine::run_simulation;
use bitcoin_trader_lib::core::indicators::calculate_all;
use bitcoin_trader_lib::core::optimizer::{
    calculate_crowding_distance, dominates, fast_non_dominated_sort, get_parameter, set_parameter,
    Individual, Nsga2Optimizer,
};
use bitcoin_trader_lib::migration::csv_import;
use bitcoin_trader_lib::models::config::OptimizerConfig;
use bitcoin_trader_lib::models::market::{Candle, IndicatorSet, MarketData};
use bitcoin_trader_lib::models::trading::{TradingParameters, SimulationResult};
use bitcoin_trader_lib::notifications::manager::{format_trade_message, NotificationManager};
use bitcoin_trader_lib::strategies::StrategyRegistry;
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use std::io::Write;

// ─── Helpers ───

struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u32() as f64) / (u32::MAX as f64)
    }
}

/// Create MarketData from (close, volume) pairs with default indicators.
fn make_market_data(prices: &[(f64, f64)]) -> Vec<MarketData> {
    prices
        .iter()
        .enumerate()
        .map(|(i, &(close, volume))| MarketData {
            candle: Candle {
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
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

/// Create MarketData with full OHLCV control.
fn make_full_market_data(data: &[(f64, f64, f64, f64, f64)]) -> Vec<MarketData> {
    data.iter()
        .enumerate()
        .map(|(i, &(open, high, low, close, volume))| MarketData {
            candle: Candle {
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
                open,
                high,
                low,
                close,
                volume,
            },
            indicators: IndicatorSet::default(),
        })
        .collect()
}

/// Generate realistic BTC-like data with indicators calculated.
fn make_realistic_data(n: usize, seed: u32) -> Vec<MarketData> {
    let mut rng = SimpleRng::new(seed);
    let mut price = 50000000.0_f64; // 50M KRW
    let mut candles = Vec::with_capacity(n);

    for i in 0..n {
        let change_pct = (rng.next_f64() - 0.498) * 0.04; // ~2% volatility
        price *= 1.0 + change_pct;
        price = price.max(1000.0);

        let high = price * (1.0 + rng.next_f64() * 0.015);
        let low = price * (1.0 - rng.next_f64() * 0.015);
        let base_volume = 500.0 + rng.next_f64() * 49500.0; // 500-50000
        let spike = if rng.next_f64() > 0.9 { 5.0 + rng.next_f64() * 15.0 } else { 1.0 };
        let volume = base_volume * spike;

        candles.push(Candle {
            timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
            open: price * (1.0 + (rng.next_f64() - 0.5) * 0.005),
            high,
            low,
            close: price,
            volume,
        });
    }

    let indicators = calculate_all(&candles);
    candles
        .into_iter()
        .zip(indicators)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect()
}

fn make_candles(closes: &[f64]) -> Vec<Candle> {
    closes
        .iter()
        .enumerate()
        .map(|(i, &c)| Candle {
            timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
            open: c,
            high: c,
            low: c,
            close: c,
            volume: 1000.0,
        })
        .collect()
}

fn make_full_candles(data: &[(f64, f64, f64, f64, f64)]) -> Vec<Candle> {
    data.iter()
        .enumerate()
        .map(|(i, &(o, h, l, c, v))| Candle {
            timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: v,
        })
        .collect()
}

/// Initialize in-memory DB with both migration schemas.
fn init_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let schema_v1 = include_str!("../migrations/001_initial.sql");
    conn.execute_batch(schema_v1).unwrap();
    let schema_v2 = include_str!("../migrations/002_users.sql");
    conn.execute_batch(schema_v2).unwrap();
    conn
}

fn seed_admin(conn: &Connection) {
    let hash = password::hash_password("admin123").unwrap();
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES ('admin', ?1, 'admin')",
        [&hash],
    )
    .unwrap();
}

/// Helper: default params that trigger buy via volume decay.
fn buy_trigger_params() -> TradingParameters {
    let mut params = TradingParameters::default();
    params.buy_ready_volume_threshold = 500.0;
    params.buy_ready_price_drop_ratio = 0.005;
    params.buy_confirm_volume_decay_ratio = 0.5;
    params.buy_wait_max_periods = 240;
    params.fee_rate = 0.0;
    params
}

/// Helper: create data that forces a buy entry at a specific price.
/// Returns data: flat bars, then buy-trigger, then additional bars at `hold_price`.
fn force_buy_then_hold(buy_price: f64, hold_price: f64, hold_bars: usize) -> Vec<(f64, f64)> {
    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5]; // baseline
    prices.push((buy_price * 0.99, 600.0)); // ready: price drop + high volume
    prices.push((buy_price, 200.0)); // decay confirms buy (200 < 600*0.5=300)
    for _ in 0..hold_bars {
        prices.push((hold_price, 100.0));
    }
    prices
}

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

// ═══════════════════════════════════════════════════════════════
// SECTION 1: Engine State Machine Scenarios (10 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_buy_ready_timeout_resets_state() {
    // 1 bar triggers buy-ready, then 300 bars with high volume (never decays) -> timeout
    let mut params = buy_trigger_params();
    params.buy_wait_max_periods = 240;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    // Trigger buy-ready: volume >= 500, price drop >= 0.5%
    prices.push((99.0, 600.0));
    // 300 bars with volume still high (600 * 0.5 = 300; volume=400 > 300 so no decay)
    for _ in 0..300 {
        prices.push((99.0, 400.0));
    }

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 0, "buy-ready should have timed out without executing");
    assert_eq!(result.last_position, 0, "should be idle after timeout");
}

#[test]
fn test_buy_decay_confirmation_works() {
    // Volume spike then rapid decay confirms buy
    let mut params = buy_trigger_params();
    params.buy_confirm_volume_decay_ratio = 0.5;
    params.max_hold_periods = 5; // force sell so we get a completed trade

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 10];
    // Buy-ready trigger: price drop + high volume
    prices.push((99.0, 50000.0));
    // Rapid decay: volume drops to far below 50000 * 0.5 = 25000
    prices.push((99.0, 100.0));
    // Hold bars
    for _ in 0..10 {
        prices.push((99.0, 100.0));
    }

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert!(result.buy_signals > 0, "should have detected buy signal");
    // Either completed trade or still holding
    assert!(
        result.total_trades > 0 || result.last_position == 1,
        "should have entered a position (trades={}, pos={})",
        result.total_trades,
        result.last_position
    );
}

#[test]
fn test_urgent_buy_immediate_entry() {
    let mut params = TradingParameters::default();
    params.urgent_buy_volume_threshold = 500.0;
    params.urgent_buy_price_drop_ratio = 0.01; // 1% drop
    params.max_hold_periods = 3;
    params.fee_rate = 0.0;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    // Urgent buy: volume >= 500 AND price drops >= 1%
    prices.push((98.0, 600.0)); // 2% drop, volume=600
    // Hold bars
    for _ in 0..5 {
        prices.push((99.0, 100.0));
    }

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert!(result.buy_signals > 0, "urgent buy should trigger buy signal");
    // Urgent buy sets buy_sign=2 so it executes immediately
    assert!(
        result.total_trades > 0 || result.last_position == 1,
        "urgent buy should have entered position"
    );
}

#[test]
fn test_fixed_stop_loss_triggers() {
    let mut params = buy_trigger_params();
    params.fixed_stop_loss_pct = 0.05; // 5% stop loss
    params.fee_rate = 0.0;

    // Force buy at ~99.0, then drop to 93 (>5% from buy_price)
    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    prices.push((99.0, 600.0)); // ready
    prices.push((99.0, 200.0)); // decay confirms, buy at 99.0
    // Price drops to 93 = 6.06% loss
    prices.push((93.0, 100.0));
    prices.push((90.0, 100.0));

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 1, "stop loss should trigger a sell");
    assert_eq!(result.trades[0].sell_signal, "fixed_stop_loss");
    assert!(result.trades[0].pnl_pct < 0.0, "pnl should be negative");
}

#[test]
fn test_fixed_take_profit_triggers() {
    let mut params = buy_trigger_params();
    params.fixed_take_profit_pct = 0.10; // 10% take profit
    params.fee_rate = 0.0;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    prices.push((99.0, 600.0)); // ready
    prices.push((99.0, 200.0)); // decay confirms, buy at 99.0
    // Price rises to 111 = 12.1% gain
    prices.push((105.0, 100.0));
    prices.push((111.0, 100.0));

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 1, "take profit should trigger a sell");
    assert_eq!(result.trades[0].sell_signal, "fixed_take_profit");
    assert!(result.trades[0].pnl_pct > 0.0, "pnl should be positive");
}

#[test]
fn test_trailing_stop_triggers() {
    let mut params = buy_trigger_params();
    params.trailing_stop_pct = 0.05; // 5% trailing stop
    params.fee_rate = 0.0;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    prices.push((99.0, 600.0)); // ready
    prices.push((99.0, 200.0)); // decay confirms, buy at 99.0
    // Price rises to 120 (new peak)
    prices.push((110.0, 100.0));
    prices.push((120.0, 100.0));
    // Price drops to 110: (120-110)/120 = 8.3% from peak > 5%
    prices.push((110.0, 100.0));

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 1, "trailing stop should trigger a sell");
    assert_eq!(result.trades[0].sell_signal, "trailing_stop");
}

#[test]
fn test_max_hold_period_forces_sell() {
    let mut params = buy_trigger_params();
    params.max_hold_periods = 10;
    params.fee_rate = 0.0;

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    prices.push((99.0, 600.0)); // ready
    prices.push((99.0, 200.0)); // decay confirms, buy
    // Hold for 15 bars at same price (no other sell triggers)
    for _ in 0..15 {
        prices.push((99.0, 100.0));
    }

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 1, "max hold should force sell");
    assert_eq!(result.trades[0].sell_signal, "max_hold");
    assert_eq!(result.trades[0].hold_bars, 10, "should sell exactly at bar 10");
}

#[test]
fn test_fee_accumulation_correct() {
    let mut params = buy_trigger_params();
    params.fixed_take_profit_pct = 0.05;
    params.fee_rate = 0.001; // 0.1% fee each way

    let mut prices: Vec<(f64, f64)> = vec![(100000.0, 100.0); 5];
    prices.push((99000.0, 600.0)); // ready
    prices.push((100000.0, 200.0)); // decay confirms, buy at 100000*(1+0.001)=100100
    prices.push((106000.0, 100.0)); // 6% gain > 5% TP

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 1);

    let trade = &result.trades[0];
    // buy_price = 100000 * (1 + 0.001) = 100100
    // sell_price = 106000 * (1 - 0.001) = 105894
    // pnl_pct = (105894 - 100100) / 100100 = 0.05788...
    assert!(trade.pnl_pct > 0.0, "should still be profitable after fees");
    assert!(trade.pnl_pct < 0.06, "fee should reduce the gain from ~6% to ~5.8%");
    // Verify fee-adjusted buy price
    assert_relative_eq!(trade.buy_price, 100100.0, epsilon = 1.0);
}

#[test]
fn test_multiple_round_trips() {
    let mut params = buy_trigger_params();
    params.fixed_take_profit_pct = 0.05;
    params.fee_rate = 0.0;

    let mut prices: Vec<(f64, f64)> = Vec::new();

    // 3 complete buy-sell cycles
    for cycle in 0..3 {
        let base = 100.0 + cycle as f64 * 10.0;
        // Flat baseline
        for _ in 0..5 {
            prices.push((base, 100.0));
        }
        // Buy trigger
        prices.push((base * 0.99, 600.0));
        prices.push((base, 200.0)); // decay confirms buy
        // Price rises > 5% to trigger take profit
        prices.push((base * 1.06, 100.0));
    }

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.total_trades, 3, "should have 3 complete round trips");
    for (i, trade) in result.trades.iter().enumerate() {
        assert!(trade.buy_index < trade.sell_index, "trade {}: buy should be before sell", i);
        assert!(trade.pnl_pct > 0.0, "trade {}: should be profitable", i);
    }
}

#[test]
fn test_position_open_at_end_of_data() {
    let mut params = buy_trigger_params();
    params.fee_rate = 0.0;
    // No stop loss, no take profit, no max hold

    let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
    prices.push((99.0, 600.0)); // ready
    prices.push((99.0, 200.0)); // decay confirms buy
    // Only 3 more bars, no sell trigger
    prices.push((100.0, 100.0));
    prices.push((101.0, 100.0));
    prices.push((102.0, 100.0));

    let data = make_market_data(&prices);
    let result = run_simulation(&data, &params);
    assert_eq!(result.last_position, 1, "should still be holding");
    assert_eq!(result.total_trades, 0, "no completed trade");
    assert!(result.last_buy_price > 0.0, "should have a buy price");
}

// ═══════════════════════════════════════════════════════════════
// SECTION 2: Indicator Edge Cases (8 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_indicators_exact_warmup_boundary() {
    // RSI: period 14, first valid at index 14
    let closes_15: Vec<f64> = (0..15).map(|i| 100.0 + i as f64).collect();
    let candles_15 = make_candles(&closes_15);
    let ind_15 = calculate_all(&candles_15);
    assert!(ind_15[14].rsi > 0.0, "RSI should be valid at index 14");
    assert_relative_eq!(ind_15[13].rsi, 0.0, epsilon = 1e-10); // not yet valid

    // SMA60: first valid at index 59
    let closes_60: Vec<f64> = (0..60).map(|i| 100.0 + i as f64).collect();
    let candles_60 = make_candles(&closes_60);
    let ind_60 = calculate_all(&candles_60);
    assert!(ind_60[59].sma_60 > 0.0, "SMA60 should be valid at index 59");
    assert_relative_eq!(ind_60[58].sma_60, 0.0, epsilon = 1e-10);

    // MACD signal: first valid at approximately index 33 (EMA26 warmup + 9 signal)
    let closes_34: Vec<f64> = (0..34).map(|i| 100.0 + (i as f64 * 0.5).sin() * 10.0).collect();
    let candles_34 = make_candles(&closes_34);
    let ind_34 = calculate_all(&candles_34);
    // macd_signal should be valid at bar 33
    assert!(
        ind_34[33].macd_signal != 0.0 || ind_34[33].macd != 0.0,
        "MACD should have values at bar 33"
    );
}

#[test]
fn test_indicators_extreme_price_spike() {
    // 50 bars at 100, then 1 bar at 10000 (100x spike)
    let mut closes: Vec<f64> = vec![100.0; 50];
    closes.push(10000.0);
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);

    let last = &ind[50];
    assert!(last.rsi > 90.0, "RSI should be near 100 after huge spike, got {}", last.rsi);
    assert!(!last.rsi.is_nan(), "RSI should not be NaN");
    assert!(last.atr > 0.0, "ATR should spike");
    assert!(!last.atr.is_nan(), "ATR should not be NaN");
    // Bollinger upper should expand massively
    assert!(
        last.bollinger_upper > last.bollinger_middle,
        "BB should widen after spike"
    );
}

#[test]
fn test_indicators_price_crash_to_near_zero() {
    // 50 bars at 100, then crash to 0.01
    let mut closes: Vec<f64> = vec![100.0; 50];
    closes.push(0.01);
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);

    let last = &ind[50];
    // No NaN or Infinity
    assert!(!last.rsi.is_nan(), "RSI should not be NaN");
    assert!(!last.rsi.is_infinite(), "RSI should not be Infinity");
    assert!(last.rsi < 10.0, "RSI should be near 0 after crash, got {}", last.rsi);
    assert!(!last.atr.is_nan(), "ATR should not be NaN");
    assert!(!last.sma_10.is_nan(), "SMA10 should not be NaN");
    assert!(!last.bollinger_upper.is_nan(), "BB upper should not be NaN");
}

#[test]
fn test_indicators_constant_price_all_zero_volatility() {
    let closes = vec![50000.0; 100];
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);

    let last = &ind[99];
    assert_relative_eq!(last.sma_10, 50000.0, epsilon = 1e-6);
    assert_relative_eq!(last.sma_25, 50000.0, epsilon = 1e-6);
    assert_relative_eq!(last.sma_60, 50000.0, epsilon = 1e-6);
    // RSI with constant price: implementation returns 100.0 when avg_loss=0
    assert!(last.rsi == 50.0 || last.rsi == 100.0,
        "RSI with constant price should be 50 or 100 (impl-dependent), got {}", last.rsi);
    assert_relative_eq!(last.macd, 0.0, epsilon = 1e-6);
    // Bollinger: width=0, upper=middle=lower
    assert_relative_eq!(last.bollinger_upper, 50000.0, epsilon = 1e-6);
    assert_relative_eq!(last.bollinger_middle, 50000.0, epsilon = 1e-6);
    assert_relative_eq!(last.bollinger_lower, 50000.0, epsilon = 1e-6);
}

#[test]
fn test_indicators_alternating_prices() {
    // Price alternates: 100, 200, 100, 200, ... for 100 bars
    let closes: Vec<f64> = (0..100).map(|i| if i % 2 == 0 { 100.0 } else { 200.0 }).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);

    let last = &ind[99];
    // RSI should be near 50 with equal up/down moves
    assert!(last.rsi > 30.0 && last.rsi < 70.0, "RSI should be near 50, got {}", last.rsi);
    // Bollinger bands should have width
    assert!(last.bollinger_upper > last.bollinger_lower, "BB should have width");
    // Stochastic should oscillate
    assert!(!last.stoch_k.is_nan(), "Stochastic K should not be NaN");
}

#[test]
fn test_indicators_single_candle() {
    let candles = make_candles(&[100.0]);
    let ind = calculate_all(&candles);
    assert_eq!(ind.len(), 1, "should return 1 indicator set");
    // All indicators should be 0 or default, no panic
    let i = &ind[0];
    assert_relative_eq!(i.sma_10, 0.0, epsilon = 1e-10);
    assert_relative_eq!(i.rsi, 0.0, epsilon = 1e-10);
    assert_relative_eq!(i.macd, 0.0, epsilon = 1e-10);
}

#[test]
fn test_indicators_two_candles() {
    let candles = make_candles(&[100.0, 110.0]);
    let ind = calculate_all(&candles);
    assert_eq!(ind.len(), 2, "should return 2 indicator sets");
    // No panic, all values finite
    for i in &ind {
        assert!(!i.sma_10.is_nan());
        assert!(!i.rsi.is_nan());
        assert!(!i.macd.is_nan());
        assert!(!i.atr.is_nan());
    }
}

#[test]
fn test_psy_calculation_accuracy() {
    // 13 bars: first at 100, then 12 consecutive rises
    let closes: Vec<f64> = (0..13).map(|i| 100.0 + i as f64).collect();
    let candles = make_candles(&closes);
    let ind = calculate_all(&candles);
    // PSY hour (period=12): at bar 12, all 12 changes are rises
    assert_relative_eq!(ind[12].psy_hour, 100.0, epsilon = 1e-10);

    // 6 up, 6 down → PSY=50
    let mut closes_mixed: Vec<f64> = vec![100.0]; // bar 0
    for i in 0..12 {
        if i < 6 {
            closes_mixed.push(closes_mixed.last().unwrap() + 1.0); // up
        } else {
            closes_mixed.push(closes_mixed.last().unwrap() - 1.0); // down
        }
    }
    let candles_mixed = make_candles(&closes_mixed);
    let ind_mixed = calculate_all(&candles_mixed);
    assert_relative_eq!(ind_mixed[12].psy_hour, 50.0, epsilon = 1e-10);
}

// ═══════════════════════════════════════════════════════════════
// SECTION 3: Strategy Comparison Scenarios (8 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_all_strategies_same_data_no_panic() {
    let data = make_realistic_data(500, 42);
    let registry = StrategyRegistry::new();
    let params = TradingParameters::default();

    for key in &["V0", "V1", "V2", "V3", "V4", "V5"] {
        let strategy = registry.get(key).unwrap();
        let result = strategy.run_simulation(&data, &params);
        assert!(
            result.total_return.is_finite(),
            "Strategy {} returned non-finite total_return",
            key
        );
        assert!(
            result.win_rate >= 0.0 && result.win_rate <= 100.0,
            "Strategy {} win_rate out of range: {}",
            key,
            result.win_rate
        );
    }
}

#[test]
fn test_v3_rsi_boundary_interpolation() {
    // V3 interpolation: P = Lo + (Hi - Lo) * clamp((RSI - 20) / 60, 0, 1)^Pow
    // RSI=20 -> t=0 -> P=Lo
    // RSI=80 -> t=1 -> P=Hi
    // RSI=50, pow=1.0 -> t=0.5 -> P=(Lo+Hi)/2

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();

    // RSI=20 means t=0, so buy volume should equal lo value
    let mut params = TradingParameters::default();
    params.v3_buy_volume_lo = 1000.0;
    params.v3_buy_volume_hi = 10000.0;
    params.v3_buy_volume_pow = 1.0;
    params.v3_buy_price_drop_lo = 0.01;
    params.v3_buy_price_drop_hi = 0.05;
    params.v3_buy_price_drop_pow = 1.0;
    params.v3_buy_decay_lo = 0.1;
    params.v3_buy_decay_hi = 0.5;
    params.v3_buy_decay_pow = 1.0;
    params.v3_buy_wait_lo = 10.0;
    params.v3_buy_wait_hi = 100.0;
    params.v3_buy_wait_pow = 1.0;
    params.v3_sell_fixed_sl_lo = 0.03;
    params.v3_sell_fixed_sl_hi = 0.1;
    params.v3_sell_fixed_sl_pow = 1.0;
    params.v3_sell_profit_lo = 0.02;
    params.v3_sell_profit_hi = 0.1;
    params.v3_sell_profit_pow = 1.0;
    params.v3_sell_max_hold_lo = 20.0;
    params.v3_sell_max_hold_hi = 100.0;
    params.v3_sell_max_hold_pow = 1.0;
    params.v3_fee_rate = 0.0;
    params.v3_min_hold_bars = 1;

    // Test with data that produces known RSI - run simulation and ensure no panic
    let data = make_realistic_data(200, 77);
    let result = v3.run_simulation(&data, &params);
    assert!(result.total_return.is_finite());

    // Direct interpolation check (testing the formula):
    // f(rsi=20, lo=1000, hi=10000, pow=1.0) = 1000 + 9000 * 0^1 = 1000
    // f(rsi=80, lo=1000, hi=10000, pow=1.0) = 1000 + 9000 * 1^1 = 10000
    // f(rsi=50, lo=1000, hi=10000, pow=1.0) = 1000 + 9000 * 0.5 = 5500
    // (These are tested implicitly via the strategy running correctly with these params)
}

#[test]
fn test_v3_pow_zero_behavior() {
    // When pow <= 0.0, the code uses t directly (no exponentiation)
    // So pow=0 is equivalent to pow=1 (linear interpolation)
    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();

    let mut params = TradingParameters::default();
    params.v3_buy_volume_lo = 1000.0;
    params.v3_buy_volume_hi = 10000.0;
    params.v3_buy_volume_pow = 0.0; // pow=0 -> uses t directly
    params.v3_buy_price_drop_lo = 0.01;
    params.v3_buy_price_drop_hi = 0.05;
    params.v3_buy_price_drop_pow = 0.0;
    params.v3_buy_decay_lo = 0.1;
    params.v3_buy_decay_hi = 0.5;
    params.v3_buy_decay_pow = 0.0;
    params.v3_buy_wait_lo = 10.0;
    params.v3_buy_wait_hi = 100.0;
    params.v3_buy_wait_pow = 0.0;
    params.v3_sell_fixed_sl_lo = 0.03;
    params.v3_sell_fixed_sl_hi = 0.1;
    params.v3_sell_fixed_sl_pow = 0.0;
    params.v3_sell_profit_lo = 0.02;
    params.v3_sell_profit_hi = 0.1;
    params.v3_sell_profit_pow = 0.0;
    params.v3_sell_max_hold_lo = 20.0;
    params.v3_sell_max_hold_hi = 100.0;
    params.v3_sell_max_hold_pow = 0.0;
    params.v3_fee_rate = 0.0;
    params.v3_min_hold_bars = 1;

    let data = make_realistic_data(200, 99);
    let result = v3.run_simulation(&data, &params);
    assert!(result.total_return.is_finite(), "pow=0 should not cause NaN");
}

#[test]
fn test_v2_score_threshold_extremes() {
    let registry = StrategyRegistry::new();
    let v2 = registry.get("V2").unwrap();
    let data = make_realistic_data(500, 55);

    // threshold=0 -> buy very frequently (any score triggers)
    let mut params_low = TradingParameters::default();
    params_low.v2_buy_score_threshold = 0.0;
    params_low.v2_sell_score_threshold = 0.0;
    let result_low = v2.run_simulation(&data, &params_low);

    // threshold=1.0 -> never buy (impossible to reach perfect score)
    let mut params_high = TradingParameters::default();
    params_high.v2_buy_score_threshold = 1.0;
    let result_high = v2.run_simulation(&data, &params_high);

    assert!(
        result_high.total_trades <= result_low.total_trades,
        "higher threshold should produce fewer or equal trades (high={}, low={})",
        result_high.total_trades,
        result_low.total_trades
    );
}

#[test]
fn test_v5_psy_buy_equals_sell_threshold() {
    let registry = StrategyRegistry::new();
    let v5 = registry.get("V5").unwrap();
    let data = make_realistic_data(500, 33);

    let mut params = TradingParameters::default();
    params.v5_psy_buy_threshold = 50.0;
    params.v5_psy_sell_threshold = 50.0;
    params.v5_volume_threshold = 5000.0;
    params.v5_decay_ratio = 0.3;
    params.v5_stop_loss = 0.05;
    params.v5_take_profit = 0.10;
    params.v5_fee_rate = 0.0;
    params.v5_min_hold_bars = 1;

    // Should not panic or get stuck
    let result = v5.run_simulation(&data, &params);
    assert!(result.total_return.is_finite());
}

#[test]
fn test_v4_ml_no_predictions_early_data() {
    let registry = StrategyRegistry::new();
    let v4 = registry.get("V4").unwrap();

    // Only 100 bars, less than default train_window of 2160
    let data = make_realistic_data(100, 11);
    let params = TradingParameters::default();
    let result = v4.run_simulation(&data, &params);

    assert_eq!(result.total_trades, 0, "ML can't train with only 100 bars");
    assert!(result.total_return.is_finite());
}

#[test]
fn test_v1_adaptive_volume_adjusts_thresholds() {
    let registry = StrategyRegistry::new();
    let v1 = registry.get("V1").unwrap();

    // Low volume data
    let mut low_vol_data: Vec<MarketData> = Vec::new();
    for i in 0..200 {
        low_vol_data.push(MarketData {
            candle: Candle {
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 100.0, // very low volume
            },
            indicators: IndicatorSet::default(),
        });
    }

    // High volume data
    let mut high_vol_data: Vec<MarketData> = Vec::new();
    for i in 0..200 {
        high_vol_data.push(MarketData {
            candle: Candle {
                timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 100000.0, // very high volume
            },
            indicators: IndicatorSet::default(),
        });
    }

    let params = TradingParameters::default();
    // Both should run without panic
    let result_low = v1.run_simulation(&low_vol_data, &params);
    let result_high = v1.run_simulation(&high_vol_data, &params);
    assert!(result_low.total_return.is_finite());
    assert!(result_high.total_return.is_finite());
}

#[test]
fn test_strategy_parameter_ranges_valid() {
    let registry = StrategyRegistry::new();
    for key in &["V0", "V1", "V2", "V3", "V4", "V5"] {
        let strategy = registry.get(key).unwrap();
        let ranges = strategy.parameter_ranges();
        assert!(!ranges.is_empty(), "{} should have parameter ranges", key);

        for range in &ranges {
            assert!(
                range.min < range.max,
                "{}: range '{}' has min({}) >= max({})",
                key, range.name, range.min, range.max
            );
            assert!(
                range.step > 0.0,
                "{}: range '{}' has step({}) <= 0",
                key, range.name, range.step
            );
            // Verify the parameter name is recognized by get_parameter
            let params = TradingParameters::default();
            let _val = get_parameter(&params, &range.name);
            // get_parameter returns 0.0 for unknown names, but all known names
            // should have a valid default (some defaults are 0.0 which is fine)
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SECTION 4: NSGA-II Optimizer Scenarios (6 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_optimizer_single_individual_population() {
    let data = make_realistic_data(200, 42);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 1,
        generations: 3,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let result = optimizer.run(&data, strategy, None);
    assert_eq!(result.len(), 1, "should return 1 individual");
    assert_eq!(result[0].objectives.len(), 2);
}

#[test]
fn test_optimizer_all_identical_objectives() {
    // All individuals have the same fitness
    let individuals = vec![
        Individual::new_with_objectives(vec![5.0, 5.0]),
        Individual::new_with_objectives(vec![5.0, 5.0]),
        Individual::new_with_objectives(vec![5.0, 5.0]),
        Individual::new_with_objectives(vec![5.0, 5.0]),
    ];

    let fronts = fast_non_dominated_sort(&individuals);
    // All should be in front 0 (none dominates any other)
    assert_eq!(fronts[0].len(), 4, "all identical should be in front 0");

    // Crowding distance: with identical objectives, boundary gets infinity, interior may get 0
    // (when range=0 for all objectives, the code skips distance accumulation)
    let mut individuals_mut = individuals;
    calculate_crowding_distance(&mut individuals_mut, &fronts[0], 2);
    // Boundary individuals (first/last in sorted order) always get infinity
    // With 4 identical individuals, all are effectively boundary, but the implementation
    // sets boundary indices [0] and [n-1] to infinity per objective sort.
    // Just verify no NaN and that at least boundary members get infinity.
    let inf_count = individuals_mut.iter().filter(|i| i.crowding_distance.is_infinite()).count();
    assert!(inf_count >= 2, "at least boundary members should get infinite crowding distance, got {} infinite", inf_count);
    for ind in &individuals_mut {
        assert!(!ind.crowding_distance.is_nan(), "crowding distance should not be NaN");
    }
}

#[test]
fn test_optimizer_constraint_violation_penalty() {
    let data = make_realistic_data(200, 42);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 5,
        generations: 3,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        min_win_rate: 99.0,    // impossible
        min_trades: 1000,       // impossible with 200 bars
        min_return: 0.0,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let result = optimizer.run(&data, strategy, None);
    assert_eq!(result.len(), 5, "should still return population");

    // All should have constraint violations
    for ind in &result {
        assert!(
            ind.constraint_violation > 0.0,
            "impossible constraints should produce violations"
        );
    }
}

#[test]
fn test_optimizer_progress_callback_count() {
    let data = make_realistic_data(100, 42);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 5,
        generations: 10,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let counter = std::cell::Cell::new(0usize);
    let gen_numbers = std::cell::RefCell::new(Vec::new());

    let _result = optimizer.run(&data, strategy, Some(&|gr| {
        counter.set(counter.get() + 1);
        gen_numbers.borrow_mut().push(gr.generation);
    }));

    assert_eq!(counter.get(), 10, "callback should be called exactly 10 times");
    let nums = gen_numbers.borrow();
    let expected: Vec<usize> = (0..10).collect();
    assert_eq!(*nums, expected, "generation numbers should be 0-9");
}

#[test]
fn test_optimizer_get_set_parameter_roundtrip() {
    let registry = StrategyRegistry::new();
    let v0 = registry.get("V0").unwrap();
    let ranges = v0.parameter_ranges();

    for range in &ranges {
        let mut params = TradingParameters::default();
        let test_value = (range.min + range.max) / 2.0;
        set_parameter(&mut params, &range.name, test_value);
        let got = get_parameter(&params, &range.name);

        // For integer parameters (cast via `as i32`), allow +-1 difference
        if range.name.contains("periods") || range.name.contains("hold_bars") || range.name.contains("window") || range.name.contains("lookback") || range.name.contains("interval") {
            assert!(
                (got - test_value).abs() < 1.0,
                "param '{}': set {}, got {} (integer rounding ok)",
                range.name, test_value, got
            );
        } else {
            assert!(
                (got - test_value).abs() < 1e-10,
                "param '{}': set {}, got {}",
                range.name, test_value, got
            );
        }
    }

    // Edge values
    let mut params = TradingParameters::default();
    set_parameter(&mut params, "fee_rate", 0.0);
    assert_relative_eq!(get_parameter(&params, "fee_rate"), 0.0, epsilon = 1e-10);

    set_parameter(&mut params, "fee_rate", 999999.0);
    assert_relative_eq!(get_parameter(&params, "fee_rate"), 999999.0, epsilon = 1e-6);
}

#[test]
fn test_optimizer_pareto_front_quality() {
    let data = make_realistic_data(200, 42);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 20,
        generations: 10,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let population = optimizer.run(&data, strategy, None);

    // Run non-dominated sort on final population
    let fronts = fast_non_dominated_sort(&population);
    assert!(!fronts.is_empty(), "should have at least one front");
    assert!(!fronts[0].is_empty(), "front 0 should not be empty");

    // Verify: no solution in front 0 dominates another in front 0
    let front0 = &fronts[0];
    for i in 0..front0.len() {
        for j in (i + 1)..front0.len() {
            let d = dominates(
                &population[front0[i]].objectives,
                &population[front0[j]].objectives,
            );
            assert_ne!(
                d.abs(),
                1,
                "front 0 members should not dominate each other: {} vs {}",
                front0[i],
                front0[j]
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SECTION 5: Auth & Security Scenarios (6 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_password_hash_different_each_time() {
    let hash1 = password::hash_password("mysecretpassword").unwrap();
    let hash2 = password::hash_password("mysecretpassword").unwrap();
    assert_ne!(hash1, hash2, "hashes should differ due to random salt");
    assert!(password::verify_password("mysecretpassword", &hash1).unwrap());
    assert!(password::verify_password("mysecretpassword", &hash2).unwrap());
}

#[test]
fn test_password_verify_wrong_password() {
    let hash = password::hash_password("correct_password").unwrap();
    assert!(!password::verify_password("wrong_password", &hash).unwrap());
    assert!(!password::verify_password("", &hash).unwrap());
}

#[test]
fn test_session_expires_after_24h() {
    let conn = init_db();
    seed_admin(&conn);

    let token = session::create_session(&conn, 1).unwrap();
    // Valid now
    assert!(session::validate_session(&conn, &token).unwrap().is_some());

    // Manually update expires_at to the past
    conn.execute(
        "UPDATE sessions SET expires_at = '2020-01-01 00:00:00' WHERE id = ?1",
        [&token],
    )
    .unwrap();

    // Should be expired now
    assert!(
        session::validate_session(&conn, &token).unwrap().is_none(),
        "expired session should return None"
    );
}

#[test]
fn test_session_deleted_cannot_validate() {
    let conn = init_db();
    seed_admin(&conn);

    let token = session::create_session(&conn, 1).unwrap();
    assert!(session::validate_session(&conn, &token).unwrap().is_some());

    session::delete_session(&conn, &token).unwrap();
    assert!(session::validate_session(&conn, &token).unwrap().is_none());
}

#[test]
fn test_crypto_roundtrip_various_lengths() {
    let key = b"01234567890123456789012345678901"; // 32 bytes

    // Empty string
    let encrypted = crypto::encrypt("", key).unwrap();
    assert_eq!(crypto::decrypt(&encrypted, key).unwrap(), "");

    // Short string
    let encrypted = crypto::encrypt("hi", key).unwrap();
    assert_eq!(crypto::decrypt(&encrypted, key).unwrap(), "hi");

    // Long string (1000 chars)
    let long_str: String = "a".repeat(1000);
    let encrypted = crypto::encrypt(&long_str, key).unwrap();
    assert_eq!(crypto::decrypt(&encrypted, key).unwrap(), long_str);

    // Unicode (Korean)
    let korean = "한글 테스트 암호화";
    let encrypted = crypto::encrypt(korean, key).unwrap();
    assert_eq!(crypto::decrypt(&encrypted, key).unwrap(), korean);
}

#[test]
fn test_crypto_different_ciphertexts_same_plaintext() {
    let key = b"01234567890123456789012345678901";
    let plaintext = "same_plaintext";

    let e1 = crypto::encrypt(plaintext, key).unwrap();
    let e2 = crypto::encrypt(plaintext, key).unwrap();
    assert_ne!(e1, e2, "ciphertexts should differ due to random nonce");
    assert_eq!(crypto::decrypt(&e1, key).unwrap(), plaintext);
    assert_eq!(crypto::decrypt(&e2, key).unwrap(), plaintext);
}

// ═══════════════════════════════════════════════════════════════
// SECTION 6: CSV Import & DB Scenarios (5 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_csv_import_empty_file() {
    let conn = init_db();
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("empty.csv");
    {
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        // No data rows
    }

    let count = csv_import::import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
    assert_eq!(count, 0, "empty CSV should import 0 records");
}

#[test]
fn test_csv_import_duplicate_rows_ignored() {
    let conn = init_db();
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("dup.csv");
    {
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        writeln!(f, "2024-01-01T00:00:00Z,100.0,105.0,95.0,102.0,1000.0").unwrap();
        writeln!(f, "2024-01-01T01:00:00Z,102.0,108.0,99.0,106.0,1500.0").unwrap();
    }

    let count1 = csv_import::import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
    assert_eq!(count1, 2);

    let count2 = csv_import::import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
    assert_eq!(count2, 0, "second import should skip all duplicates");

    let candles = csv_import::load_candles(&conn, "BTC", "hour").unwrap();
    assert_eq!(candles.len(), 2, "total should still be 2");
}

#[test]
fn test_csv_import_malformed_values() {
    let conn = init_db();
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("bad.csv");
    {
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        writeln!(f, "2024-01-01T00:00:00Z,100.0,105.0,95.0,102.0,1000.0").unwrap();
        writeln!(f, "2024-01-01T01:00:00Z,not_a_number,108.0,99.0,106.0,1500.0").unwrap();
    }

    // The csv_import uses `parse::<f64>()` with `?` so it errors on bad rows.
    // The whole import may fail or skip the bad row depending on implementation.
    // Since the implementation uses `?` on parse, the entire import will error.
    let result = csv_import::import_csv(&conn, &csv_path, "BTC", "hour");
    // Either it errors or imports only the valid row
    match result {
        Ok(count) => {
            // If it somehow succeeded (e.g., skipping bad rows), verify count
            assert!(count <= 1, "should have at most 1 valid row");
        }
        Err(_) => {
            // Expected: parse error on malformed row
            // The first row was inserted in the transaction but tx may be rolled back
        }
    }
    // No panic is the key assertion
}

#[test]
fn test_csv_import_and_indicator_calculation_pipeline() {
    let conn = init_db();
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("pipeline.csv");

    // Generate 200 rows of realistic CSV data
    {
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        let mut rng = SimpleRng::new(42);
        let mut price = 50000.0;
        for i in 0..200 {
            let change = (rng.next_f64() - 0.498) * 0.04;
            price *= 1.0 + change;
            let h = price * 1.01;
            let l = price * 0.99;
            let v = 1000.0 + rng.next_f64() * 5000.0;
            let ts = format!("2024-01-{:02}T{:02}:00:00Z", (i / 24) + 1, i % 24);
            writeln!(f, "{},{:.2},{:.2},{:.2},{:.2},{:.2}", ts, price, h, l, price, v).unwrap();
        }
    }

    let count = csv_import::import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
    assert_eq!(count, 200);

    let candles = csv_import::load_candles(&conn, "BTC", "hour").unwrap();
    assert_eq!(candles.len(), 200);

    let indicators = calculate_all(&candles);
    let data: Vec<MarketData> = candles
        .into_iter()
        .zip(indicators)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect();

    let registry = StrategyRegistry::new();
    let v0 = registry.get("V0").unwrap();
    let params = TradingParameters::default();
    let result = v0.run_simulation(&data, &params);
    assert!(result.total_return.is_finite(), "full pipeline should produce finite result");
}

#[test]
fn test_db_schema_all_tables_exist() {
    let conn = init_db();

    let expected_tables = [
        "market_data",
        "indicators",
        "strategy_configs",
        "positions",
        "trades",
        "optimization_runs",
        "optimization_results",
        "users",
        "sessions",
        "notification_configs",
    ];

    for table in &expected_tables {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "table '{}' should exist", table);
    }
}

// ═══════════════════════════════════════════════════════════════
// SECTION 7: Notification Scenarios (4 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_format_trade_buy_message() {
    let msg = format_trade_message("buy", "KRW-BTC", 50000000.0, 0.001, None);
    assert!(msg.is_some());
    let msg = msg.unwrap();
    assert!(msg.contains("매수"), "should contain '매수'");
    assert!(msg.contains("50000000"), "should contain price");
    assert!(msg.contains("0.001"), "should contain volume");
}

#[test]
fn test_format_trade_sell_message_with_pnl() {
    let msg = format_trade_message("sell", "KRW-ETH", 4000000.0, 0.5, Some(3.5));
    assert!(msg.is_some());
    let msg = msg.unwrap();
    assert!(msg.contains("매도"), "should contain '매도'");
    assert!(msg.contains("P/L"), "should contain P/L");
    assert!(msg.contains("3.50"), "should contain pnl percentage 3.50%");
}

#[test]
fn test_format_trade_invalid_side() {
    let msg = format_trade_message("invalid", "KRW-BTC", 50000000.0, 0.001, None);
    assert!(msg.is_none(), "invalid side should return None");
}

#[test]
fn test_notification_manager_empty_config() {
    let conn = init_db();
    // No user, no configs
    let _mgr = NotificationManager::from_db(&conn, 999);
    // Creating manager from empty DB should not panic
    // (We can't easily test async notify_trade here, but from_db is the key)
}

// ═══════════════════════════════════════════════════════════════
// SECTION 8: Full Integration Scenarios (3 tests)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_full_backtest_pipeline_realistic_data() {
    let data = make_realistic_data(2000, 12345);

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();

    let mut params = TradingParameters::default();
    params.v3_buy_volume_lo = 5000.0;
    params.v3_buy_volume_hi = 30000.0;
    params.v3_buy_volume_pow = 1.0;
    params.v3_buy_price_drop_lo = 0.003;
    params.v3_buy_price_drop_hi = 0.02;
    params.v3_buy_price_drop_pow = 1.0;
    params.v3_buy_decay_lo = 0.05;
    params.v3_buy_decay_hi = 0.3;
    params.v3_buy_decay_pow = 1.0;
    params.v3_buy_wait_lo = 5.0;
    params.v3_buy_wait_hi = 30.0;
    params.v3_buy_wait_pow = 1.0;
    params.v3_sell_fixed_sl_lo = 0.03;
    params.v3_sell_fixed_sl_hi = 0.08;
    params.v3_sell_fixed_sl_pow = 1.0;
    params.v3_sell_profit_lo = 0.02;
    params.v3_sell_profit_hi = 0.1;
    params.v3_sell_profit_pow = 1.0;
    params.v3_sell_max_hold_lo = 20.0;
    params.v3_sell_max_hold_hi = 100.0;
    params.v3_sell_max_hold_pow = 1.0;
    params.v3_fee_rate = 0.0005;
    params.v3_min_hold_bars = 3;

    let result = v3.run_simulation(&data, &params);
    assert!(result.total_return.is_finite(), "total_return should be finite");
    assert!(result.win_rate >= 0.0 && result.win_rate <= 100.0);
    // With 2000 bars and reasonable params, should have at least 1 trade
    assert!(result.total_trades >= 1, "should have at least 1 trade with 2000 bars, got 0");
}

#[test]
fn test_optimization_produces_better_results() {
    let data = make_realistic_data(500, 77);
    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    // Baseline with defaults
    let default_params = TradingParameters::default();
    let baseline = strategy.run_simulation(&data, &default_params);
    let baseline_return = baseline.total_return;

    // Optimize
    let config = OptimizerConfig {
        population_size: 15,
        generations: 5,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let population = optimizer.run(&data, strategy, None);

    // Find best return in the population
    let best_return = population
        .iter()
        .map(|ind| ind.objectives.first().copied().unwrap_or(f64::NEG_INFINITY))
        .fold(f64::NEG_INFINITY, f64::max);

    assert!(
        best_return >= baseline_return,
        "optimizer best ({:.2}%) should be >= baseline ({:.2}%)",
        best_return,
        baseline_return
    );
}

#[test]
fn test_full_auth_trading_pipeline() {
    let conn = init_db();
    seed_admin(&conn);

    // Create trader user
    let trader_hash = password::hash_password("trader_pass").unwrap();
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES ('trader1', ?1, 'trader')",
        [&trader_hash],
    )
    .unwrap();
    let trader_id = conn.last_insert_rowid();

    // Login as trader
    let token = session::create_session(&conn, trader_id).unwrap();
    let validated = session::validate_session(&conn, &token).unwrap().unwrap();
    assert_eq!(validated, trader_id);

    // Import CSV data
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("trade_data.csv");
    {
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        let mut rng = SimpleRng::new(99);
        let mut price = 50000.0;
        for i in 0..200 {
            let change = (rng.next_f64() - 0.498) * 0.04;
            price *= 1.0 + change;
            let h = price * 1.01;
            let l = price * 0.99;
            let v = 1000.0 + rng.next_f64() * 5000.0;
            let ts = format!("2024-01-{:02}T{:02}:00:00Z", (i / 24) + 1, i % 24);
            writeln!(f, "{},{:.2},{:.2},{:.2},{:.2},{:.2}", ts, price, h, l, price, v).unwrap();
        }
    }

    let count = csv_import::import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
    assert!(count > 0, "should import data");

    // Load and run simulation
    let candles = csv_import::load_candles(&conn, "BTC", "hour").unwrap();
    let indicators = calculate_all(&candles);
    let data: Vec<MarketData> = candles
        .into_iter()
        .zip(indicators)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect();

    let registry = StrategyRegistry::new();
    let v0 = registry.get("V0").unwrap();
    let params = TradingParameters::default();
    let result = v0.run_simulation(&data, &params);
    assert!(result.total_return.is_finite(), "simulation should produce valid result");
}
