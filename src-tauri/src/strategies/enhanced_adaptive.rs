use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradeRecord, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

/// V5 EnhancedAdaptive: PSY-hour dual confirmation + ATR trailing stop + volume decay.
pub struct EnhancedAdaptiveStrategy;

impl Strategy for EnhancedAdaptiveStrategy {
    fn name(&self) -> &str {
        "Enhanced Adaptive (V5)"
    }

    fn description(&self) -> &str {
        "PSY-hour dual confirmation with ATR-based trailing stop and volume decay"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        let mut result = SimulationResult::default();
        if data.is_empty() {
            return result;
        }

        let fee_rate = params.v5_fee_rate;
        let psy_buy = params.v5_psy_buy_threshold;
        let psy_sell = params.v5_psy_sell_threshold;
        let atr_mult = params.v5_atr_multiplier;
        let vol_threshold = params.v5_volume_threshold;
        let decay_ratio = params.v5_decay_ratio;
        let min_hold = params.v5_min_hold_bars;
        let stop_loss = params.v5_stop_loss;
        let take_profit = params.v5_take_profit;

        let mut position: i32 = 0;
        let mut buy_price: f64 = 0.0;
        let mut hold_bars: i32 = 0;
        let mut highest_since_buy: f64 = 0.0;
        let mut entry_rsi: f64 = 0.0;
        let mut set_volume: f64 = 0.0;
        let mut buy_sign: i32 = 0; // 0=none, 1=volume surge detected, 2=decay confirmed

        let mut cumulative_return: f64 = 1.0;
        let mut peak: f64 = 1.0;
        let mut max_drawdown: f64 = 0.0;
        let mut wins: usize = 0;
        let mut sum_wins: f64 = 0.0;
        let mut sum_losses: f64 = 0.0;
        let mut consecutive_losses: usize = 0;
        let mut max_consecutive_losses: usize = 0;
        let mut trade_returns: Vec<f64> = Vec::new();
        let mut buy_index: usize = 0;
        let mut buy_wait_bars: i32 = 0;

        for i in 1..data.len() {
            let close = data[i].candle.close;
            let volume = data[i].candle.volume;
            let psy_hour = data[i].indicators.psy_hour;
            let atr = data[i].indicators.atr;

            if position == 0 {
                // Buy logic: PSY hour < buy threshold (pessimism) + volume surge + decay
                match buy_sign {
                    0 => {
                        // Check: PSY low (market pessimism) + volume surge
                        if psy_hour < psy_buy && psy_hour > 0.0
                            && vol_threshold > 0.0
                            && volume >= vol_threshold
                        {
                            buy_sign = 1;
                            set_volume = volume;
                            buy_wait_bars = 0;
                            result.buy_signals += 1;
                        }
                    }
                    1 => {
                        buy_wait_bars += 1;
                        // Wait for volume decay confirmation
                        if decay_ratio > 0.0 && volume <= set_volume * decay_ratio {
                            buy_sign = 2;
                        } else if buy_wait_bars > 20 {
                            buy_sign = 0; // Timeout
                        }
                    }
                    _ => {}
                }

                if buy_sign == 2 {
                    buy_price = close + close * fee_rate;
                    position = 1;
                    hold_bars = 0;
                    highest_since_buy = close;
                    entry_rsi = data[i].indicators.rsi;
                    buy_index = i;
                    buy_sign = 0;
                }
            } else {
                hold_bars += 1;
                if close > highest_since_buy {
                    highest_since_buy = close;
                }

                if hold_bars < min_hold {
                    continue;
                }

                let mut should_sell = false;
                let mut sell_signal_str = String::new();

                // 1. Fixed stop loss
                let pnl = (close - buy_price) / buy_price;
                if stop_loss > 0.0 && pnl <= -stop_loss {
                    should_sell = true;
                    sell_signal_str = "v5_stop_loss".into();
                }

                // 2. Fixed take profit
                if !should_sell && take_profit > 0.0 && pnl >= take_profit {
                    should_sell = true;
                    sell_signal_str = "v5_take_profit".into();
                }

                // 3. ATR-based trailing stop
                if !should_sell && atr > 0.0 && atr_mult > 0.0 {
                    let trail_stop = highest_since_buy - atr * atr_mult;
                    if close <= trail_stop {
                        should_sell = true;
                        sell_signal_str = "v5_atr_trailing".into();
                    }
                }

                // 4. PSY hour > sell threshold (market optimism → sell into strength)
                if !should_sell && psy_sell > 0.0 && psy_hour > psy_sell {
                    should_sell = true;
                    sell_signal_str = "v5_psy_sell".into();
                    result.sell_signals += 1;
                }

                if should_sell {
                    let sell_price = close - close * fee_rate;
                    let pnl_pct = (sell_price - buy_price) / buy_price;

                    trade_returns.push(pnl_pct);
                    cumulative_return *= 1.0 + pnl_pct;
                    if cumulative_return > peak {
                        peak = cumulative_return;
                    }
                    let drawdown = (peak - cumulative_return) / peak;
                    if drawdown > max_drawdown {
                        max_drawdown = drawdown;
                    }

                    if pnl_pct > 0.0 {
                        wins += 1;
                        sum_wins += pnl_pct;
                        consecutive_losses = 0;
                    } else {
                        sum_losses += pnl_pct.abs();
                        consecutive_losses += 1;
                        if consecutive_losses > max_consecutive_losses {
                            max_consecutive_losses = consecutive_losses;
                        }
                    }

                    result.trades.push(TradeRecord {
                        buy_index,
                        sell_index: i,
                        buy_price,
                        sell_price,
                        pnl_pct,
                        hold_bars,
                        buy_signal: "v5_psy_decay_buy".into(),
                        sell_signal: sell_signal_str,
                    });

                    position = 0;
                    buy_price = 0.0;
                    hold_bars = 0;
                    highest_since_buy = 0.0;
                    buy_sign = 0;
                }
            }
        }

        // Finalize metrics using shared helper
        super::ml_strategy::finalize_result(
            &mut result, cumulative_return, peak, max_drawdown,
            max_consecutive_losses, wins, sum_wins, sum_losses,
            &trade_returns, data, position, buy_price, hold_bars,
            entry_rsi, highest_since_buy, set_volume,
        );

        result
    }

