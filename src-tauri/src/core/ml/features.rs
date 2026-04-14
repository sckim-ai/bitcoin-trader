use crate::models::market::MarketData;

/// Feature vector extracted from market data at a given index.
pub struct FeatureVector {
    pub features: Vec<f64>,
}

/// Minimum lookback bars required for feature extraction.
pub const MIN_LOOKBACK: usize = 168;

/// Number of features extracted.
pub const FEATURE_COUNT: usize = 22;

/// Extract a feature vector from MarketData at the given index.
/// Returns None if insufficient lookback (< 168 bars).
pub fn extract(data: &[MarketData], index: usize) -> Option<FeatureVector> {
    if index < MIN_LOOKBACK || index >= data.len() {
        return None;
    }

    let mut features = Vec::with_capacity(FEATURE_COUNT);
    let close = data[index].candle.close;

    // === Price momentum (6 features) ===
    for &lookback in &[1, 4, 12, 24, 48, 168] {
        let prev_close = data[index - lookback].candle.close;
        let ret = if prev_close > 0.0 {
            (close - prev_close) / prev_close
        } else {
            0.0
        };
        features.push(ret);
    }

    // === Volatility (3 features) ===
    // Rolling std of returns over 24 bars
    let returns_24: Vec<f64> = (1..=24)
        .map(|j| {
            let c = data[index - j + 1].candle.close;
            let p = data[index - j].candle.close;
            if p > 0.0 { (c - p) / p } else { 0.0 }
        })
        .collect();
    let mean_24 = returns_24.iter().sum::<f64>() / 24.0;
    let std_24 = (returns_24.iter().map(|r| (r - mean_24).powi(2)).sum::<f64>() / 24.0).sqrt();
    features.push(std_24);

    // ATR ratio (current ATR / close)
    let atr_ratio = if close > 0.0 {
        data[index].indicators.atr / close
    } else {
        0.0
    };
    features.push(atr_ratio);

    // High-low range ratio
    let high = data[index].candle.high;
    let low = data[index].candle.low;
    let hl_range = if close > 0.0 {
        (high - low) / close
    } else {
        0.0
    };
    features.push(hl_range);

    // === Volume (4 features) ===
    let vol = data[index].candle.volume;

    // Volume ratio vs 20-bar average
    let vol_sum_20: f64 = (0..20).map(|j| data[index - j].candle.volume).sum();
    let vol_avg_20 = vol_sum_20 / 20.0;
    let vol_ratio = if vol_avg_20 > 0.0 { vol / vol_avg_20 } else { 1.0 };
    features.push(vol_ratio);

    // Volume trend (linear slope over 20 bars, normalized)
    let vol_first_10: f64 = (10..20).map(|j| data[index - j].candle.volume).sum::<f64>() / 10.0;
    let vol_last_10: f64 = (0..10).map(|j| data[index - j].candle.volume).sum::<f64>() / 10.0;
    let vol_trend = if vol_first_10 > 0.0 {
        (vol_last_10 - vol_first_10) / vol_first_10
    } else {
        0.0
    };
    features.push(vol_trend);

    // Volume change from previous bar
    let prev_vol = data[index - 1].candle.volume;
    let vol_change = if prev_vol > 0.0 {
        (vol - prev_vol) / prev_vol
    } else {
        0.0
    };
    features.push(vol_change);

    // Volume std over 20 bars (normalized by mean)
    let vols_20: Vec<f64> = (0..20).map(|j| data[index - j].candle.volume).collect();
    let vol_std = (vols_20.iter().map(|v| (v - vol_avg_20).powi(2)).sum::<f64>() / 20.0).sqrt();
    let vol_std_norm = if vol_avg_20 > 0.0 { vol_std / vol_avg_20 } else { 0.0 };
    features.push(vol_std_norm);

    // === Technical indicators (8 features) ===
    let ind = &data[index].indicators;

    // RSI (normalized 0-1)
    features.push(ind.rsi / 100.0);

    // MACD histogram (normalized by close)
    features.push(if close > 0.0 { ind.macd_histogram / close } else { 0.0 });

    // Bollinger %B: (close - lower) / (upper - lower)
    let bb_width = ind.bollinger_upper - ind.bollinger_lower;
    let bb_pct_b = if bb_width > 0.0 {
        (close - ind.bollinger_lower) / bb_width
    } else {
        0.5
    };
    features.push(bb_pct_b);

    // Stochastic K (normalized 0-1)
    features.push(ind.stoch_k / 100.0);

    // ADX (normalized 0-1)
    features.push(ind.adx / 100.0);

    // DI+ - DI- (trend direction)
    features.push((ind.di_plus - ind.di_minus) / 100.0);

    // PSY hour (normalized 0-1)
    features.push(ind.psy_hour / 100.0);

    // PSY day (normalized 0-1)
    features.push(ind.psy_day / 100.0);

    // === SMA cross features (4 features) ===
    // Price vs SMA10
    features.push(if ind.sma_10 > 0.0 { (close - ind.sma_10) / ind.sma_10 } else { 0.0 });

    debug_assert_eq!(features.len(), FEATURE_COUNT);
    Some(FeatureVector { features })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::{Candle, IndicatorSet, MarketData};
    use chrono::{TimeZone, Utc};

    fn make_data(n: usize) -> Vec<MarketData> {
        (0..n)
            .map(|i| {
                let price = 50000.0 + (i as f64) * 10.0;
                MarketData {
                    candle: Candle {
                        timestamp: Utc.with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0).unwrap(),
                        open: price,
                        high: price * 1.01,
                        low: price * 0.99,
                        close: price,
                        volume: 5000.0 + (i as f64) * 100.0,
                    },
                    indicators: IndicatorSet {
                        rsi: 50.0,
                        macd_histogram: 10.0,
                        bollinger_upper: price * 1.02,
                        bollinger_lower: price * 0.98,
                        bollinger_middle: price,
                        atr: price * 0.01,
                        adx: 25.0,
                        di_plus: 30.0,
                        di_minus: 20.0,
                        stoch_k: 60.0,
                        stoch_d: 55.0,
                        psy_hour: 50.0,
                        psy_day: 50.0,
                        sma_10: price * 0.999,
                        sma_25: price * 0.998,
                        sma_60: price * 0.995,
                        macd: 5.0,
                        macd_signal: 3.0,
                    },
                }
            })
            .collect()
    }

    #[test]
    fn test_extract_returns_none_for_insufficient_data() {
        let data = make_data(100);
        assert!(extract(&data, 50).is_none());
        assert!(extract(&data, 167).is_none());
    }

    #[test]
    fn test_extract_returns_correct_feature_count() {
        let data = make_data(200);
        let fv = extract(&data, 180).unwrap();
        assert_eq!(fv.features.len(), FEATURE_COUNT);
    }

    #[test]
    fn test_features_are_finite() {
        let data = make_data(200);
        let fv = extract(&data, 180).unwrap();
        for (i, f) in fv.features.iter().enumerate() {
            assert!(f.is_finite(), "Feature {} is not finite: {}", i, f);
        }
    }
}
