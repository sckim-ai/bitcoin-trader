use crate::core::engine::push_signal;
use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradeRecord, TradingParameters};
use std::collections::HashMap;

use super::enhanced_adaptive::EnhancedAdaptiveStrategy;
use super::regime_adaptive_v31::RegimeAdaptiveV31Strategy;
use super::Strategy;

/// V6 HybridAdaptive.
///
/// V6 combines the practical parts of the existing adaptive family:
///   * V3.1 trade-value thresholds (`close * volume`) for price-scale stability.
///   * V5 dual PSY confirmation for buy/sell decay confirmation.
///   * ATR stop and ATR trailing exits as an internal risk layer.
///   * Optional ADX buy filter (`v6_min_adx`; 0 disables it).
pub struct HybridAdaptiveV6Strategy;

fn rsi_param(rsi: f64, lo: f64, hi: f64, pow: f64) -> f64 {
    let t = ((rsi - 20.0) / 60.0).clamp(0.0, 1.0);
    let t_pow = if pow > 0.0 { t.powf(pow) } else { t };
    lo + (hi - lo) * t_pow
}

fn safe_rsi(rsi: f64) -> f64 {
    if rsi > 0.0 {
        rsi
    } else {
        50.0
    }
}

fn trend_ok(current: &MarketData, params: &TradingParameters) -> bool {
    params.v6_min_adx <= 0.0 || current.indicators.adx >= params.v6_min_adx
}

impl Strategy for HybridAdaptiveV6Strategy {
    fn name(&self) -> &str {
        "Hybrid Adaptive TV+PSY+ATR (V6)"
    }

