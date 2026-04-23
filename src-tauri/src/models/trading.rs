use serde::{Deserialize, Serialize};

/// Trading parameters for the V3 RegimeAdaptive strategy — the only live
/// strategy in this project. Fields mirror the legacy
/// `TradingConfig_V3_RegimeAdaptive_20260401_140149.json` verbatim so
/// simulation results reproduce the baseline (~278% on clean day_psy pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingParameters {
    // RSI-interpolated buy parameters
    pub v3_urgent_buy_volume_lo: f64,
    pub v3_urgent_buy_volume_hi: f64,
    pub v3_urgent_buy_volume_pow: f64,
    pub v3_buy_volume_lo: f64,
    pub v3_buy_volume_hi: f64,
    pub v3_buy_volume_pow: f64,
    pub v3_buy_price_drop_lo: f64,
    pub v3_buy_price_drop_hi: f64,
    pub v3_buy_price_drop_pow: f64,
    pub v3_buy_decay_lo: f64,
    pub v3_buy_decay_hi: f64,
    pub v3_buy_decay_pow: f64,
    pub v3_buy_psy_lo: f64,
    pub v3_buy_psy_hi: f64,
    pub v3_buy_psy_pow: f64,
    pub v3_buy_wait_lo: f64,
    pub v3_buy_wait_hi: f64,
    pub v3_buy_wait_pow: f64,

    // RSI-interpolated sell parameters
    pub v3_sell_stop_loss_lo: f64,
    pub v3_sell_stop_loss_hi: f64,
    pub v3_sell_stop_loss_pow: f64,
    pub v3_sell_profit_lo: f64,
    pub v3_sell_profit_hi: f64,
    pub v3_sell_profit_pow: f64,
    pub v3_sell_volume_lo: f64,
    pub v3_sell_volume_hi: f64,
    pub v3_sell_volume_pow: f64,
    pub v3_sell_decay_lo: f64,
    pub v3_sell_decay_hi: f64,
    pub v3_sell_decay_pow: f64,
    pub v3_sell_fixed_sl_lo: f64,
    pub v3_sell_fixed_sl_hi: f64,
    pub v3_sell_fixed_sl_pow: f64,
    pub v3_sell_max_hold_lo: f64,
    pub v3_sell_max_hold_hi: f64,
    pub v3_sell_max_hold_pow: f64,

    // Misc
    pub v3_fee_rate: f64,
    pub v3_min_hold_bars: i32,
    pub v3_volume_lookback: i32,

    // V5 dual-PSY thresholds (EnhancedAdaptive overlays V3 with PsyHour+PsyDay
    // dual confirmation). V5 does NOT use v3_buy_psy_* — these 12 fields
    // replace that single-PSY filter. Defaults mirror legacy
    // NetTradingEngine.cs:254-279.
    #[serde(default = "default_v5_buy_psy_hour_lo")] pub v5_buy_psy_hour_lo: f64,
    #[serde(default = "default_v5_buy_psy_hour_hi")] pub v5_buy_psy_hour_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_buy_psy_hour_pow: f64,
    #[serde(default = "default_v5_buy_psy_day_lo")] pub v5_buy_psy_day_lo: f64,
    #[serde(default = "default_v5_buy_psy_day_hi")] pub v5_buy_psy_day_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_buy_psy_day_pow: f64,
    #[serde(default = "default_v5_sell_psy_hour_lo")] pub v5_sell_psy_hour_lo: f64,
    #[serde(default = "default_v5_sell_psy_hour_hi")] pub v5_sell_psy_hour_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_sell_psy_hour_pow: f64,
    #[serde(default = "default_v5_sell_psy_day_lo")] pub v5_sell_psy_day_lo: f64,
    #[serde(default = "default_v5_sell_psy_day_hi")] pub v5_sell_psy_day_hi: f64,
    #[serde(default = "default_v5_pow")] pub v5_sell_psy_day_pow: f64,
}

fn default_v5_buy_psy_hour_lo() -> f64 { 0.05 }
fn default_v5_buy_psy_hour_hi() -> f64 { -0.15 }
fn default_v5_buy_psy_day_lo() -> f64 { 0.15 }
fn default_v5_buy_psy_day_hi() -> f64 { -0.20 }
fn default_v5_sell_psy_hour_lo() -> f64 { -0.05 }
fn default_v5_sell_psy_hour_hi() -> f64 { 0.15 }
fn default_v5_sell_psy_day_lo() -> f64 { -0.10 }
fn default_v5_sell_psy_day_hi() -> f64 { 0.20 }
fn default_v5_pow() -> f64 { 1.0 }

impl TradingParameters {
    /// Market-aware defaults. Volume thresholds are calibrated against ETH/hour
    /// (legacy base). For BTC we scale by the ETH→BTC average-volume ratio
    /// (≈1/13) so the defaults stay in a reasonable operating range even
    /// though re-optimization for BTC is expected.
    pub fn default_for_market(market: &str) -> Self {
        let mut p = Self::default();
        const ETH_TO_BTC: f64 = 1.0 / 13.0;
        if market == "BTC" {
            p.v3_urgent_buy_volume_lo *= ETH_TO_BTC;
            p.v3_urgent_buy_volume_hi *= ETH_TO_BTC;
            p.v3_buy_volume_lo *= ETH_TO_BTC;
            p.v3_buy_volume_hi *= ETH_TO_BTC;
            p.v3_sell_volume_lo *= ETH_TO_BTC;
            p.v3_sell_volume_hi *= ETH_TO_BTC;
        }
        p
    }
}

