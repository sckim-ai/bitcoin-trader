use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradeRecord, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

/// V2 MultiIndicator: composite score from RSI + MACD histogram + Bollinger %B.
pub struct MultiIndicatorStrategy;

impl MultiIndicatorStrategy {
    fn compute_buy_score(ind: &crate::models::market::IndicatorSet, params: &TradingParameters) -> f64 {
        let total_weight = params.v2_rsi_weight + params.v2_macd_weight + params.v2_bb_weight;
        if total_weight <= 0.0 {
            return 0.0;
        }

        // RSI score: oversold(<=30)=1.0, overbought(>=70)=0.0, linear between
        let rsi_score = if ind.rsi <= 30.0 {
            1.0
        } else if ind.rsi >= 70.0 {
            0.0
        } else {
            1.0 - (ind.rsi - 30.0) / 40.0
        };

        // MACD histogram: positive = bullish (buy signal)
        let macd_score = if ind.macd_histogram > 0.0 { 1.0 } else { 0.0 };

        // Bollinger %B: close below lower band = buy signal
        let bb_range = ind.bollinger_upper - ind.bollinger_lower;
        let bb_pct_b = if bb_range > 0.0 {
            // %B = 0 at lower band, 1 at upper band
            // For buy: low %B is good
            let pct_b = (ind.bollinger_middle - ind.bollinger_lower) / bb_range; // approximate with middle
            1.0 - pct_b.clamp(0.0, 1.0)
        } else {
            0.5
        };

        (rsi_score * params.v2_rsi_weight + macd_score * params.v2_macd_weight + bb_pct_b * params.v2_bb_weight)
            / total_weight
    }

    fn compute_sell_score(ind: &crate::models::market::IndicatorSet, params: &TradingParameters) -> f64 {
        let total_weight = params.v2_rsi_weight + params.v2_macd_weight + params.v2_bb_weight;
        if total_weight <= 0.0 {
            return 0.0;
        }

        // RSI score for sell: overbought(>=70)=1.0, oversold(<=30)=0.0
        let rsi_score = if ind.rsi >= 70.0 {
            1.0
        } else if ind.rsi <= 30.0 {
            0.0
        } else {
            (ind.rsi - 30.0) / 40.0
        };

        // MACD histogram: negative = bearish (sell signal)
        let macd_score = if ind.macd_histogram < 0.0 { 1.0 } else { 0.0 };

        // Bollinger %B: close above upper band = sell signal
        let bb_range = ind.bollinger_upper - ind.bollinger_lower;
        let bb_pct_b = if bb_range > 0.0 {
            let pct_b = (ind.bollinger_middle - ind.bollinger_lower) / bb_range;
            pct_b.clamp(0.0, 1.0)
        } else {
            0.5
        };

        (rsi_score * params.v2_rsi_weight + macd_score * params.v2_macd_weight + bb_pct_b * params.v2_bb_weight)
            / total_weight
    }
}

impl Strategy for MultiIndicatorStrategy {
    fn name(&self) -> &str {
        "Multi Indicator (V2)"
    }

