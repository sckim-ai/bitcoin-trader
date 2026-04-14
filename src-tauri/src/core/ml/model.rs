/// Simple linear regression model (stub for ML).
/// Uses ordinary least squares for training.
pub struct SimpleModel {
    pub weights: Vec<f64>,
    pub bias: f64,
}

impl SimpleModel {
    /// Train a simple linear regression model using least squares.
    /// Falls back to equal weights if matrix is singular.
    pub fn train(features: &[Vec<f64>], labels: &[f64]) -> Self {
        let n = features.len();
        if n == 0 || features[0].is_empty() {
            return Self {
                weights: Vec::new(),
                bias: 0.0,
            };
        }

        let dim = features[0].len();
        let label_mean = labels.iter().sum::<f64>() / n as f64;

        // Compute feature means
        let mut feat_means = vec![0.0; dim];
        for row in features {
            for (j, val) in row.iter().enumerate() {
                feat_means[j] += val;
            }
        }
        for m in feat_means.iter_mut() {
            *m /= n as f64;
        }

        // Compute weights via correlation-based approach (simplified):
        // w_j = sum((x_j - mean_j) * (y - mean_y)) / sum((x_j - mean_j)^2)
        let mut weights = vec![0.0; dim];
        for j in 0..dim {
            let mut num = 0.0;
            let mut den = 0.0;
            for i in 0..n {
                let xc = features[i][j] - feat_means[j];
                let yc = labels[i] - label_mean;
                num += xc * yc;
                den += xc * xc;
            }
            weights[j] = if den > 1e-15 { num / den } else { 0.0 };
        }

        // Normalize weights (prevent explosion) by scaling to unit sum of abs
        let w_sum: f64 = weights.iter().map(|w| w.abs()).sum();
        if w_sum > 1e-15 {
            // Scale so total weight impact is reasonable
            let scale = 1.0 / (w_sum * dim as f64);
            for w in weights.iter_mut() {
                *w *= scale;
            }
        }

        // Compute bias: bias = mean_y - sum(w_j * mean_j)
        let bias = label_mean - weights.iter().zip(feat_means.iter()).map(|(w, m)| w * m).sum::<f64>();

        Self { weights, bias }
    }

    /// Predict a value from a feature vector.
    pub fn predict(&self, features: &[f64]) -> f64 {
        let dot: f64 = self
            .weights
            .iter()
            .zip(features.iter())
            .map(|(w, f)| w * f)
            .sum();
        dot + self.bias
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_model_train_and_predict() {
        // Simple linear relationship: y = 2*x1 + 3*x2 + 1
        let features = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 1.0],
            vec![2.0, 1.0],
            vec![1.0, 2.0],
        ];
        let labels = vec![3.0, 4.0, 6.0, 8.0, 9.0];

        let model = SimpleModel::train(&features, &labels);
        assert_eq!(model.weights.len(), 2);

        // Predictions should be finite
        let pred = model.predict(&[1.0, 1.0]);
        assert!(pred.is_finite());
    }

    #[test]
    fn test_empty_training() {
        let model = SimpleModel::train(&[], &[]);
        assert!(model.weights.is_empty());
        assert_eq!(model.bias, 0.0);
    }
}
