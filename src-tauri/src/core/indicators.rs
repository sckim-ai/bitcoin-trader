use crate::models::market::{Candle, IndicatorSet};
use chrono::NaiveDate;
use std::collections::BTreeMap;

/// Calculate all technical indicators for a slice of candles.
pub fn calculate_all(candles: &[Candle]) -> Vec<IndicatorSet> {
    calculate_all_with_day_psy(candles, None)
}

/// Clean variant (non-legacy): given an optional map of `NaiveDate →
/// day-scale new-PSY value` (KST dates), fill `psy_day` on each hourly bar
/// with the **previous KST date's** closed day-PSY.
///
/// Differences from the legacy C# pipeline (`DataUpdateManager.cs:380-384`):
/// * No silent hour-PSY fallback. Unavailable day-PSY is emitted as
///   `f64::NAN` so strategies can detect warmup/missing data explicitly.
/// * Comparison `NaN < threshold` is always `false`, so buy-conditions
///   relying on negative thresholds naturally skip missing bars without
///   adding explicit NaN checks.
/// * Caller supplies map via `Some(...)`. `None` means "no day-scale data
///   available" and every bar gets `NaN` (used by tests for strategies
///   that don't consume `psy_day`).
pub fn calculate_all_with_day_psy(
    candles: &[Candle],
    day_psy_kst: Option<&BTreeMap<NaiveDate, f64>>,
) -> Vec<IndicatorSet> {
    let n = candles.len();
    if n == 0 {
        return Vec::new();
    }

    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let highs: Vec<f64> = candles.iter().map(|c| c.high).collect();
    let lows: Vec<f64> = candles.iter().map(|c| c.low).collect();

    let sma_10 = calc_sma(&closes, 10);
    let sma_25 = calc_sma(&closes, 25);
    let sma_60 = calc_sma(&closes, 60);
    let rsi = calc_rsi(&closes, 14);
    let (macd, macd_signal, macd_histogram) = calc_macd(&closes, 12, 26, 9);
    let (bb_upper, bb_middle, bb_lower) = calc_bollinger(&closes, 20, 2.0);
    let atr = calc_atr(&highs, &lows, &closes, 14);
    let (adx, di_plus, di_minus) = calc_adx(&highs, &lows, &closes, 14);
    let (stoch_k, stoch_d) = calc_stochastic(&highs, &lows, &closes, 14, 3);
    let psy_hour = calc_psy(&closes, 10);

    let empty_map: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let effective_map = day_psy_kst.unwrap_or(&empty_map);
    let psy_day: Vec<f64> = candles
        .iter()
        .map(|c| {
            let kst = c.timestamp + chrono::Duration::hours(9);
            let prev_date = kst.date_naive() - chrono::Duration::days(1);
            effective_map
                .get(&prev_date)
                .copied()
                .filter(|v| *v != 0.0)
                .unwrap_or(f64::NAN)
        })
        .collect();

    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        result.push(IndicatorSet {
            sma_10: sma_10[i],
            sma_25: sma_25[i],
            sma_60: sma_60[i],
            rsi: rsi[i],
            macd: macd[i],
            macd_signal: macd_signal[i],
            macd_histogram: macd_histogram[i],
            bollinger_upper: bb_upper[i],
            bollinger_middle: bb_middle[i],
            bollinger_lower: bb_lower[i],
            atr: atr[i],
            adx: adx[i],
            di_plus: di_plus[i],
            di_minus: di_minus[i],
            stoch_k: stoch_k[i],
            stoch_d: stoch_d[i],
            psy_hour: psy_hour[i],
            psy_day: psy_day[i],
        });
    }
    result
}

/// Phase 1: just recalculate from scratch.
pub fn calculate_incremental(candles: &[Candle], indicators: &mut Vec<IndicatorSet>, _start_index: usize) {
    let recalculated = calculate_all(candles);
    indicators.clear();
    indicators.extend(recalculated);
}

// ─── SMA ───

fn calc_sma(data: &[f64], period: usize) -> Vec<f64> {
    let n = data.len();
    let mut out = vec![0.0; n];
    if n < period {
        return out;
    }
    let mut sum: f64 = data[..period].iter().sum();
    out[period - 1] = sum / period as f64;
    for i in period..n {
        sum += data[i] - data[i - period];
        out[i] = sum / period as f64;
    }
    out
}

