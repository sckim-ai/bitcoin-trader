use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradeRecord, TradingParameters};

/// Run V0 Volume Decay trading simulation on market data.
pub fn run_simulation(data: &[MarketData], params: &TradingParameters) -> SimulationResult {
    let mut result = SimulationResult::default();
    if data.is_empty() {
        return result;
    }

    // State variables
    let mut position: i32 = 0; // 0=idle, 1=holding
    let mut buy_price: f64 = 0.0;
    let mut set_volume: f64 = 0.0;
    let mut buy_sign: i32 = 0; // 0=none, 1=ready, 2=confirmed
    let mut sell_sign: i32 = 0;
    let mut buy_wait_bars: i32 = 0;
    let mut sell_wait_bars: i32 = 0;
    let mut hold_bars: i32 = 0;
    let mut highest_since_buy: f64 = 0.0;

    // Metrics tracking
    let mut cumulative_return: f64 = 1.0;
    let mut peak: f64 = 1.0;
    let mut max_drawdown: f64 = 0.0;
    let mut wins: usize = 0;
    let mut _losses: usize = 0;
    let mut sum_wins: f64 = 0.0;
    let mut sum_losses: f64 = 0.0;
    let mut consecutive_losses: usize = 0;
    let mut max_consecutive_losses: usize = 0;
    let mut trade_returns: Vec<f64> = Vec::new();

    let mut buy_index: usize = 0;
    let mut buy_signal_str = String::new();
    let mut entry_rsi: f64 = 0.0;

    let fee_rate = params.fee_rate;

    for i in 1..data.len() {
        let close = data[i].candle.close;
        let volume = data[i].candle.volume;
        let prev_close = data[i - 1].candle.close;
        let price_change = if prev_close != 0.0 {
            (close - prev_close) / prev_close
        } else {
            0.0
        };

        if position == 0 {
            // IDLE state
            match buy_sign {
                0 => {
                    // Check urgent buy
                    if volume >= params.urgent_buy_volume_threshold
                        && price_change <= -params.urgent_buy_price_drop_ratio
                    {
                        buy_sign = 2;
                        set_volume = volume;
                        buy_signal_str = "urgent_buy".to_string();
                        result.buy_signals += 1;
                    }
                    // Check ready buy
                    else if volume >= params.buy_ready_volume_threshold
                        && price_change <= -params.buy_ready_price_drop_ratio
                    {
                        buy_sign = 1;
                        set_volume = volume;
                        buy_wait_bars = 0;
                        result.buy_signals += 1;
                    }
                }
                1 => {
                    buy_wait_bars += 1;
                    // Check volume decay → confirm
                    if volume <= set_volume * params.buy_confirm_volume_decay_ratio {
                        buy_sign = 2;
                        buy_signal_str = "decay_buy".to_string();
                    }
                    // Timeout
                    else if buy_wait_bars > params.buy_wait_max_periods {
                        buy_sign = 0;
                    }
                }
                _ => {}
            }

            // Execute buy if confirmed
            if buy_sign == 2 {
                buy_price = close + close * fee_rate;
                position = 1;
                hold_bars = 0;
                highest_since_buy = close;
                buy_sign = 0;
                sell_sign = 0;
                sell_wait_bars = 0;
                buy_index = i;
                entry_rsi = data[i].indicators.rsi;
            }
        } else {
            // HOLDING state
            hold_bars += 1;
            if close > highest_since_buy {
                highest_since_buy = close;
            }

            let mut should_sell = false;
            let mut sell_signal_str = String::new();

            // Risk management checks (in order)

            // 1. Fixed stop loss
            if params.fixed_stop_loss_pct > 0.0 {
                let loss_pct = (close - buy_price) / buy_price;
                if loss_pct <= -params.fixed_stop_loss_pct {
                    should_sell = true;
                    sell_signal_str = "fixed_stop_loss".to_string();
                }
            }

            // 2. Fixed take profit
            if !should_sell && params.fixed_take_profit_pct > 0.0 {
                let gain_pct = (close - buy_price) / buy_price;
                if gain_pct >= params.fixed_take_profit_pct {
                    should_sell = true;
                    sell_signal_str = "fixed_take_profit".to_string();
                }
            }

            // 3. Trailing stop
            if !should_sell && params.trailing_stop_pct > 0.0 {
                let drop_from_peak = (highest_since_buy - close) / highest_since_buy;
                if drop_from_peak >= params.trailing_stop_pct {
                    should_sell = true;
                    sell_signal_str = "trailing_stop".to_string();
                }
            }

            // 4. Max hold periods
            if !should_sell && params.max_hold_periods > 0 && hold_bars >= params.max_hold_periods {
                should_sell = true;
                sell_signal_str = "max_hold".to_string();
            }

            // 5. Volume-based sell (same pattern as buy)
            if !should_sell {
                match sell_sign {
                    0 => {
                        // Urgent sell
                        if volume >= params.urgent_sell_volume_threshold
                            && price_change >= params.urgent_sell_profit_ratio
                        {
                            should_sell = true;
                            sell_signal_str = "urgent_sell".to_string();
                            result.sell_signals += 1;
                        }
                        // Ready sell
                        else if volume >= params.sell_ready_volume_threshold
                            && price_change >= params.sell_ready_price_rise_ratio
                        {
                            sell_sign = 1;
                            set_volume = volume;
                            sell_wait_bars = 0;
                            result.sell_signals += 1;
                        }
                    }
                    1 => {
                        sell_wait_bars += 1;
                        // Volume decay → confirm sell
                        if volume <= set_volume * params.sell_confirm_volume_decay_ratio {
                            should_sell = true;
                            sell_signal_str = "decay_sell".to_string();
                        }
                        // Timeout
                        else if sell_wait_bars > params.sell_wait_max_periods {
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
                    _losses += 1;
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

                // Reset state
                position = 0;
                buy_price = 0.0;
                buy_sign = 0;
                sell_sign = 0;
                hold_bars = 0;
                highest_since_buy = 0.0;
            }
        }
    }

    // Calculate final metrics
    let total_trades = result.trades.len();
    result.total_trades = total_trades;
    result.total_return = (cumulative_return - 1.0) * 100.0;
    result.fee_adjusted_return = result.total_return;
    result.max_drawdown = max_drawdown * 100.0;
    result.max_consecutive_losses = max_consecutive_losses;

    if total_trades > 0 {
        result.win_rate = (wins as f64 / total_trades as f64) * 100.0;
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

    // Market return (buy & hold)
    let first_close = data[0].candle.close;
    let last_close = data[data.len() - 1].candle.close;
    if first_close > 0.0 {
        result.market_return = ((last_close - first_close) / first_close) * 100.0;
    }

    // Sharpe/Sortino/Calmar (simplified, assume risk-free = 0)
    if trade_returns.len() >= 2 {
        let mean = trade_returns.iter().sum::<f64>() / trade_returns.len() as f64;
        let variance =
            trade_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (trade_returns.len() - 1) as f64;
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

    // Annualized return (rough: assume hourly data, 8760 hours/year)
    let n_bars = data.len() as f64;
    if n_bars > 0.0 && cumulative_return > 0.0 {
        result.annual_return = (cumulative_return.powf(8760.0 / n_bars) - 1.0) * 100.0;
    }

    // Last state
    result.last_position = position;
    result.last_buy_price = buy_price;
    result.last_set_volume = set_volume;
    result.last_hold_bars = hold_bars;
    result.last_entry_rsi = entry_rsi;
    result.last_highest_since_buy = highest_since_buy;
    result.last_signal_type = if position == 1 {
        "holding".to_string()
    } else {
        "idle".to_string()
    };

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::{Candle, IndicatorSet, MarketData};
    use chrono::{TimeZone, Utc};

    fn make_market_data(prices: &[(f64, f64)]) -> Vec<MarketData> {
        // (close, volume) pairs
        prices
            .iter()
            .enumerate()
            .map(|(i, &(close, volume))| MarketData {
                candle: Candle {
                    timestamp: Utc.with_ymd_and_hms(2024, 1, 1, i as u32 % 24, 0, 0).unwrap(),
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

    #[test]
    fn test_no_trades_low_volume() {
        // Flat data with low volume — no signals should trigger
        let data = make_market_data(
            &(0..100)
                .map(|_| (100.0, 100.0)) // volume=100, way below thresholds
                .collect::<Vec<_>>(),
        );
        let params = TradingParameters::default();
        let result = run_simulation(&data, &params);
        assert_eq!(result.total_trades, 0);
    }

    #[test]
    fn test_buy_signal_on_high_volume() {
        // Create a scenario with high volume spike + price drop to trigger buy
        let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 10]; // low vol baseline
        // Spike: volume >= urgent_buy_volume_threshold (30000) AND price_change <= -urgent_buy_price_drop_ratio (1.0 = 100%)
        // With default params, urgent_buy_price_drop_ratio=1.0, so price_change must be <= -1.0 (impossible).
        // Use buy_ready path instead: volume >= 9000, price_change <= -1.0 (also impossible with default).
        // Default buy_ready_price_drop_ratio=1.0 → need 100% drop. Let's use custom params.
        let mut params = TradingParameters::default();
        params.buy_ready_volume_threshold = 500.0;
        params.buy_ready_price_drop_ratio = 0.01; // 1% drop
        params.buy_confirm_volume_decay_ratio = 0.5;
        params.buy_wait_max_periods = 10;

        // Add price drop with high volume, then decay
        prices.push((98.0, 600.0)); // ready buy triggers (1% drop + vol > 500)
        prices.push((97.5, 200.0)); // volume decays to 200 < 600*0.5=300 → confirmed
        // Now holding, add some flat bars then sell via max_hold
        params.max_hold_periods = 3;
        for _ in 0..5 {
            prices.push((99.0, 100.0));
        }

        let data = make_market_data(&prices);
        let result = run_simulation(&data, &params);
        assert!(result.buy_signals > 0, "should have at least one buy signal");
    }

    #[test]
    fn test_round_trip_trade() {
        let mut params = TradingParameters::default();
        params.buy_ready_volume_threshold = 500.0;
        params.buy_ready_price_drop_ratio = 0.005;
        params.buy_confirm_volume_decay_ratio = 0.5;
        params.buy_wait_max_periods = 10;
        params.fixed_take_profit_pct = 0.03; // 3% take profit
        params.fee_rate = 0.001;

        let mut prices: Vec<(f64, f64)> = vec![(100.0, 100.0); 5];
        // Trigger buy: price drop + high volume
        prices.push((99.0, 600.0)); // ready
        prices.push((98.5, 200.0)); // decay confirms → buy at ~98.5
        // Price rises to trigger take profit
        prices.push((102.0, 100.0));
        prices.push((105.0, 100.0)); // 105 vs buy ~98.6 → ~6.5% gain > 3% TP

        let data = make_market_data(&prices);
        let result = run_simulation(&data, &params);
        assert!(
            result.total_trades >= 1,
            "should complete at least one round trip, got {} trades",
            result.total_trades
        );
        assert!(
            result.total_return > 0.0,
            "should have positive return, got {}",
            result.total_return
        );
    }

    #[test]
    fn test_empty_data() {
        let data: Vec<MarketData> = vec![];
        let params = TradingParameters::default();
        let result = run_simulation(&data, &params);
        assert_eq!(result.total_trades, 0);
        assert_eq!(result.total_return, 0.0);
    }
}
