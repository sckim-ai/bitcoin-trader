use crate::core::engine::push_signal;
use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SignalEvent, SimulationResult, TradeRecord, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

/// V3.1 RegimeAdaptive (trade-value edition) — V3 포팅 버전.
/// V3 대비 차이:
///   1. 신호 기준이 **거래대금(close × volume, KRW)**. V3는 BTC 거래량.
///      → 가격 스케일 변화(예: ETH 2M → 5M)에 강건.
///   2. V3의 하드코딩 상수 4개가 파라미터화됨:
///        * `v31_cutoff_tv_mult` (V3: 1.0)
///        * `v31_urgent_sell_tv_mult` (V3: 2.0)
///        * `v31_sell_ready_price_rise` (V3: 1.0)
///        * `v31_sell_wait_max` (V3: 168)
///   3. `set_volume` → `set_trade_value` 로 이름만 변경 (의미상 거래대금).
/// 나머지 로직(RSI 보간, bar-by-bar returns, state machine)은 V3와 동일.
pub struct RegimeAdaptiveV31Strategy;

fn rsi_param(rsi: f64, lo: f64, hi: f64, pow: f64) -> f64 {
    let t = ((rsi - 20.0) / 60.0).clamp(0.0, 1.0);
    let t_pow = if pow > 0.0 { t.powf(pow) } else { t };
    lo + (hi - lo) * t_pow
}

fn safe_rsi(rsi: f64) -> f64 {
    if rsi > 0.0 { rsi } else { 50.0 }
}

impl Strategy for RegimeAdaptiveV31Strategy {
    fn name(&self) -> &str {
        "Regime Adaptive TV (V3.1)"
    }

