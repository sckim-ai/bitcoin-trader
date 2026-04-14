use crate::core::ml::trainer::WalkForwardTrainer;
use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradeRecord, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

/// V4 MachineLearning: Walk-forward trained linear model for buy/sell signals.
pub struct MachineLearningStrategy;

impl Strategy for MachineLearningStrategy {
    fn name(&self) -> &str {
        "Machine Learning (V4)"
    }

    fn description(&self) -> &str {
        "Walk-forward trained model with momentum/volume/indicator features"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        let mut result = SimulationResult::default();
        if data.is_empty() {
            return result;
        }

        // Run walk-forward training
        let trainer = WalkForwardTrainer {
            train_window: params.v4_train_window.max(100) as usize,
            retrain_interval: params.v4_retrain_interval.max(50) as usize,
        };
        let predictions = trainer.train_and_predict(data);

        let fee_rate = params.v4_fee_rate;
        let buy_threshold = params.v4_buy_threshold;
        let stop_loss = params.v4_stop_loss;
        let take_profit = params.v4_take_profit;
        let min_hold = params.v4_min_hold_bars;

        let mut position: i32 = 0;
        let mut buy_price: f64 = 0.0;
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

        for i in 1..data.len() {
            let close = data[i].candle.close;

            if position == 0 {
                // Check ML prediction for buy
                if let Some(pred) = &predictions[i] {
                    if pred.predicted_return > buy_threshold && pred.profit_probability > 0.55 {
                        buy_price = close + close * fee_rate;
                        position = 1;
                        hold_bars = 0;
                        highest_since_buy = close;
                        buy_index = i;
                        entry_rsi = data[i].indicators.rsi;
                        result.buy_signals += 1;
                    }
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

                // Fixed stop loss
                let loss_pct = (close - buy_price) / buy_price;
                if stop_loss > 0.0 && loss_pct <= -stop_loss {
                    should_sell = true;
                    sell_signal_str = "ml_stop_loss".into();
                }

                // Fixed take profit
                if !should_sell && take_profit > 0.0 && loss_pct >= take_profit {
                    should_sell = true;
                    sell_signal_str = "ml_take_profit".into();
                }

                // ML-based sell: predicted return is very negative
                if !should_sell {
                    if let Some(pred) = &predictions[i] {
                        if pred.predicted_return < params.v4_sell_threshold {
                            should_sell = true;
                            sell_signal_str = "ml_predict_sell".into();
                            result.sell_signals += 1;
                        }
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
                        buy_signal: "ml_buy".into(),
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
        finalize_result(&mut result, cumulative_return, peak, max_drawdown,
            max_consecutive_losses, wins, sum_wins, sum_losses,
            &trade_returns, data, position, buy_price, hold_bars,
            entry_rsi, highest_since_buy, 0.0);

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
            ParameterRange { name: "v4_train_window".into(), min: 500.0, max: 4000.0, step: 100.0 },
            ParameterRange { name: "v4_retrain_interval".into(), min: 100.0, max: 1500.0, step: 100.0 },
            ParameterRange { name: "v4_buy_threshold".into(), min: 0.001, max: 0.02, step: 0.001 },
            ParameterRange { name: "v4_sell_threshold".into(), min: -0.02, max: -0.001, step: 0.001 },
            ParameterRange { name: "v4_stop_loss".into(), min: 0.02, max: 0.15, step: 0.01 },
            ParameterRange { name: "v4_take_profit".into(), min: 0.02, max: 0.20, step: 0.01 },
            ParameterRange { name: "v4_fee_rate".into(), min: 0.0, max: 0.005, step: 0.0001 },
            ParameterRange { name: "v4_min_hold_bars".into(), min: 1.0, max: 24.0, step: 1.0 },
        ]
    }
}

/// Shared helper to finalize simulation result metrics.
pub(crate) fn finalize_result(
    result: &mut SimulationResult,
    cumulative_return: f64,
    _peak: f64,
    max_drawdown: f64,
    max_consecutive_losses: usize,
    wins: usize,
    sum_wins: f64,
    sum_losses: f64,
    trade_returns: &[f64],
    data: &[MarketData],
    position: i32,
    buy_price: f64,
    hold_bars: i32,
    entry_rsi: f64,
    highest_since_buy: f64,
    set_volume: f64,
) {
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

    if !data.is_empty() {
        let first_close = data[0].candle.close;
        let last_close = data[data.len() - 1].candle.close;
        if first_close > 0.0 {
            result.market_return = ((last_close - first_close) / first_close) * 100.0;
        }
    }

    if trade_returns.len() >= 2 {
        let mean = trade_returns.iter().sum::<f64>() / trade_returns.len() as f64;
        let variance = trade_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
            / (trade_returns.len() - 1) as f64;
        let std_dev = variance.sqrt();
        if std_dev > 0.0 {
            result.sharpe_ratio = mean / std_dev;
        }
        let downside_variance = trade_returns
            .iter()
            .filter(|&&r| r < 0.0)
            .map(|r| r.powi(2))
            .sum::<f64>()
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
}
