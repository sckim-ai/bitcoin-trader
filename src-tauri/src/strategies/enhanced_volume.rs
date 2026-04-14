use crate::core::engine;
use crate::core::signals::{PositionState, SignalType, TradingSignal};
use crate::models::config::ParameterRange;
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use std::collections::HashMap;

use super::Strategy;

/// V1 Enhanced Volume: adapts volume thresholds using adaptive window + ATR trailing stop.
pub struct EnhancedVolumeStrategy;

impl EnhancedVolumeStrategy {
    /// Adjust parameters with adaptive volume window and ATR trailing.
    fn adapt_params(&self, data: &[MarketData], params: &TradingParameters) -> TradingParameters {
        let mut adapted = params.clone();

        // Adaptive volume window: compute average volume over recent window
        let window = params.v1_adaptive_volume_window.max(1) as usize;
        if data.len() >= window {
            let recent = &data[data.len() - window..];
            let avg_vol: f64 = recent.iter().map(|d| d.candle.volume).sum::<f64>() / window as f64;

            // Scale thresholds relative to average volume
            if avg_vol > 0.0 {
                let ratio = avg_vol / 5000.0; // normalize to baseline
                adapted.urgent_buy_volume_threshold = params.urgent_buy_volume_threshold * ratio;
                adapted.buy_ready_volume_threshold = params.buy_ready_volume_threshold * ratio;
                adapted.urgent_sell_volume_threshold = params.urgent_sell_volume_threshold * ratio;
                adapted.sell_ready_volume_threshold = params.sell_ready_volume_threshold * ratio;
            }
        }

        // ATR-based trailing stop
        if data.len() > 14 {
            let last = &data[data.len() - 1];
            if last.indicators.atr > 0.0 && last.candle.close > 0.0 {
                let atr_pct = last.indicators.atr / last.candle.close;
                adapted.trailing_stop_pct = atr_pct * params.v1_atr_trailing_multiplier;
            }
        }

        adapted
    }
}

impl Strategy for EnhancedVolumeStrategy {
    fn name(&self) -> &str {
        "Enhanced Volume (V1)"
    }

    fn description(&self) -> &str {
        "Volume decay with adaptive volume window and ATR-based trailing stop"
    }

    fn run_simulation(&self, data: &[MarketData], params: &TradingParameters) -> SimulationResult {
        let adapted = self.adapt_params(data, params);
        engine::run_simulation(data, &adapted)
    }

    fn get_latest_signal(
        &self,
        data: &[MarketData],
        params: &TradingParameters,
        _position: &PositionState,
    ) -> TradingSignal {
        let adapted = self.adapt_params(data, params);
        let result = engine::run_simulation(data, &adapted);
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
        // V1 adds adaptive params on top of V0 ranges
        vec![
            ParameterRange { name: "urgent_buy_volume_threshold".into(), min: 5000.0, max: 100000.0, step: 1000.0 },
            ParameterRange { name: "buy_ready_volume_threshold".into(), min: 1000.0, max: 50000.0, step: 500.0 },
            ParameterRange { name: "buy_confirm_volume_decay_ratio".into(), min: 0.001, max: 0.5, step: 0.01 },
            ParameterRange { name: "buy_wait_max_periods".into(), min: 10.0, max: 500.0, step: 10.0 },
            ParameterRange { name: "urgent_buy_price_drop_ratio".into(), min: 0.001, max: 0.1, step: 0.005 },
            ParameterRange { name: "buy_ready_price_drop_ratio".into(), min: 0.001, max: 0.1, step: 0.005 },
            ParameterRange { name: "trailing_stop_pct".into(), min: 0.0, max: 0.2, step: 0.01 },
            ParameterRange { name: "fee_rate".into(), min: 0.0, max: 0.01, step: 0.0001 },
            ParameterRange { name: "fixed_stop_loss_pct".into(), min: 0.0, max: 0.2, step: 0.01 },
            ParameterRange { name: "fixed_take_profit_pct".into(), min: 0.0, max: 0.5, step: 0.01 },
            ParameterRange { name: "v1_adaptive_volume_window".into(), min: 5.0, max: 100.0, step: 5.0 },
            ParameterRange { name: "v1_atr_trailing_multiplier".into(), min: 0.5, max: 5.0, step: 0.5 },
        ]
    }
}