// ─── RSI (Wilder's smoothing) ───

fn calc_rsi(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    let mut out = vec![0.0; n];
    if n <= period {
        return out;
    }

    // Seed: average gain/loss over first `period` changes (bars 1..=period)
    let mut avg_gain = 0.0;
    let mut avg_loss = 0.0;
    for i in 1..=period {
        let change = closes[i] - closes[i - 1];
        if change > 0.0 {
            avg_gain += change;
        } else {
            avg_loss += -change;
        }
    }
    avg_gain /= period as f64;
    avg_loss /= period as f64;

    out[period] = if avg_loss == 0.0 {
        100.0
    } else {
        100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
    };

    // Wilder's smoothing
    for i in (period + 1)..n {
        let change = closes[i] - closes[i - 1];
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        avg_gain = (avg_gain * (period as f64 - 1.0) + gain) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + loss) / period as f64;
        out[i] = if avg_loss == 0.0 {
            100.0
        } else {
            100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
        };
    }
    out
}

// ─── MACD ───

fn calc_macd(closes: &[f64], fast: usize, slow: usize, signal_period: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = closes.len();
    let mut macd_line = vec![0.0; n];
    let mut signal_line = vec![0.0; n];
    let mut histogram = vec![0.0; n];

    if n < slow {
        return (macd_line, signal_line, histogram);
    }

    // Build fast EMA (seed = SMA of first `fast` bars)
    let fast_ema = calc_ema(closes, fast);
    // Build slow EMA (seed = SMA of first `slow` bars)
    let slow_ema = calc_ema(closes, slow);

    // MACD line valid from bar slow-1
    for i in (slow - 1)..n {
        macd_line[i] = fast_ema[i] - slow_ema[i];
    }

    // Signal line = EMA9 of MACD, starting from bar slow-1
    // We need signal_period valid MACD values. First valid MACD is at slow-1.
    // Signal seed = mean of MACD[slow-1 .. slow-1+signal_period-1]
    let signal_start = slow - 1 + signal_period - 1; // bar 33 for default params
    if n > signal_start {
        let mut seed_sum = 0.0;
        for j in (slow - 1)..=(signal_start) {
            seed_sum += macd_line[j];
        }
        signal_line[signal_start] = seed_sum / signal_period as f64;
        histogram[signal_start] = macd_line[signal_start] - signal_line[signal_start];

        let multiplier = 2.0 / (signal_period as f64 + 1.0);
        for i in (signal_start + 1)..n {
            signal_line[i] = (macd_line[i] - signal_line[i - 1]) * multiplier + signal_line[i - 1];
            histogram[i] = macd_line[i] - signal_line[i];
        }
    }

    (macd_line, signal_line, histogram)
}

fn calc_ema(data: &[f64], period: usize) -> Vec<f64> {
    let n = data.len();
    let mut out = vec![0.0; n];
    if n < period {
        return out;
    }
    // Seed: SMA of first `period` bars
    let seed: f64 = data[..period].iter().sum::<f64>() / period as f64;
    out[period - 1] = seed;
    let multiplier = 2.0 / (period as f64 + 1.0);
    for i in period..n {
        out[i] = (data[i] - out[i - 1]) * multiplier + out[i - 1];
    }
    out
}

// ─── Bollinger Bands (population variance) ───

fn calc_bollinger(closes: &[f64], period: usize, num_std: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = closes.len();
    let mut upper = vec![0.0; n];
    let mut middle = vec![0.0; n];
    let mut lower = vec![0.0; n];

    if n < period {
        return (upper, middle, lower);
    }

    for i in (period - 1)..n {
        let window = &closes[i + 1 - period..=i];
        let mean: f64 = window.iter().sum::<f64>() / period as f64;
        let mean_sq: f64 = window.iter().map(|x| x * x).sum::<f64>() / period as f64;
        let variance = mean_sq - mean * mean;
        let std = variance.max(0.0).sqrt();
        middle[i] = mean;
        upper[i] = mean + num_std * std;
        lower[i] = mean - num_std * std;
    }
    (upper, middle, lower)
}