impl Default for TradingParameters {
    fn default() -> Self {
        // Verbatim from legacy TradingConfig_V3_RegimeAdaptive_20260401_140149.json.
        Self {
            v3_urgent_buy_volume_lo: 21000.0,
            v3_urgent_buy_volume_hi: 75000.0,
            v3_urgent_buy_volume_pow: 1.3,
            v3_buy_volume_lo: 5000.0,
            v3_buy_volume_hi: 17500.0,
            v3_buy_volume_pow: 3.5,
            v3_buy_price_drop_lo: 1.045,
            v3_buy_price_drop_hi: 1.025,
            v3_buy_price_drop_pow: 2.7,
            v3_buy_decay_lo: 0.09,
            v3_buy_decay_hi: 0.077,
            v3_buy_decay_pow: 2.6,
            v3_buy_psy_lo: 0.14,
            v3_buy_psy_hi: -0.24,
            v3_buy_psy_pow: 2.4,
            v3_buy_wait_lo: 492.0,
            v3_buy_wait_hi: 336.0,
            v3_buy_wait_pow: 3.4,

            v3_sell_stop_loss_lo: 0.85,
            v3_sell_stop_loss_hi: 0.82,
            v3_sell_stop_loss_pow: 2.0,
            v3_sell_profit_lo: 1.145,
            v3_sell_profit_hi: 1.09,
            v3_sell_profit_pow: 2.0,
            v3_sell_volume_lo: 2000.0,
            v3_sell_volume_hi: 31500.0,
            v3_sell_volume_pow: 3.2,
            v3_sell_decay_lo: 0.116,
            v3_sell_decay_hi: 0.079,
            v3_sell_decay_pow: 3.3,
            v3_sell_fixed_sl_lo: 0.08,
            v3_sell_fixed_sl_hi: 0.08,
            v3_sell_fixed_sl_pow: 2.6,
            v3_sell_max_hold_lo: 672.0,
            v3_sell_max_hold_hi: 816.0,
            v3_sell_max_hold_pow: 1.9,

            v3_fee_rate: 0.0005,
            v3_min_hold_bars: 21,
            v3_volume_lookback: 35,

            v5_buy_psy_hour_lo: default_v5_buy_psy_hour_lo(),
            v5_buy_psy_hour_hi: default_v5_buy_psy_hour_hi(),
            v5_buy_psy_hour_pow: default_v5_pow(),
            v5_buy_psy_day_lo: default_v5_buy_psy_day_lo(),
            v5_buy_psy_day_hi: default_v5_buy_psy_day_hi(),
            v5_buy_psy_day_pow: default_v5_pow(),
            v5_sell_psy_hour_lo: default_v5_sell_psy_hour_lo(),
            v5_sell_psy_hour_hi: default_v5_sell_psy_hour_hi(),
            v5_sell_psy_hour_pow: default_v5_pow(),
            v5_sell_psy_day_lo: default_v5_sell_psy_day_lo(),
            v5_sell_psy_day_hi: default_v5_sell_psy_day_hi(),
            v5_sell_psy_day_pow: default_v5_pow(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub buy_index: usize,
    pub sell_index: usize,
    pub buy_price: f64,
    pub sell_price: f64,
    pub pnl_pct: f64,
    pub hold_bars: i32,
    pub buy_signal: String,
    pub sell_signal: String,
    #[serde(default)]
    pub buy_timestamp: String,
    #[serde(default)]
    pub sell_timestamp: String,
}

/// Per-bar signal state transition — ported from legacy `DetermineSignalType`.
/// Emitted whenever the signal kind changes, so the sequence is compact yet
/// complete for the timeline UI (ready → buy ready → buy → hold → sell ready → sell).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub index: usize,
    pub timestamp: String,
    pub signal_type: String,
    pub price: f64,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub total_return: f64,
    pub market_return: f64,
    pub max_drawdown: f64,
    pub total_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_trade_return: f64,
    pub max_consecutive_losses: usize,
    pub fee_adjusted_return: f64,
    pub buy_signals: usize,
    pub sell_signals: usize,
    pub last_position: i32,
    pub last_buy_price: f64,
    pub last_set_volume: f64,
    pub last_signal_type: String,
    pub last_hold_bars: i32,
    pub last_entry_rsi: f64,
    pub last_highest_since_buy: f64,
    pub trades: Vec<TradeRecord>,
    #[serde(default)]
    pub signal_log: Vec<SignalEvent>,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    pub annual_return: f64,
}

impl Default for SimulationResult {
    fn default() -> Self {
        Self {
            total_return: 0.0,
            market_return: 0.0,
            max_drawdown: 0.0,
            total_trades: 0,
            win_rate: 0.0,
            profit_factor: 0.0,
            avg_trade_return: 0.0,
            max_consecutive_losses: 0,
            fee_adjusted_return: 0.0,
            buy_signals: 0,
            sell_signals: 0,
            last_position: 0,
            last_buy_price: 0.0,
            last_set_volume: 0.0,
            last_signal_type: String::new(),
            last_hold_bars: 0,
            last_entry_rsi: 0.0,
            last_highest_since_buy: 0.0,
            trades: Vec::new(),
            signal_log: Vec::new(),
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            annual_return: 0.0,
        }
    }
}