    fn description(&self) -> &str {
        "V3의 거래대금(KRW) 기반 버전 — 가격 스케일 독립성 + legacy 하드코딩 4개 파라미터화"
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

        let mut buy_index: usize = 0;
        let mut buy_signal_str = String::new();
        let mut wins: usize = 0;
        let mut sum_wins: f64 = 0.0;
        let mut sum_losses: f64 = 0.0;
        let mut consecutive_losses: usize = 0;
        let mut max_consecutive_losses: usize = 0;
        let mut trade_returns: Vec<f64> = Vec::new();

        let mut prev_signal_type = String::from("ready");

        let trace_path = std::env::var("V31_TRACE_PATH").ok();
        let mut trace_sink: Option<std::fs::File> = trace_path.as_ref().map(|p| {
            let mut f = std::fs::File::create(p).expect("trace file");
            use std::io::Write as _;
            writeln!(f, "bar,timestamp,close,tv,rsi,psyDay,pos,bSign,sSign,buyPo,sellPo,setTv,entryRsi,event").unwrap();
            f
        });

        for i in 1..data.len() {
            let current = &data[i];
            let previous = &data[i - 1];
            let current_rsi = safe_rsi(current.indicators.rsi);
            let mut skip_exec = false;

            // 거래대금 (KRW) — V3.1 핵심 변경점
            let tv = current.candle.close * current.candle.volume;

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
                let buy_psy = rsi_param(
                    current_rsi,
                    params.v31_buy_psy_lo,
                    params.v31_buy_psy_hi,
                    params.v31_buy_psy_pow,
                );
                let buy_wait_max = rsi_param(
                    current_rsi,
                    params.v31_buy_wait_lo,
                    params.v31_buy_wait_hi,
                    params.v31_buy_wait_pow,
                ) as i32;

                let cls = current.candle.close;
                let prev_close = previous.candle.close;
                let psy = current.indicators.psy_day;

                if tv > urgent_buy_tv && prev_close * buy_price_drop > cls {
                    buy_sign = 3;
                    buy_signal_str = "regime_urgent_buy_tv".into();
                    result.buy_signals += 1;
                } else {
                    if tv > buy_ready_tv && prev_close * buy_price_drop > cls {
                        buy_sign = 1;
                        buy_po = 0;
                        set_trade_value = tv;
                        result.buy_signals += 1;
                        skip_exec = true;
                    } else if buy_sign == 1 {
                        buy_po += 1;
                        if buy_po < buy_wait_max && tv > buy_ready_tv {
                            buy_po = 0;
                        } else if tv < set_trade_value * buy_decay && psy < buy_psy {
                            buy_sign = 2;
                            buy_signal_str = "regime_decay_buy_tv".into();
                        }
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
                if current.candle.close > highest_since_buy {
                    highest_since_buy = current.candle.close;
                }

                if hold_bars < params.v31_min_hold_bars {
                    // goto CalculateReturns
                } else {
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

                    let cls = current.candle.close;
                    let prev_close = previous.candle.close;

                    // 1. Fixed stop loss
                    if fixed_sl > 0.0 && cls < buy_price * (1.0 - fixed_sl) {
                        sell_sign = 2;
                    }

                    // 2. Max hold
                    if max_hold > 0 && hold_bars > max_hold {
                        sell_sign = 2;
                    }

                    // 3. TV cutoff — v31_cutoff_tv_mult (V3: hardcoded 1.0)
                    if tv > set_trade_value * params.v31_cutoff_tv_mult
                        && cls < buy_price * stop_loss_price
                    {
                        sell_sign = 2;
                    }

                    // 4. Urgent sell trigger — v31_urgent_sell_tv_mult (V3: hardcoded 2.0)
                    if tv > sell_ready_tv * params.v31_urgent_sell_tv_mult
                        && cls > buy_price * sell_profit
                    {
                        sell_sign = 3;
                        skip_exec = true;
                    }

                    if !skip_exec {
                        if sell_sign == 3 && tv > sell_ready_tv {
                            sell_sign = 2;
                        }

                        // 6. Ready sell — v31_sell_ready_price_rise (V3: hardcoded 1.0)
                        if sell_sign == 0
                            && tv > sell_ready_tv
                            && prev_close * params.v31_sell_ready_price_rise < cls
                        {
                            sell_sign = 1;
                            sell_po = 0;
                            skip_exec = true;
                        }

                        // 7. Decay confirm — v31_sell_wait_max (V3: hardcoded 168)
                        if !skip_exec && sell_sign == 1 {
                            sell_po += 1;
                            if sell_po < params.v31_sell_wait_max && tv > sell_ready_tv {
                                sell_po = 0;
                            } else if tv < set_trade_value * sell_decay {
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
                            sell_signal: "regime_sell_tv".into(),
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

            // bar-by-bar compounding (V3 identical)
            if i > 10 {
                let daily_return = (current.candle.close - previous.candle.close)
                    / previous.candle.close;
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

            if let Some(f) = trace_sink.as_mut() {
                let evt = if position == 1 && previous_position == 0 { "buy" }
                          else if position == 0 && previous_position == 1 { "sell" }
                          else { "" };
                let kst = current.candle.timestamp + chrono::Duration::hours(9);
                use std::io::Write as _;
                writeln!(
                    f,
                    "{},{},{},{:.2},{:.6},{:.6},{},{},{},{},{},{:.2},{:.6},{}",
                    i,
                    kst.format("%Y-%m-%dT%H:%M:%S"),
                    current.candle.close,
                    tv,
                    safe_rsi(current.indicators.rsi),
                    current.indicators.psy_day,
                    position, buy_sign, sell_sign, buy_po, sell_po,
                    set_trade_value, entry_rsi, evt
                ).unwrap();
            }

            previous_position = position;
        }

        // Performance metrics (V3 identical)
        let cumulative_product: f64 = strategy_returns.iter().map(|r| 1.0 + r).product();
        let total_return = cumulative_product - 1.0;
        let market_product: f64 = market_returns.iter().map(|r| 1.0 + r).product();
        let market_return = market_product - 1.0;

        let days = {
            let first = data[0].candle.timestamp;
            let last = data[data.len() - 1].candle.timestamp;
            let d = (last - first).num_seconds() as f64 / 86400.0;
            if d <= 0.0 { 1.0 } else { d }
        };
        let annual_return = (1.0 + total_return).powf(365.0 / days) - 1.0;

        let sharpe = if strategy_returns.len() > 1 {
            let mean: f64 = strategy_returns.iter().sum::<f64>() / strategy_returns.len() as f64;
            let variance = strategy_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
                / strategy_returns.len() as f64;
            let std = variance.sqrt();
            if std > 0.0 { mean / std * (365.0_f64 * 24.0).sqrt() } else { 0.0 }
        } else { 0.0 };

        let downside: Vec<f64> = strategy_returns.iter().copied().filter(|r| *r < 0.0).collect();
        let sortino = if downside.len() > 1 {
            let mean: f64 = strategy_returns.iter().sum::<f64>() / strategy_returns.len() as f64;
            let downside_std = (downside.iter().map(|r| r * r).sum::<f64>() / downside.len() as f64).sqrt();
            if downside_std > 0.0 { mean / downside_std * (365.0_f64 * 24.0).sqrt() } else { 0.0 }
        } else { 0.0 };

        let mut peak = 1.0_f64;
        let mut max_dd = 0.0_f64;
        let mut cum = 1.0_f64;
        for r in &strategy_returns {
            cum *= 1.0 + r;
            if cum > peak { peak = cum; }
            let dd = (peak - cum) / peak;
            if dd > max_dd { max_dd = dd; }
        }

        let total_trades = result.trades.len();
        result.total_trades = total_trades;
        result.total_return = total_return * 100.0;
        result.fee_adjusted_return = result.total_return;
        result.market_return = market_return * 100.0;
        result.annual_return = annual_return * 100.0;
        result.max_drawdown = max_dd * 100.0;
        result.max_consecutive_losses = max_consecutive_losses;
        if total_trades > 0 {
            result.win_rate = wins as f64 / total_trades as f64 * 100.0;
            result.avg_trade_return = trade_returns.iter().sum::<f64>() / total_trades as f64 * 100.0;
        }
        result.profit_factor = if sum_losses > 0.0 { sum_wins / sum_losses }
            else if sum_wins > 0.0 { f64::INFINITY } else { 0.0 };
        result.sharpe_ratio = sharpe;
        result.sortino_ratio = sortino;
        result.calmar_ratio = if max_dd > 0.0 { annual_return / max_dd } else { 0.0 };

        result.last_position = position;
        result.last_buy_price = buy_price;
        result.last_set_volume = set_trade_value;  // struct 필드명은 공용 (의미만 거래대금)
        result.last_hold_bars = hold_bars;
        result.last_entry_rsi = entry_rsi;
        result.last_highest_since_buy = highest_since_buy;
        result.last_signal_type = if position == 1 { "holding".into() } else { "idle".into() };

        let _ = buy_index;
        let _ = buy_signal_str;
        let _: Vec<SignalEvent> = result.signal_log.clone();

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

        TradingSignal { signal_type, confidence: None, metadata: HashMap::new() }
    }

    fn parameter_ranges(&self) -> Vec<ParameterRange> {
        // ETH/hour trade value (KRW) 기준 탐색 범위.
        vec![
            ParameterRange { name: "v31_urgent_buy_tv_lo".into(), min: 1.0e10, max: 2.0e11, step: 5.0e9 },
            ParameterRange { name: "v31_urgent_buy_tv_hi".into(), min: 5.0e10, max: 5.0e11, step: 1.0e10 },
            ParameterRange { name: "v31_urgent_buy_tv_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_buy_tv_lo".into(), min: 5.0e9, max: 1.0e11, step: 1.0e9 },
            ParameterRange { name: "v31_buy_tv_hi".into(), min: 1.0e10, max: 2.0e11, step: 5.0e9 },
            ParameterRange { name: "v31_buy_tv_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_buy_price_drop_lo".into(), min: 0.9, max: 1.1, step: 0.005 },
            ParameterRange { name: "v31_buy_price_drop_hi".into(), min: 0.9, max: 1.1, step: 0.005 },
            ParameterRange { name: "v31_buy_price_drop_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_buy_decay_lo".into(), min: 0.01, max: 0.5, step: 0.005 },
            ParameterRange { name: "v31_buy_decay_hi".into(), min: 0.01, max: 0.5, step: 0.005 },
            ParameterRange { name: "v31_buy_decay_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_buy_psy_lo".into(), min: -1.0, max: 1.0, step: 0.02 },
            ParameterRange { name: "v31_buy_psy_hi".into(), min: -1.0, max: 1.0, step: 0.02 },
            ParameterRange { name: "v31_buy_psy_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_buy_wait_lo".into(), min: 10.0, max: 1000.0, step: 10.0 },
            ParameterRange { name: "v31_buy_wait_hi".into(), min: 10.0, max: 1000.0, step: 10.0 },
            ParameterRange { name: "v31_buy_wait_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_stop_loss_lo".into(), min: 0.5, max: 1.0, step: 0.005 },
            ParameterRange { name: "v31_sell_stop_loss_hi".into(), min: 0.5, max: 1.0, step: 0.005 },
            ParameterRange { name: "v31_sell_stop_loss_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_profit_lo".into(), min: 1.0, max: 2.0, step: 0.005 },
            ParameterRange { name: "v31_sell_profit_hi".into(), min: 1.0, max: 2.0, step: 0.005 },
            ParameterRange { name: "v31_sell_profit_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_tv_lo".into(), min: 5.0e9, max: 1.0e11, step: 1.0e9 },
            ParameterRange { name: "v31_sell_tv_hi".into(), min: 1.0e10, max: 5.0e11, step: 5.0e9 },
            ParameterRange { name: "v31_sell_tv_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_decay_lo".into(), min: 0.01, max: 0.5, step: 0.005 },
            ParameterRange { name: "v31_sell_decay_hi".into(), min: 0.01, max: 0.5, step: 0.005 },
            ParameterRange { name: "v31_sell_decay_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_fixed_sl_lo".into(), min: 0.01, max: 0.3, step: 0.005 },
            ParameterRange { name: "v31_sell_fixed_sl_hi".into(), min: 0.01, max: 0.3, step: 0.005 },
            ParameterRange { name: "v31_sell_fixed_sl_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_max_hold_lo".into(), min: 10.0, max: 1000.0, step: 10.0 },
            ParameterRange { name: "v31_sell_max_hold_hi".into(), min: 10.0, max: 1000.0, step: 10.0 },
            ParameterRange { name: "v31_sell_max_hold_pow".into(), min: 0.1, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_fee_rate".into(), min: 0.0, max: 0.005, step: 0.0001 },
            ParameterRange { name: "v31_min_hold_bars".into(), min: 1.0, max: 48.0, step: 1.0 },
            ParameterRange { name: "v31_volume_lookback".into(), min: 5.0, max: 100.0, step: 5.0 },
            // 신규 파라미터화된 4개
            ParameterRange { name: "v31_cutoff_tv_mult".into(), min: 0.5, max: 3.0, step: 0.05 },
            ParameterRange { name: "v31_urgent_sell_tv_mult".into(), min: 1.0, max: 5.0, step: 0.1 },
            ParameterRange { name: "v31_sell_ready_price_rise".into(), min: 0.99, max: 1.05, step: 0.002 },
            ParameterRange { name: "v31_sell_wait_max".into(), min: 24.0, max: 720.0, step: 24.0 },
        ]
    }
}
