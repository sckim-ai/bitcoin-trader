pub mod enhanced_adaptive;
pub mod regime_adaptive;
pub mod regime_adaptive_v31;

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
        registry.register("V3", Box::new(regime_adaptive::RegimeAdaptiveStrategy));
        registry.register("V3.1", Box::new(regime_adaptive_v31::RegimeAdaptiveV31Strategy));
        registry.register("V5", Box::new(enhanced_adaptive::EnhancedAdaptiveStrategy));
        registry
    }

    pub fn register(&mut self, key: &str, strategy: Box<dyn Strategy>) {
        self.strategies.insert(key.to_string(), strategy);
    }

    pub fn get(&self, key: &str) -> Option<&dyn Strategy> {
        self.strategies.get(key).map(|s| s.as_ref())
    }

    pub fn list(&self) -> Vec<(&str, &str, Vec<ParameterRange>)> {
        let mut items: Vec<(&str, &str, Vec<ParameterRange>)> = self
            .strategies
            .iter()
            .map(|(k, v)| (k.as_str(), v.name(), v.parameter_ranges()))
            .collect();
        items.sort_by_key(|(k, _, _)| k.to_string());
        items
    }
}

impl Default for StrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}
