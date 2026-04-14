use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingParameters {
    // Buy parameters
    pub urgent_buy_volume_threshold: f64,
    pub buy_ready_volume_threshold: f64,
    pub buy_confirm_volume_decay_ratio: f64,
    pub buy_wait_max_periods: i32,
    pub buy_confirm_psy_threshold: f64,
    pub urgent_buy_price_drop_ratio: f64,
    pub buy_ready_price_drop_ratio: f64,

    // Sell parameters
    pub urgent_sell_volume_threshold: f64,
    pub sell_ready_volume_threshold: f64,
    pub sell_confirm_volume_decay_ratio: f64,
    pub sell_wait_max_periods: i32,
    pub urgent_sell_profit_ratio: f64,
    pub sell_ready_price_rise_ratio: f64,

    // Risk parameters
    pub trailing_stop_pct: f64,
    pub max_hold_periods: i32,
    pub fee_rate: f64,
    pub fixed_stop_loss_pct: f64,
    pub fixed_take_profit_pct: f64,

    // V1 parameters
    pub v1_adaptive_volume_window: i32,
    pub v1_atr_trailing_multiplier: f64,

    // V2 parameters
    pub v2_rsi_weight: f64,
    pub v2_macd_weight: f64,
    pub v2_bb_weight: f64,
    pub v2_buy_score_threshold: f64,
    pub v2_sell_score_threshold: f64,

    // V3 RSI interpolation - Buy
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

    // V3 RSI interpolation - Sell
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

    // V3 misc
    pub v3_fee_rate: f64,
    pub v3_min_hold_bars: i32,
    pub v3_volume_lookback: i32,
}

impl Default for TradingParameters {
    fn default() -> Self {
        Self {
            urgent_buy_volume_threshold: 30000.0,
            buy_ready_volume_threshold: 9000.0,
            buy_confirm_volume_decay_ratio: 0.02,
            buy_wait_max_periods: 240,
            buy_confirm_psy_threshold: 0.0,
            urgent_buy_price_drop_ratio: 1.0,
            buy_ready_price_drop_ratio: 1.0,

            urgent_sell_volume_threshold: 30000.0,
            sell_ready_volume_threshold: 9000.0,
            sell_confirm_volume_decay_ratio: 0.02,
            sell_wait_max_periods: 168,
            urgent_sell_profit_ratio: 1.0,
            sell_ready_price_rise_ratio: 1.0,

            trailing_stop_pct: 0.0,
            max_hold_periods: 0,
            fee_rate: 0.0,
            fixed_stop_loss_pct: 0.0,
            fixed_take_profit_pct: 0.0,

            v1_adaptive_volume_window: 20,
            v1_atr_trailing_multiplier: 2.0,

            v2_rsi_weight: 1.0,
            v2_macd_weight: 1.0,
            v2_bb_weight: 1.0,
            v2_buy_score_threshold: 0.6,
            v2_sell_score_threshold: 0.6,

            v3_buy_volume_lo: 0.0,
            v3_buy_volume_hi: 0.0,
            v3_buy_volume_pow: 0.0,
            v3_buy_price_drop_lo: 0.0,
            v3_buy_price_drop_hi: 0.0,
            v3_buy_price_drop_pow: 0.0,
            v3_buy_decay_lo: 0.0,
            v3_buy_decay_hi: 0.0,
            v3_buy_decay_pow: 0.0,
            v3_buy_psy_lo: 0.0,
            v3_buy_psy_hi: 0.0,
            v3_buy_psy_pow: 0.0,
            v3_buy_wait_lo: 0.0,
            v3_buy_wait_hi: 0.0,
            v3_buy_wait_pow: 0.0,

            v3_sell_stop_loss_lo: 0.0,
            v3_sell_stop_loss_hi: 0.0,
            v3_sell_stop_loss_pow: 0.0,
            v3_sell_profit_lo: 0.0,
            v3_sell_profit_hi: 0.0,
            v3_sell_profit_pow: 0.0,
            v3_sell_volume_lo: 0.0,
            v3_sell_volume_hi: 0.0,
            v3_sell_volume_pow: 0.0,
            v3_sell_decay_lo: 0.0,
            v3_sell_decay_hi: 0.0,
            v3_sell_decay_pow: 0.0,
            v3_sell_fixed_sl_lo: 0.0,
            v3_sell_fixed_sl_hi: 0.0,
            v3_sell_fixed_sl_pow: 0.0,
            v3_sell_max_hold_lo: 0.0,
            v3_sell_max_hold_hi: 0.0,
            v3_sell_max_hold_pow: 0.0,

            v3_fee_rate: 0.0005,
            v3_min_hold_bars: 6,
            v3_volume_lookback: 20,
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
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            annual_return: 0.0,
        }
    }
}
