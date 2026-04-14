pub mod volume_decay;
pub mod enhanced_volume;
pub mod multi_indicator;
pub mod regime_adaptive;

use crate::core::signals::{PositionState, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use std::collections::HashMap;

pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult;
    fn get_latest_signal(
        &self,
        data: &[MarketData],
        params: &TradingParameters,
        position: &PositionState,
    ) -> TradingSignal;
    fn parameter_ranges(&self) -> Vec<ParameterRange>;
}

pub struct StrategyRegistry {
    strategies: HashMap<String, Box<dyn Strategy>>,
}

impl StrategyRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            strategies: HashMap::new(),
        };
        registry.register("V0", Box::new(volume_decay::VolumeDecayStrategy));
        registry.register("V1", Box::new(enhanced_volume::EnhancedVolumeStrategy));
        registry.register("V2", Box::new(multi_indicator::MultiIndicatorStrategy));
        registry.register("V3", Box::new(regime_adaptive::RegimeAdaptiveStrategy));
        registry
    }

    pub fn register(&mut self, key: &str, strategy: Box<dyn Strategy>) {
        self.strategies.insert(key.to_string(), strategy);
    }

    pub fn get(&self, key: &str) -> Option<&dyn Strategy> {
        self.strategies.get(key).map(|s| s.as_ref())
    }

    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut items: Vec<(&str, &str)> = self
            .strategies
            .iter()
            .map(|(k, v)| (k.as_str(), v.name()))
            .collect();
        items.sort_by_key(|(k, _)| k.to_string());
        items
    }
}

impl Default for StrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}