    fn get_latest_signal(
        &self,
        data: &[MarketData],
        params: &TradingParameters,
        _position: &PositionState,
    ) -> TradingSignal {
        let result = self.run_simulation(data, params);
        let signal_type = if result.last_position == 1 {
            SignalType::Hold
        } else {
            SignalType::Hold
        };
        TradingSignal {
            signal_type,
            confidence: None,
            metadata: HashMap::new(),
        }
    }

    fn parameter_ranges(&self) -> Vec<ParameterRange> {
        vec![
            ParameterRange { name: "v5_psy_buy_threshold".into(), min: 20.0, max: 50.0, step: 2.0 },
            ParameterRange { name: "v5_psy_sell_threshold".into(), min: 55.0, max: 85.0, step: 2.0 },
            ParameterRange { name: "v5_atr_multiplier".into(), min: 1.0, max: 5.0, step: 0.5 },
            ParameterRange { name: "v5_volume_threshold".into(), min: 1000.0, max: 50000.0, step: 1000.0 },
            ParameterRange { name: "v5_decay_ratio".into(), min: 0.05, max: 0.5, step: 0.05 },
            ParameterRange { name: "v5_min_hold_bars".into(), min: 1.0, max: 24.0, step: 1.0 },
            ParameterRange { name: "v5_stop_loss".into(), min: 0.02, max: 0.15, step: 0.01 },
            ParameterRange { name: "v5_take_profit".into(), min: 0.02, max: 0.20, step: 0.01 },
            ParameterRange { name: "v5_fee_rate".into(), min: 0.0, max: 0.005, step: 0.0001 },
            ParameterRange { name: "v5_trailing_atr_bars".into(), min: 7.0, max: 28.0, step: 1.0 },
        ]
    }
}