// ─── ATR (Wilder's smoothing) ───

fn calc_atr(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> Vec<f64> {
    let n = highs.len();
    let mut out = vec![0.0; n];
    if n <= period {
        return out;
    }

    // TR values (TR[0] = H-L, TR[i] uses prev close)
    let mut tr = vec![0.0; n];
    tr[0] = highs[0] - lows[0];
    for i in 1..n {
        let hl = highs[i] - lows[i];
        let hc = (highs[i] - closes[i - 1]).abs();
        let lc = (lows[i] - closes[i - 1]).abs();
        tr[i] = hl.max(hc).max(lc);
    }

    // First ATR = mean of TR[1..=period] (period values starting from bar 1)
    let first_atr: f64 = tr[1..=period].iter().sum::<f64>() / period as f64;
    out[period] = first_atr;

    // Wilder's smoothing
    for i in (period + 1)..n {
        out[i] = (out[i - 1] * (period as f64 - 1.0) + tr[i]) / period as f64;
    }
    out
}

// ─── ADX (Wilder's smoothing) ───

fn calc_adx(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = highs.len();
    let mut adx_out = vec![0.0; n];
    let mut di_plus_out = vec![0.0; n];
    let mut di_minus_out = vec![0.0; n];

    if n <= 2 * period {
        return (adx_out, di_plus_out, di_minus_out);
    }

    // +DM, -DM, TR arrays
    let mut plus_dm = vec![0.0; n];
    let mut minus_dm = vec![0.0; n];
    let mut tr = vec![0.0; n];

    tr[0] = highs[0] - lows[0];
    for i in 1..n {
        let up_move = highs[i] - highs[i - 1];
        let down_move = lows[i - 1] - lows[i];
        plus_dm[i] = if up_move > down_move && up_move > 0.0 { up_move } else { 0.0 };
        minus_dm[i] = if down_move > up_move && down_move > 0.0 { down_move } else { 0.0 };

        let hl = highs[i] - lows[i];
        let hc = (highs[i] - closes[i - 1]).abs();
        let lc = (lows[i] - closes[i - 1]).abs();
        tr[i] = hl.max(hc).max(lc);
    }

    // First smoothed values = sum of first `period` values (bars 1..=period)
    let mut sm_plus_dm: f64 = plus_dm[1..=period].iter().sum();
    let mut sm_minus_dm: f64 = minus_dm[1..=period].iter().sum();
    let mut sm_tr: f64 = tr[1..=period].iter().sum();

    // DI at bar `period`
    let compute_di = |dm: f64, tr: f64| -> f64 {
        if tr == 0.0 { 0.0 } else { 100.0 * dm / tr }
    };

    di_plus_out[period] = compute_di(sm_plus_dm, sm_tr);
    di_minus_out[period] = compute_di(sm_minus_dm, sm_tr);

    // DX values for ADX seed
    let mut dx_values: Vec<f64> = Vec::new();
    let di_sum = di_plus_out[period] + di_minus_out[period];
    dx_values.push(if di_sum == 0.0 { 0.0 } else { 100.0 * (di_plus_out[period] - di_minus_out[period]).abs() / di_sum });

    // Wilder's smoothing for DM/TR from bar period+1 to 2*period
    for i in (period + 1)..n {
        sm_plus_dm = sm_plus_dm - sm_plus_dm / period as f64 + plus_dm[i];
        sm_minus_dm = sm_minus_dm - sm_minus_dm / period as f64 + minus_dm[i];
        sm_tr = sm_tr - sm_tr / period as f64 + tr[i];

        di_plus_out[i] = compute_di(sm_plus_dm, sm_tr);
        di_minus_out[i] = compute_di(sm_minus_dm, sm_tr);

        let di_sum = di_plus_out[i] + di_minus_out[i];
        let dx = if di_sum == 0.0 { 0.0 } else { 100.0 * (di_plus_out[i] - di_minus_out[i]).abs() / di_sum };

        if i < 2 * period {
            dx_values.push(dx);
        } else if i == 2 * period {
            dx_values.push(dx);
            // First ADX = mean of first `period` DX values
            let first_adx: f64 = dx_values.iter().sum::<f64>() / dx_values.len() as f64;
            adx_out[i] = first_adx;
        } else {
            // Wilder's smoothing for ADX
            adx_out[i] = (adx_out[i - 1] * (period as f64 - 1.0) + dx) / period as f64;
        }
    }

    (adx_out, di_plus_out, di_minus_out)
}

// ─── Stochastic (%K, %D) ───

fn calc_stochastic(highs: &[f64], lows: &[f64], closes: &[f64], k_period: usize, d_period: usize) -> (Vec<f64>, Vec<f64>) {
    let n = closes.len();
    let mut k_out = vec![0.0; n];
    let mut d_out = vec![0.0; n];

    if n < k_period {
        return (k_out, d_out);
    }

    // Calculate %K
    for i in (k_period - 1)..n {
        let window_high = highs[i + 1 - k_period..=i].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let window_low = lows[i + 1 - k_period..=i].iter().cloned().fold(f64::INFINITY, f64::min);
        let range = window_high - window_low;
        k_out[i] = if range == 0.0 { 50.0 } else { 100.0 * (closes[i] - window_low) / range };
    }

    // %D = SMA of %K over d_period
    let d_start = k_period - 1 + d_period - 1;
    if n > d_start {
        for i in d_start..n {
            let sum: f64 = k_out[i + 1 - d_period..=i].iter().sum();
            d_out[i] = sum / d_period as f64;
        }
    }

    (k_out, d_out)
}

// ─── PSY ───

/// Legacy `NewPsy` — verbatim port of C# `DataUpdateManager.cs:294-318`
/// and `UpbitApiClient.cs:856-883`:
///   NewPsy = (upD × (upC / t) - downD × (downC / t)) / period
/// where upD/downD = up/down bar counts, upC/downC = sum of up/down magnitudes,
/// t = total magnitude. Range is `[-1, +1]` and the metric is **weighted by
/// price-move size** — a few large-move bars dominate many small-move bars.
/// Period is 10 for both hourly and daily PSY in legacy.
/// Build a day-PSY lookup table keyed by KST date.
///
/// `day_candles` must be sorted ascending by timestamp. For each candle we
/// compute the legacy NewPsy (period 10) on the close series and index the
/// result under the KST date of that candle. Warmup bars (first 10) remain
/// at 0.0 — callers treat 0.0 as "no day-scale data", same as a missing key.
pub fn build_day_psy_map(day_candles: &[Candle]) -> BTreeMap<NaiveDate, f64> {
    let mut map = BTreeMap::new();
    if day_candles.is_empty() {
        return map;
    }
    let closes: Vec<f64> = day_candles.iter().map(|c| c.close).collect();
    let psy_values = calc_psy(&closes, 10);
    for (c, &psy) in day_candles.iter().zip(psy_values.iter()) {
        let kst_date = (c.timestamp + chrono::Duration::hours(9)).date_naive();
        map.insert(kst_date, psy);
    }
    map
}

pub fn calc_psy(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    let mut out = vec![0.0; n];
    if n <= period {
        return out;
    }

    for i in period..n {
        let (mut up_d, mut down_d) = (0.0f64, 0.0f64);
        let (mut up_c, mut down_c) = (0.0f64, 0.0f64);
        for j in (i + 1 - period)..=i {
            let ch = closes[j] - closes[j - 1];
            if ch > 0.0 {
                up_d += 1.0;
                up_c += ch;
            } else if ch < 0.0 {
                down_d += 1.0;
                down_c -= ch;
            }
        }
        let t = up_c + down_c;
        if t > 0.0 {
            out[i] = (up_d * (up_c / t) - down_d * (down_c / t)) / period as f64;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sma_basic() {
        let data: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let sma = calc_sma(&data, 10);
        assert!((sma[9] - 5.5).abs() < 1e-10);
        assert!((sma[19] - 15.5).abs() < 1e-10);
    }

    #[test]
    fn test_sma_constant() {
        let data = vec![100.0; 30];
        let sma = calc_sma(&data, 25);
        assert!((sma[24] - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_ema_seed() {
        let data: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let ema = calc_ema(&data, 10);
        // Seed = SMA of first 10 = 5.5
        assert!((ema[9] - 5.5).abs() < 1e-10);
    }
}