    fn description(&self) -> &str {
        "V3.1 trade-value state machine + V5 dual PSY confirmation + ATR risk exits"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        let mut result = SimulationResult::default();
        if data.len() < 30 {
            return result;
        }

        let mut position: i32 = 0;
        let mut previous_position: i32 = 0;
        let mut buy_price: f64 = 0.0;
        let mut set_trade_value: f64 = 0.0;
        let mut buy_sign: i32 = 0;
        let mut sell_sign: i32 = 0;
        let mut buy_po: i32 = 0;
        let mut sell_po: i32 = 0;
        let mut highest_since_buy: f64 = 0.0;
        let mut hold_bars: i32 = 0;
        let mut entry_rsi: f64 = 50.0;

        let mut market_returns: Vec<f64> = Vec::new();
        let mut strategy_returns: Vec<f64> = Vec::new();
        let mut trade_returns: Vec<f64> = Vec::new();

        let mut buy_index: usize = 0;
        let mut buy_signal_str = String::new();
        let mut wins: usize = 0;
        let mut sum_wins: f64 = 0.0;
        let mut sum_losses: f64 = 0.0;
        let mut consecutive_losses: usize = 0;
        let mut max_consecutive_losses: usize = 0;
        let mut prev_signal_type = String::from("ready");

        for i in 1..data.len() {
            let current = &data[i];
            let previous = &data[i - 1];
            let current_rsi = safe_rsi(current.indicators.rsi);
            let tv = current.candle.close * current.candle.volume;
            let mut skip_exec = false;

            if position == 0 {
                let urgent_buy_tv = rsi_param(
                    current_rsi,
                    params.v31_urgent_buy_tv_lo,
                    params.v31_urgent_buy_tv_hi,
                    params.v31_urgent_buy_tv_pow,
                );
                let buy_ready_tv = rsi_param(
                    current_rsi,
                    params.v31_buy_tv_lo,
                    params.v31_buy_tv_hi,
                    params.v31_buy_tv_pow,
                );
                let buy_price_drop = rsi_param(
                    current_rsi,
                    params.v31_buy_price_drop_lo,
                    params.v31_buy_price_drop_hi,
                    params.v31_buy_price_drop_pow,
                );
                let buy_decay = rsi_param(
                    current_rsi,
                    params.v31_buy_decay_lo,
                    params.v31_buy_decay_hi,
                    params.v31_buy_decay_pow,
                );
                let buy_psy_hour = rsi_param(
                    current_rsi,
                    params.v5_buy_psy_hour_lo,
                    params.v5_buy_psy_hour_hi,
                    params.v5_buy_psy_hour_pow,
                );
                let buy_psy_day = rsi_param(
                    current_rsi,
                    params.v5_buy_psy_day_lo,
                    params.v5_buy_psy_day_hi,
                    params.v5_buy_psy_day_pow,
                );
                let buy_wait_max = rsi_param(
                    current_rsi,
                    params.v31_buy_wait_lo,
                    params.v31_buy_wait_hi,
                    params.v31_buy_wait_pow,
                ) as i32;

                let cls = current.candle.close;
                let prev_close = previous.candle.close;
                let psy_hour = current.indicators.psy_hour;
                let psy_day = current.indicators.psy_day;
                let trend_pass = trend_ok(current, params);

                if trend_pass && tv > urgent_buy_tv && prev_close * buy_price_drop > cls {
                    buy_sign = 3;
                    buy_signal_str = "v6_urgent_buy_tv".into();
                    result.buy_signals += 1;
                } else if tv > buy_ready_tv && prev_close * buy_price_drop > cls {
                    buy_sign = 1;
                    buy_po = 0;
                    set_trade_value = tv;
                    result.buy_signals += 1;
                    skip_exec = true;
                } else if buy_sign == 1 {
                    buy_po += 1;
                    if buy_po < buy_wait_max && tv > buy_ready_tv {
                        buy_po = 0;
                    } else if trend_pass
                        && tv < set_trade_value * buy_decay
                        && psy_hour < buy_psy_hour
                        && psy_day < buy_psy_day
                    {
                        buy_sign = 2;
                        buy_signal_str = "v6_dual_psy_buy_tv".into();
                    }
                }

                if !skip_exec && (buy_sign == 2 || buy_sign == 3) {
                    buy_price = cls;
                    position = 1;
                    buy_sign = 0;
                    highest_since_buy = cls;
                    hold_bars = 0;
                    entry_rsi = current_rsi;
                    buy_index = i;
                }
            } else if position == 1 {
                hold_bars += 1;
                let cls = current.candle.close;
                if cls > highest_since_buy {
                    highest_since_buy = cls;
                }

                if hold_bars >= params.v31_min_hold_bars {
                    let fixed_sl = rsi_param(
                        entry_rsi,
                        params.v31_sell_fixed_sl_lo,
                        params.v31_sell_fixed_sl_hi,
                        params.v31_sell_fixed_sl_pow,
                    );
                    let stop_loss_price = rsi_param(
                        entry_rsi,
                        params.v31_sell_stop_loss_lo,
                        params.v31_sell_stop_loss_hi,
                        params.v31_sell_stop_loss_pow,
                    );
                    let sell_profit = rsi_param(
                        entry_rsi,
                        params.v31_sell_profit_lo,
                        params.v31_sell_profit_hi,
                        params.v31_sell_profit_pow,
                    );
                    let sell_ready_tv = rsi_param(
                        entry_rsi,
                        params.v31_sell_tv_lo,
                        params.v31_sell_tv_hi,
                        params.v31_sell_tv_pow,
                    );
                    let sell_decay = rsi_param(
                        entry_rsi,
                        params.v31_sell_decay_lo,
                        params.v31_sell_decay_hi,
                        params.v31_sell_decay_pow,
                    );
                    let max_hold = rsi_param(
                        entry_rsi,
                        params.v31_sell_max_hold_lo,
                        params.v31_sell_max_hold_hi,
                        params.v31_sell_max_hold_pow,
                    ) as i32;
                    let sell_psy_hour = rsi_param(
                        entry_rsi,
                        params.v5_sell_psy_hour_lo,
                        params.v5_sell_psy_hour_hi,
                        params.v5_sell_psy_hour_pow,
                    );
                    let sell_psy_day = rsi_param(
                        entry_rsi,
                        params.v5_sell_psy_day_lo,
                        params.v5_sell_psy_day_hi,
                        params.v5_sell_psy_day_pow,
                    );

                    let prev_close = previous.candle.close;
                    let psy_hour = current.indicators.psy_hour;
                    let psy_day = current.indicators.psy_day;
                    let atr = current.indicators.atr;

                    if fixed_sl > 0.0 && cls < buy_price * (1.0 - fixed_sl) {
                        sell_sign = 2;
                    }
                    if atr > 0.0
                        && params.v6_atr_stop_mult > 0.0
                        && cls < buy_price - atr * params.v6_atr_stop_mult
                    {
                        sell_sign = 2;
                    }
                    if atr > 0.0
                        && params.v6_atr_trail_mult > 0.0
                        && cls < highest_since_buy - atr * params.v6_atr_trail_mult
                    {
                        sell_sign = 2;
                    }
                    if max_hold > 0 && hold_bars > max_hold {
                        sell_sign = 2;
                    }
                    if tv > set_trade_value * params.v31_cutoff_tv_mult
                        && cls < buy_price * stop_loss_price
                    {
                        sell_sign = 2;
                    }
                    if voluminous_sell(tv, sell_ready_tv, params) && cls > buy_price * sell_profit {
                        sell_sign = 3;
                        skip_exec = true;
                    }

                    if !skip_exec {
                        if sell_sign == 3 && tv > sell_ready_tv {
                            sell_sign = 2;
                        }
                        if sell_sign == 0
                            && tv > sell_ready_tv
                            && prev_close * params.v31_sell_ready_price_rise < cls
                        {
                            sell_sign = 1;
                            sell_po = 0;
                            skip_exec = true;
                        }
                        if !skip_exec && sell_sign == 1 {
                            sell_po += 1;
                            if sell_po < params.v31_sell_wait_max && tv > sell_ready_tv {
                                sell_po = 0;
                            } else if tv < set_trade_value * sell_decay
                                && psy_hour > sell_psy_hour
                                && psy_day > sell_psy_day
                            {
                                sell_sign = 2;
                            }
                        }
                    }

                    if !skip_exec && sell_sign == 2 {
                        let sell_price = cls;
                        let pnl_pct = (sell_price - buy_price) / buy_price;
                        trade_returns.push(pnl_pct);
                        if pnl_pct > 0.0 {
                            wins += 1;
                            sum_wins += pnl_pct;
                            consecutive_losses = 0;
                        } else {
                            sum_losses += pnl_pct.abs();
                            consecutive_losses += 1;
                            max_consecutive_losses = max_consecutive_losses.max(consecutive_losses);
                        }
                        result.trades.push(TradeRecord {
                            buy_index,
                            sell_index: i,
                            buy_price,
                            sell_price,
                            pnl_pct,
                            hold_bars,
                            buy_signal: buy_signal_str.clone(),
                            sell_signal: "v6_sell".into(),
                            buy_timestamp: data[buy_index].candle.timestamp.to_rfc3339(),
                            sell_timestamp: current.candle.timestamp.to_rfc3339(),
                        });
                        result.sell_signals += 1;

                        position = 0;
                        sell_sign = 0;
                        highest_since_buy = 0.0;
                        hold_bars = 0;
                    }
                }
            }

            if i > 10 {
                let daily_return =
                    (current.candle.close - previous.candle.close) / previous.candle.close;
                market_returns.push(daily_return);

                let mut bar_return = previous_position as f64 * daily_return;
                if previous_position == 0 && position == 1 {
                    bar_return -= params.v31_fee_rate;
                }
                if previous_position == 1 && position == 0 {
                    bar_return -= params.v31_fee_rate;
                }
                strategy_returns.push(bar_return);
            }

            push_signal(
                &mut result.signal_log,
                &mut prev_signal_type,
                i,
                data,
                position,
                buy_sign,
                sell_sign,
                previous_position,
            );

            previous_position = position;
        }

        let cumulative_product: f64 = strategy_returns.iter().map(|r| 1.0 + r).product();
        let total_return = cumulative_product - 1.0;
        let market_product: f64 = market_returns.iter().map(|r| 1.0 + r).product();
        let market_return = market_product - 1.0;

        let days = {
            let first = data[0].candle.timestamp;
            let last = data[data.len() - 1].candle.timestamp;
            let d = (last - first).num_seconds() as f64 / 86400.0;
            if d <= 0.0 {
                1.0
            } else {
                d
            }
        };
        let annual_return = (1.0 + total_return).powf(365.0 / days) - 1.0;

        let sharpe = if strategy_returns.len() > 1 {
            let mean: f64 = strategy_returns.iter().sum::<f64>() / strategy_returns.len() as f64;
            let variance = strategy_returns
                .iter()
                .map(|r| (r - mean).powi(2))
                .sum::<f64>()
                / strategy_returns.len() as f64;
            let std = variance.sqrt();
            if std > 0.0 {
                mean / std * (365.0_f64 * 24.0).sqrt()
            } else {
                0.0
            }
        } else {
            0.0
        };

        let downside: Vec<f64> = strategy_returns
            .iter()
            .copied()
            .filter(|r| *r < 0.0)
            .collect();
        let sortino = if downside.len() > 1 {
            let mean: f64 = strategy_returns.iter().sum::<f64>() / strategy_returns.len() as f64;
            let downside_std =
                (downside.iter().map(|r| r * r).sum::<f64>() / downside.len() as f64).sqrt();
            if downside_std > 0.0 {
                mean / downside_std * (365.0_f64 * 24.0).sqrt()
            } else {
                0.0
            }
        } else {
            0.0
        };

        let mut peak = 1.0_f64;
        let mut max_dd = 0.0_f64;
        let mut cum = 1.0_f64;
        for r in &strategy_returns {
            cum *= 1.0 + r;
            if cum > peak {
                peak = cum;
            }
            let dd = (peak - cum) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }

        let total_trades = result.trades.len();
        result.total_trades = total_trades;
        result.total_return = total_return * 100.0;
        result.market_return = market_return * 100.0;
        result.annual_return = annual_return * 100.0;
        result.max_drawdown = max_dd * 100.0;
        result.max_consecutive_losses = max_consecutive_losses;
        if total_trades > 0 {
            result.win_rate = wins as f64 / total_trades as f64 * 100.0;
            result.avg_trade_return =
                trade_returns.iter().sum::<f64>() / total_trades as f64 * 100.0;
        }
        result.profit_factor = if sum_losses > 0.0 {
            sum_wins / sum_losses
        } else if sum_wins > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };
        result.sharpe_ratio = sharpe;
        result.sortino_ratio = sortino;
        result.calmar_ratio = if max_dd > 0.0 {
            annual_return / max_dd
        } else {
            0.0
        };

