use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradeRecord, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

/// V3 RegimeAdaptive: RSI-based parameter interpolation for dynamic thresholds.
pub struct RegimeAdaptiveStrategy;

/// Interpolate: P = Lo + (Hi - Lo) * clamp((RSI - 20) / 60, 0, 1)^Pow
fn interpolate(lo: f64, hi: f64, pow: f64, rsi: f64) -> f64 {
    let t = ((rsi - 20.0) / 60.0).clamp(0.0, 1.0);
    let t_pow = if pow > 0.0 { t.powf(pow) } else { t };
    lo + (hi - lo) * t_pow
}

impl Strategy for RegimeAdaptiveStrategy {
    fn name(&self) -> &str {
        "Regime Adaptive (V3)"
    }

    fn description(&self) -> &str {
        "RSI-based dynamic parameter interpolation with volume decay state machine"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        let mut result = SimulationResult::default();
        if data.is_empty() {
            return result;
        }

        let mut position: i32 = 0;
        let mut buy_price: f64 = 0.0;
        let mut set_volume: f64 = 0.0;
        let mut buy_sign: i32 = 0;
        let mut sell_sign: i32 = 0;
        let mut buy_wait_bars: i32 = 0;
        let mut sell_wait_bars: i32 = 0;
        let mut hold_bars: i32 = 0;
        let mut highest_since_buy: f64 = 0.0;
        let mut entry_rsi: f64 = 0.0;

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
        let mut buy_signal_str = String::new();

        let fee_rate = params.v3_fee_rate;

        for i in 1..data.len() {
            let close = data[i].candle.close;
            let volume = data[i].candle.volume;
            let prev_close = data[i - 1].candle.close;
            let price_change = if prev_close != 0.0 {
                (close - prev_close) / prev_close
            } else {
                0.0
            };
            let rsi = data[i].indicators.rsi;

            // Dynamic buy params
            let dyn_buy_volume = interpolate(params.v3_buy_volume_lo, params.v3_buy_volume_hi, params.v3_buy_volume_pow, rsi);
            let dyn_buy_price_drop = interpolate(params.v3_buy_price_drop_lo, params.v3_buy_price_drop_hi, params.v3_buy_price_drop_pow, rsi);
            let dyn_buy_decay = interpolate(params.v3_buy_decay_lo, params.v3_buy_decay_hi, params.v3_buy_decay_pow, rsi);
            let dyn_buy_psy = interpolate(params.v3_buy_psy_lo, params.v3_buy_psy_hi, params.v3_buy_psy_pow, rsi);
            let dyn_buy_wait = interpolate(params.v3_buy_wait_lo, params.v3_buy_wait_hi, params.v3_buy_wait_pow, rsi);

            if position == 0 {
                match buy_sign {
                    0 => {
                        // Check buy ready (volume threshold + price drop)
                        if dyn_buy_volume > 0.0
                            && volume >= dyn_buy_volume
                            && price_change <= -dyn_buy_price_drop
                        {
                            // Check PSY threshold
                            if data[i].indicators.psy_hour <= dyn_buy_psy || dyn_buy_psy <= 0.0 {
                                buy_sign = 1;
                                set_volume = volume;
                                buy_wait_bars = 0;
                                result.buy_signals += 1;
                            }
                        }
                    }
                    1 => {
                        buy_wait_bars += 1;
                        if dyn_buy_decay > 0.0 && volume <= set_volume * dyn_buy_decay {
                            buy_sign = 2;
                            buy_signal_str = "regime_decay_buy".to_string();
                        } else if buy_wait_bars > dyn_buy_wait as i32 {
                            buy_sign = 0;
                        }
                    }
                    _ => {}
                }

                if buy_sign == 2 {
                    buy_price = close + close * fee_rate;
                    position = 1;
                    hold_bars = 0;
                    highest_since_buy = close;
                    buy_sign = 0;
                    sell_sign = 0;
                    sell_wait_bars = 0;
                    buy_index = i;
                    entry_rsi = rsi;
                }
            } else {
                hold_bars += 1;
                if close > highest_since_buy {
                    highest_since_buy = close;
                }

                // Check min hold bars
                if hold_bars < params.v3_min_hold_bars {
                    continue;
                }

                let mut should_sell = false;
                let mut sell_signal_str = String::new();

                // Dynamic sell params (interpolated by current RSI)
                let dyn_stop_loss = interpolate(params.v3_sell_stop_loss_lo, params.v3_sell_stop_loss_hi, params.v3_sell_stop_loss_pow, rsi);
                let dyn_profit = interpolate(params.v3_sell_profit_lo, params.v3_sell_profit_hi, params.v3_sell_profit_pow, rsi);
                let dyn_sell_volume = interpolate(params.v3_sell_volume_lo, params.v3_sell_volume_hi, params.v3_sell_volume_pow, rsi);
                let dyn_sell_decay = interpolate(params.v3_sell_decay_lo, params.v3_sell_decay_hi, params.v3_sell_decay_pow, rsi);
                let dyn_fixed_sl = interpolate(params.v3_sell_fixed_sl_lo, params.v3_sell_fixed_sl_hi, params.v3_sell_fixed_sl_pow, rsi);
                let dyn_max_hold = interpolate(params.v3_sell_max_hold_lo, params.v3_sell_max_hold_hi, params.v3_sell_max_hold_pow, rsi);

                // Fixed stop loss
                if dyn_fixed_sl > 0.0 {
                    let loss_pct = (close - buy_price) / buy_price;
                    if loss_pct <= -dyn_fixed_sl {
                        should_sell = true;
                        sell_signal_str = "regime_fixed_sl".to_string();
                    }
                }

                // Dynamic profit target
                if !should_sell && dyn_profit > 0.0 {
                    let gain_pct = (close - buy_price) / buy_price;
                    if gain_pct >= dyn_profit {
                        should_sell = true;
                        sell_signal_str = "regime_profit".to_string();
                    }
                }

                // Dynamic stop loss (trailing-like from entry)
                if !should_sell && dyn_stop_loss > 0.0 {
                    let drop_from_peak = (highest_since_buy - close) / highest_since_buy;
                    if drop_from_peak >= dyn_stop_loss {
                        should_sell = true;
                        sell_signal_str = "regime_trailing_sl".to_string();
                    }
                }

                // Max hold
                if !should_sell && dyn_max_hold > 0.0 && hold_bars >= dyn_max_hold as i32 {
                    should_sell = true;
                    sell_signal_str = "regime_max_hold".to_string();
                }

                // Volume-based sell with decay
                if !should_sell {
                    match sell_sign {
                        0 => {
                            if dyn_sell_volume > 0.0 && volume >= dyn_sell_volume {
                                sell_sign = 1;
                                set_volume = volume;
                                sell_wait_bars = 0;
                                result.sell_signals += 1;
                            }
                        }
                        1 => {
                            sell_wait_bars += 1;
                            if dyn_sell_decay > 0.0 && volume <= set_volume * dyn_sell_decay {
                                should_sell = true;
                                sell_signal_str = "regime_decay_sell".to_string();
                            } else if sell_wait_bars > 20 {
                                sell_sign = 0;
                            }
                        }
                        _ => {}
                    }
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
                        buy_signal: buy_signal_str.clone(),
                        sell_signal: sell_signal_str,
                    });

                    position = 0;
                    buy_price = 0.0;
                    buy_sign = 0;
                    sell_sign = 0;
                    hold_bars = 0;
                    highest_since_buy = 0.0;
                }
            }
        }

        // Finalize metrics
        let total_trades = result.trades.len();
        result.total_trades = total_trades;
        result.total_return = (cumulative_return - 1.0) * 100.0;
        result.fee_adjusted_return = result.total_return;
        result.max_drawdown = max_drawdown * 100.0;
        result.max_consecutive_losses = max_consecutive_losses;

        if total_trades > 0 {
            result.win_rate = (wins as f64 / total_trades as f64) * 100.0;
            result.avg_trade_return = trade_returns.iter().sum::<f64>() / total_trades as f64 * 100.0;
        }

        result.profit_factor = if sum_losses > 0.0 {
            sum_wins / sum_losses
        } else if sum_wins > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        let first_close = data[0].candle.close;
        let last_close = data[data.len() - 1].candle.close;
        if first_close > 0.0 {
            result.market_return = ((last_close - first_close) / first_close) * 100.0;
        }

        if trade_returns.len() >= 2 {
            let mean = trade_returns.iter().sum::<f64>() / trade_returns.len() as f64;
            let variance = trade_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
                / (trade_returns.len() - 1) as f64;
            let std_dev = variance.sqrt();
            if std_dev > 0.0 {
                result.sharpe_ratio = mean / std_dev;
            }
            let downside_variance = trade_returns.iter().filter(|&&r| r < 0.0).map(|r| r.powi(2)).sum::<f64>()
                / trade_returns.len() as f64;
            let downside_dev = downside_variance.sqrt();
            if downside_dev > 0.0 {
                result.sortino_ratio = mean / downside_dev;
            }
        }

        if max_drawdown > 0.0 {
            result.calmar_ratio = (cumulative_return - 1.0) / max_drawdown;
        }

        let n_bars = data.len() as f64;
        if n_bars > 0.0 && cumulative_return > 0.0 {
            result.annual_return = (cumulative_return.powf(8760.0 / n_bars) - 1.0) * 100.0;
        }

        result.last_position = position;
        result.last_buy_price = buy_price;
        result.last_set_volume = set_volume;
        result.last_hold_bars = hold_bars;
        result.last_entry_rsi = entry_rsi;
        result.last_highest_since_buy = highest_since_buy;
        result.last_signal_type = if position == 1 { "holding".into() } else { "idle".into() };

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
            // Buy interpolation
            ParameterRange { name: "v3_buy_volume_lo".into(), min: 1000.0, max: 50000.0, step: 1000.0 },
            ParameterRange { name: "v3_buy_volume_hi".into(), min: 5000.0, max: 100000.0, step: 1000.0 },
            ParameterRange { name: "v3_buy_volume_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_buy_price_drop_lo".into(), min: 0.001, max: 0.05, step: 0.001 },
            ParameterRange { name: "v3_buy_price_drop_hi".into(), min: 0.01, max: 0.1, step: 0.005 },
            ParameterRange { name: "v3_buy_price_drop_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_buy_decay_lo".into(), min: 0.001, max: 0.3, step: 0.01 },
            ParameterRange { name: "v3_buy_decay_hi".into(), min: 0.01, max: 0.5, step: 0.01 },
            ParameterRange { name: "v3_buy_decay_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_buy_psy_lo".into(), min: 0.0, max: 50.0, step: 5.0 },
            ParameterRange { name: "v3_buy_psy_hi".into(), min: 20.0, max: 100.0, step: 5.0 },
            ParameterRange { name: "v3_buy_psy_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_buy_wait_lo".into(), min: 5.0, max: 100.0, step: 5.0 },
            ParameterRange { name: "v3_buy_wait_hi".into(), min: 50.0, max: 500.0, step: 10.0 },
            ParameterRange { name: "v3_buy_wait_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            // Sell interpolation
            ParameterRange { name: "v3_sell_stop_loss_lo".into(), min: 0.01, max: 0.1, step: 0.01 },
            ParameterRange { name: "v3_sell_stop_loss_hi".into(), min: 0.05, max: 0.3, step: 0.01 },
            ParameterRange { name: "v3_sell_stop_loss_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_sell_profit_lo".into(), min: 0.01, max: 0.1, step: 0.01 },
            ParameterRange { name: "v3_sell_profit_hi".into(), min: 0.05, max: 0.5, step: 0.01 },
            ParameterRange { name: "v3_sell_profit_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_sell_volume_lo".into(), min: 1000.0, max: 50000.0, step: 1000.0 },
            ParameterRange { name: "v3_sell_volume_hi".into(), min: 5000.0, max: 100000.0, step: 1000.0 },
            ParameterRange { name: "v3_sell_volume_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_sell_decay_lo".into(), min: 0.001, max: 0.3, step: 0.01 },
            ParameterRange { name: "v3_sell_decay_hi".into(), min: 0.01, max: 0.5, step: 0.01 },
            ParameterRange { name: "v3_sell_decay_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_sell_fixed_sl_lo".into(), min: 0.01, max: 0.1, step: 0.01 },
            ParameterRange { name: "v3_sell_fixed_sl_hi".into(), min: 0.05, max: 0.3, step: 0.01 },
            ParameterRange { name: "v3_sell_fixed_sl_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            ParameterRange { name: "v3_sell_max_hold_lo".into(), min: 10.0, max: 200.0, step: 10.0 },
            ParameterRange { name: "v3_sell_max_hold_hi".into(), min: 50.0, max: 1000.0, step: 10.0 },
            ParameterRange { name: "v3_sell_max_hold_pow".into(), min: 0.1, max: 3.0, step: 0.1 },
            // Misc
            ParameterRange { name: "v3_fee_rate".into(), min: 0.0, max: 0.005, step: 0.0001 },
            ParameterRange { name: "v3_min_hold_bars".into(), min: 1.0, max: 24.0, step: 1.0 },
            ParameterRange { name: "v3_volume_lookback".into(), min: 5.0, max: 60.0, step: 5.0 },
        ]
    }
}
