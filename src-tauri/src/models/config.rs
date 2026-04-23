use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterRange {
    pub name: String,
    pub min: f64,
    pub max: f64,
    /// UI hint only — HTML <input step> 화살표 증감 단위로 SimulationPage에서 사용.
    /// NSGA-II 옵티마이저는 [min, max] 연속 공간에서 샘플링하므로 step 값을 무시한다.
    pub step: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    pub population_size: usize,
    pub generations: usize,
    pub crossover_rate: f64,
    pub mutation_rate: f64,
    pub objectives: Vec<String>,
    pub min_win_rate: f64,
    pub min_trades: usize,
    pub min_return: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            population_size: 50,
            generations: 100,
            crossover_rate: 0.9,
            mutation_rate: 0.1,
            objectives: Vec::new(),
            min_win_rate: 0.0,
            min_trades: 0,
            min_return: 0.0,
        }
    }
}