        result.last_position = position;
        result.last_buy_price = buy_price;
        result.last_set_volume = set_trade_value;
        result.last_hold_bars = hold_bars;
        result.last_entry_rsi = entry_rsi;
        result.last_highest_since_buy = highest_since_buy;
        result.last_signal_type = if position == 1 {
            "holding".into()
        } else {
            "idle".into()
        };

        result
    }

    fn get_latest_signal(
        &self,
        data: &[MarketData],
        params: &TradingParameters,
        position: &PositionState,
    ) -> TradingSignal {
        if data.len() < 3 {
            return TradingSignal {
                signal_type: SignalType::Hold,
                confidence: None,
                metadata: HashMap::new(),
            };
        }
        let prev = self.run_simulation(&data[..data.len() - 1], params);
        let full = self.run_simulation(data, params);

        let signal_type = if position.position == 0 {
            if full.last_position == 1 && prev.last_position == 0 {
                SignalType::Buy
            } else if full.total_trades > prev.total_trades && full.last_position == 1 {
                SignalType::Buy
            } else {
                SignalType::Hold
            }
        } else if full.total_trades > prev.total_trades && full.last_position == 0 {
            SignalType::Sell
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
        let mut ranges: Vec<ParameterRange> = RegimeAdaptiveV31Strategy
            .parameter_ranges()
            .into_iter()
            .filter(|r| {
                !matches!(
                    r.name.as_str(),
                    "v31_buy_psy_lo" | "v31_buy_psy_hi" | "v31_buy_psy_pow"
                )
            })
            .collect();

        ranges.extend(
            EnhancedAdaptiveStrategy
                .parameter_ranges()
                .into_iter()
                .filter(|r| r.name.starts_with("v5_")),
        );
        ranges.extend([
            ParameterRange {
                name: "v6_atr_stop_mult".into(),
                min: 0.0,
                max: 8.0,
                step: 0.1,
            },
            ParameterRange {
                name: "v6_atr_trail_mult".into(),
                min: 0.0,
                max: 12.0,
                step: 0.1,
            },
            ParameterRange {
                name: "v6_min_adx".into(),
                min: 0.0,
                max: 50.0,
                step: 1.0,
            },
        ]);
        ranges
    }
}

fn voluminous_sell(tv: f64, sell_ready_tv: f64, params: &TradingParameters) -> bool {
    tv > sell_ready_tv * params.v31_urgent_sell_tv_mult
}
