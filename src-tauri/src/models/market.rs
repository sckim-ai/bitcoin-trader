use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndicatorSet {
    pub sma_10: f64,
    pub sma_25: f64,
    pub sma_60: f64,
    pub rsi: f64,
    pub macd: f64,
    pub macd_signal: f64,
    pub macd_histogram: f64,
    pub bollinger_upper: f64,
    pub bollinger_middle: f64,
    pub bollinger_lower: f64,
    pub atr: f64,
    pub adx: f64,
    pub di_plus: f64,
    pub di_minus: f64,
    pub stoch_k: f64,
    pub stoch_d: f64,
    pub psy_hour: f64,
    pub psy_day: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub candle: Candle,
    pub indicators: IndicatorSet,
}
