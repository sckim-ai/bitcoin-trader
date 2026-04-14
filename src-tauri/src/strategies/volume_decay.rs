use crate::core::engine;
use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

pub struct VolumeDecayStrategy;

impl Strategy for VolumeDecayStrategy {
    fn name(&self) -> &str {
        "Volume Decay (V0)"
    }

    fn description(&self) -> &str {
        "Volume spike + decay pattern based buy/sell with risk management"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        engine::run_simulation(data, params)
    }

    fn get_latest_signal(
        &self,
        data: &[MarketData],
        params: &TradingParameters,
        _position: &PositionState,
    ) -> TradingSignal {
        // Run simulation on all data and return signal based on last state
        let result = engine::run_simulation(data, params);
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
            ParameterRange { name: "urgent_buy_volume_threshold".into(), min: 5000.0, max: 100000.0, step: 1000.0 },
            ParameterRange { name: "buy_ready_volume_threshold".into(), min: 1000.0, max: 50000.0, step: 500.0 },
            ParameterRange { name: "buy_confirm_volume_decay_ratio".into(), min: 0.001, max: 0.5, step: 0.01 },
            ParameterRange { name: "buy_wait_max_periods".into(), min: 10.0, max: 500.0, step: 10.0 },
            ParameterRange { name: "buy_confirm_psy_threshold".into(), min: 0.0, max: 100.0, step: 5.0 },
            ParameterRange { name: "urgent_buy_price_drop_ratio".into(), min: 0.001, max: 0.1, step: 0.005 },
            ParameterRange { name: "buy_ready_price_drop_ratio".into(), min: 0.001, max: 0.1, step: 0.005 },
            ParameterRange { name: "urgent_sell_volume_threshold".into(), min: 5000.0, max: 100000.0, step: 1000.0 },
            ParameterRange { name: "sell_ready_volume_threshold".into(), min: 1000.0, max: 50000.0, step: 500.0 },
            ParameterRange { name: "sell_confirm_volume_decay_ratio".into(), min: 0.001, max: 0.5, step: 0.01 },
            ParameterRange { name: "sell_wait_max_periods".into(), min: 10.0, max: 500.0, step: 10.0 },
            ParameterRange { name: "urgent_sell_profit_ratio".into(), min: 0.001, max: 0.1, step: 0.005 },
            ParameterRange { name: "sell_ready_price_rise_ratio".into(), min: 0.001, max: 0.1, step: 0.005 },
            ParameterRange { name: "trailing_stop_pct".into(), min: 0.0, max: 0.2, step: 0.01 },
            ParameterRange { name: "max_hold_periods".into(), min: 0.0, max: 1000.0, step: 10.0 },
            ParameterRange { name: "fee_rate".into(), min: 0.0, max: 0.01, step: 0.0001 },
            ParameterRange { name: "fixed_stop_loss_pct".into(), min: 0.0, max: 0.2, step: 0.01 },
            ParameterRange { name: "fixed_take_profit_pct".into(), min: 0.0, max: 0.5, step: 0.01 },
        ]
    }
}
