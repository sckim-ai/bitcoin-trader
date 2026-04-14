use crate::core::ml::features;
use crate::core::ml::model::SimpleModel;
use crate::models::market::MarketData;

/// Walk-forward trainer: trains on a rolling window and predicts forward returns.
pub struct WalkForwardTrainer {
    pub train_window: usize,
    pub retrain_interval: usize,
}

/// Prediction result for a single bar.
#[derive(Debug, Clone)]
pub struct Prediction {
    pub predicted_return: f64,
    pub profit_probability: f64,
}

impl WalkForwardTrainer {
    /// Run walk-forward training and return predictions for each bar (from MIN_LOOKBACK onward).
    /// Returns a Vec of Option<Prediction> indexed by bar position.
    pub fn train_and_predict(&self, data: &[MarketData]) -> Vec<Option<Prediction>> {
        let n = data.len();
        let mut predictions: Vec<Option<Prediction>> = vec![None; n];

        if n < features::MIN_LOOKBACK + self.train_window + 1 {
            return predictions;
        }

        let mut current_model: Option<SimpleModel> = None;
        let mut bars_since_train: usize = self.retrain_interval; // Force initial train

        let start = features::MIN_LOOKBACK;

        for i in start..n {
            // Retrain if needed
            if bars_since_train >= self.retrain_interval {
                if let Some(model) = self.build_model(data, i) {
                    current_model = Some(model);
                    bars_since_train = 0;
                }
            }
            bars_since_train += 1;

            // Predict using current model
            if let (Some(model), Some(fv)) = (&current_model, features::extract(data, i)) {
                let predicted_return = model.predict(&fv.features);
                // Estimate profit probability: sigmoid-like mapping of predicted return
                let profit_probability = 1.0 / (1.0 + (-predicted_return * 200.0).exp());
                predictions[i] = Some(Prediction {
                    predicted_return,
                    profit_probability,
                });
            }
        }

        predictions
    }

    /// Build a model using training data up to (but not including) `end_index`.
    fn build_model(&self, data: &[MarketData], end_index: usize) -> Option<SimpleModel> {
        let train_start = if end_index > self.train_window {
            end_index - self.train_window
        } else {
            features::MIN_LOOKBACK
        };

        if train_start < features::MIN_LOOKBACK || end_index <= train_start + 10 {
            return None;
        }

        let mut train_features = Vec::new();
        let mut train_labels = Vec::new();

        // For each bar in training window, extract features and compute forward return as label
        for i in train_start..end_index.saturating_sub(1) {
            if let Some(fv) = features::extract(data, i) {
                let forward_return = (data[i + 1].candle.close - data[i].candle.close)
                    / data[i].candle.close;
                train_features.push(fv.features);
                train_labels.push(forward_return);
            }
        }

        if train_features.len() < 10 {
            return None;
        }

        Some(SimpleModel::train(&train_features, &train_labels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::{Candle, IndicatorSet, MarketData};
    use chrono::{TimeZone, Utc};

    fn make_trending_data(n: usize) -> Vec<MarketData> {
        (0..n)
            .map(|i| {
                let price = 50000.0 + (i as f64) * 5.0;
                MarketData {
                    candle: Candle {
                        timestamp: Utc
                            .with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0)
                            .unwrap(),
                        open: price,
                        high: price * 1.01,
                        low: price * 0.99,
                        close: price,
                        volume: 5000.0 + (i % 50) as f64 * 200.0,
                    },
                    indicators: IndicatorSet {
                        rsi: 50.0 + (i % 30) as f64,
                        atr: price * 0.01,
                        bollinger_upper: price * 1.02,
                        bollinger_lower: price * 0.98,
                        bollinger_middle: price,
                        stoch_k: 50.0,
                        adx: 25.0,
                        di_plus: 30.0,
                        di_minus: 20.0,
                        psy_hour: 50.0,
                        psy_day: 50.0,
                        sma_10: price * 0.999,
                        ..Default::default()
                    },
                }
            })
            .collect()
    }

    #[test]
    fn test_trainer_produces_predictions() {
        let data = make_trending_data(500);
        let trainer = WalkForwardTrainer {
            train_window: 200,
            retrain_interval: 100,
        };
        let predictions = trainer.train_and_predict(&data);
        assert_eq!(predictions.len(), 500);

        // Should have some predictions after warmup
        let pred_count = predictions.iter().filter(|p| p.is_some()).count();
        assert!(pred_count > 0, "Should have at least some predictions");
    }

    #[test]
    fn test_trainer_insufficient_data() {
        let data = make_trending_data(50);
        let trainer = WalkForwardTrainer {
            train_window: 200,
            retrain_interval: 100,
        };
        let predictions = trainer.train_and_predict(&data);
        // All None because not enough data
        assert!(predictions.iter().all(|p| p.is_none()));
    }
}
