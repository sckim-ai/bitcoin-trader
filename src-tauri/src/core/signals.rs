use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalType {
    Buy,
    Sell,
    BuyReady,
    SellReady,
    Hold,
    BuyReadyConfirmed,
    SellReadyConfirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSignal {
    pub signal_type: SignalType,
    pub confidence: Option<f64>,
    pub metadata: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionState {
    pub position: i32,
    pub buy_price: f64,
    pub buy_volume: f64,
    pub buy_psy: f64,
    pub hold_bars: i32,
    pub highest_since_buy: f64,
    pub entry_rsi: f64,
}

impl Default for PositionState {
    fn default() -> Self {
        Self {
            position: 0,
            buy_price: 0.0,
            buy_volume: 0.0,
            buy_psy: 0.0,
            hold_bars: 0,
            highest_since_buy: 0.0,
            entry_rsi: 0.0,
        }
    }
}
