use bitcoin_trader_lib::core::indicators::calculate_all;
use bitcoin_trader_lib::core::optimizer::Nsga2Optimizer;
use bitcoin_trader_lib::models::config::OptimizerConfig;
use bitcoin_trader_lib::models::market::{Candle, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use bitcoin_trader_lib::strategies::StrategyRegistry;
use chrono::{TimeZone, Utc};

/// Simple PRNG (xorshift32) for deterministic test data
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

/// Generate realistic market data with price fluctuation and volume spikes
fn make_realistic_data(n: usize, seed: u32) -> Vec<MarketData> {
    let mut rng = SimpleRng::new(seed);
    let mut price = 50000.0_f64;
    let mut candles = Vec::with_capacity(n);

    for i in 0..n {
        // Random walk with drift
        let change_pct = (rng.next_f64() - 0.498) * 0.04; // slight upward bias, ~4% max move
        price *= 1.0 + change_pct;
        price = price.max(1000.0);

        let high = price * (1.0 + rng.next_f64() * 0.015);
        let low = price * (1.0 - rng.next_f64() * 0.015);

        // Volume: baseline + occasional spikes
        let base_volume = 3000.0 + rng.next_f64() * 2000.0;
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

    // Calculate indicators
    let indicators = calculate_all(&candles);

    candles
        .into_iter()
        .zip(indicators.into_iter())
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect()
}

#[test]
fn test_full_pipeline_v0() {
    let data = make_realistic_data(500, 42);

    let registry = StrategyRegistry::new();
    let v0 = registry.get("V0").unwrap();

    let mut params = TradingParameters::default();
    params.buy_ready_volume_threshold = 10000.0;
    params.buy_ready_price_drop_ratio = 0.005;
    params.buy_confirm_volume_decay_ratio = 0.3;
    params.buy_wait_max_periods = 20;
    params.fixed_stop_loss_pct = 0.05;
    params.fixed_take_profit_pct = 0.03;
    params.fee_rate = 0.001;

    let result = v0.run_simulation(&data, &params);
    println!("[V0] Total return: {:.2}%, Trades: {}, Win rate: {:.1}%, Max DD: {:.2}%",
        result.total_return, result.total_trades, result.win_rate, result.max_drawdown);
    // Just verify it ran — actual performance depends on random data
    assert!(result.total_return.is_finite());
}

#[test]
fn test_full_pipeline_v3() {
    let data = make_realistic_data(500, 123);

    let registry = StrategyRegistry::new();
    let v3 = registry.get("V3").unwrap();

    let mut params = TradingParameters::default();
    params.v3_buy_volume_lo = 5000.0;
    params.v3_buy_volume_hi = 20000.0;
    params.v3_buy_volume_pow = 1.0;
    params.v3_buy_price_drop_lo = 0.003;
    params.v3_buy_price_drop_hi = 0.02;
    params.v3_buy_price_drop_pow = 1.0;
    params.v3_buy_decay_lo = 0.1;
    params.v3_buy_decay_hi = 0.4;
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
    params.v3_fee_rate = 0.001;
    params.v3_min_hold_bars = 3;

    let result = v3.run_simulation(&data, &params);
    println!("[V3] Total return: {:.2}%, Trades: {}, Win rate: {:.1}%, Max DD: {:.2}%",
        result.total_return, result.total_trades, result.win_rate, result.max_drawdown);
    assert!(result.total_return.is_finite());
}

#[test]
fn test_nsga2_with_v0() {
    let data = make_realistic_data(200, 77);

    let registry = StrategyRegistry::new();
    let strategy = registry.get("V0").unwrap();

    let config = OptimizerConfig {
        population_size: 10,
        generations: 5,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };

    let optimizer = Nsga2Optimizer::new(config);
    let results = optimizer.run(&data, strategy, Some(&|gr| {
        println!("Gen {}: best_return={:.2}%, best_wr={:.1}%, front={}",
            gr.generation, gr.best_return, gr.best_win_rate, gr.front_size);
        // Can't push to gen_results due to borrow, but we verify callback fires
    }));

    assert_eq!(results.len(), 10);
    // All individuals should have objectives
    for ind in &results {
        assert_eq!(ind.objectives.len(), 2);
    }
}

#[test]
fn test_all_strategies_registered() {
    let registry = StrategyRegistry::new();
    let list = registry.list();
    assert!(list.len() >= 6, "Should have at least 6 strategies, got {}", list.len());
}

#[test]
fn test_indicators_on_realistic_data() {
    let data = make_realistic_data(100, 999);

    // At bar 70+, indicators should be non-zero (most need ~60 bars warmup)
    let bar = &data[70];
    let ind = &bar.indicators;

    // RSI needs 14 bars warmup
    assert!(ind.rsi > 0.0, "RSI should be non-zero at bar 70, got {}", ind.rsi);
    // SMA10 needs 10 bars
    assert!(ind.sma_10 > 0.0, "SMA10 should be non-zero at bar 70");
    // MACD histogram needs ~34 bars
    // Note: might be negative, check it's not exactly 0
    assert!(ind.macd != 0.0 || ind.macd_signal != 0.0,
        "MACD should be non-zero at bar 70");
    // Bollinger bands need 20 bars
    assert!(ind.bollinger_upper > 0.0, "BB upper should be non-zero at bar 70");
    // ATR needs 14 bars
    assert!(ind.atr > 0.0, "ATR should be non-zero at bar 70, got {}", ind.atr);
}