    fn description(&self) -> &str {
        "Composite score from RSI, MACD histogram, and Bollinger %B"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        let mut result = SimulationResult::default();
        if data.is_empty() {
            return result;
        }

        let mut position: i32 = 0;
        let mut buy_price: f64 = 0.0;
        let mut hold_bars: i32 = 0;
        let mut highest_since_buy: f64 = 0.0;
        let mut buy_index: usize = 0;

        let mut cumulative_return: f64 = 1.0;
        let mut peak: f64 = 1.0;
        let mut max_drawdown: f64 = 0.0;
        let mut wins: usize = 0;
        let mut sum_wins: f64 = 0.0;
        let mut sum_losses: f64 = 0.0;
        let mut consecutive_losses: usize = 0;
        let mut max_consecutive_losses: usize = 0;
        let mut trade_returns: Vec<f64> = Vec::new();

        let fee_rate = params.fee_rate;

        for i in 1..data.len() {
            let close = data[i].candle.close;
            let ind = &data[i].indicators;

            if position == 0 {
                let buy_score = Self::compute_buy_score(ind, params);
                if buy_score >= params.v2_buy_score_threshold {
                    buy_price = close + close * fee_rate;
                    position = 1;
                    hold_bars = 0;
                    highest_since_buy = close;
                    buy_index = i;
                    result.buy_signals += 1;
                }
            } else {
                hold_bars += 1;
                if close > highest_since_buy {
                    highest_since_buy = close;
                }

                let mut should_sell = false;
                let mut sell_signal_str = String::new();

                // Fixed stop loss
                if params.fixed_stop_loss_pct > 0.0 {
                    let loss_pct = (close - buy_price) / buy_price;
                    if loss_pct <= -params.fixed_stop_loss_pct {
                        should_sell = true;
                        sell_signal_str = "fixed_stop_loss".to_string();
                    }
                }

                // Fixed take profit
                if !should_sell && params.fixed_take_profit_pct > 0.0 {
                    let gain_pct = (close - buy_price) / buy_price;
                    if gain_pct >= params.fixed_take_profit_pct {
                        should_sell = true;
                        sell_signal_str = "fixed_take_profit".to_string();
                    }
                }

                // Max hold
                if !should_sell && params.max_hold_periods > 0 && hold_bars >= params.max_hold_periods {
                    should_sell = true;
                    sell_signal_str = "max_hold".to_string();
                }

                // Score-based sell
                if !should_sell {
                    let sell_score = Self::compute_sell_score(ind, params);
                    if sell_score >= params.v2_sell_score_threshold {
                        should_sell = true;
                        sell_signal_str = "score_sell".to_string();
                        result.sell_signals += 1;
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
                        buy_signal: "score_buy".to_string(),
                        sell_signal: sell_signal_str,
                    });

                    position = 0;
                    buy_price = 0.0;
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
        result.last_hold_bars = hold_bars;
        result.last_highest_since_buy = highest_since_buy;
        result.last_signal_type = if position == 1 { "holding".into() } else { "idle".into() };

        result
    }

    fn get_latest_signal(
        &self,
        data: &[MarketData],
        params: &TradingParameters,
        position: &PositionState,
    ) -> TradingSignal {
        if data.is_empty() {
            return TradingSignal {
                signal_type: SignalType::Hold,
                confidence: None,
                metadata: HashMap::new(),
            };
        }
        let last = &data[data.len() - 1];
        let ind = &last.indicators;

        let (signal_type, confidence) = if position.position == 0 {
            let score = Self::compute_buy_score(ind, params);
            if score >= params.v2_buy_score_threshold {
                (SignalType::Buy, score)
            } else {
                (SignalType::Hold, score)
            }
        } else {
            let score = Self::compute_sell_score(ind, params);
            if score >= params.v2_sell_score_threshold {
                (SignalType::Sell, score)
            } else {
                (SignalType::Hold, score)
            }
        };

        TradingSignal {
            signal_type,
            confidence: Some(confidence),
            metadata: HashMap::new(),
        }
    }

    fn parameter_ranges(&self) -> Vec<ParameterRange> {
        vec![
            ParameterRange { name: "v2_rsi_weight".into(), min: 0.0, max: 3.0, step: 0.1 },
            ParameterRange { name: "v2_macd_weight".into(), min: 0.0, max: 3.0, step: 0.1 },
            ParameterRange { name: "v2_bb_weight".into(), min: 0.0, max: 3.0, step: 0.1 },
            ParameterRange { name: "v2_buy_score_threshold".into(), min: 0.3, max: 0.9, step: 0.05 },
            ParameterRange { name: "v2_sell_score_threshold".into(), min: 0.3, max: 0.9, step: 0.05 },
            ParameterRange { name: "fee_rate".into(), min: 0.0, max: 0.01, step: 0.0001 },
            ParameterRange { name: "fixed_stop_loss_pct".into(), min: 0.0, max: 0.2, step: 0.01 },
            ParameterRange { name: "fixed_take_profit_pct".into(), min: 0.0, max: 0.5, step: 0.01 },
            ParameterRange { name: "max_hold_periods".into(), min: 0.0, max: 1000.0, step: 10.0 },
        ]
    }
}
