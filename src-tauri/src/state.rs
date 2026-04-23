use crate::core::optimizer::Individual;
use crate::strategies::StrategyRegistry;
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// Handle for a running auto-trading session.
pub struct AutoTradingHandle {
    pub cancel_token: Arc<AtomicBool>,
    pub market: String,
    pub strategy_key: String,
}

/// Live state of a running NSGA-II optimization. Serves two purposes:
///   1. Cancel — UI can flip `cancel_token` to stop the loop at the next
///      generation boundary; the partial Pareto front is still returned.
///   2. Continue — after the loop exits (naturally or via cancel), the
///      final population is retained so a "Continue" click reseeds the
///      next run instead of re-randomising.
pub struct OptimizationHandle {
    pub cancel_token: Arc<AtomicBool>,
    pub run_id: Option<i64>,
    pub last_population: Arc<Mutex<Option<Vec<Individual>>>>,
    pub last_generation: Arc<Mutex<usize>>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
    pub auto_trading: Mutex<Option<AutoTradingHandle>>,
    pub optimization: Mutex<Option<OptimizationHandle>>,
}

impl AppState {
    /// Create an empty AppState with an in-memory DB (for middleware type resolution).
    /// The actual state is provided by the Router.
    pub fn empty() -> Self {
        Self {
            db: Mutex::new(Connection::open_in_memory().expect("in-memory DB")),
            registry: StrategyRegistry::new(),
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
        }
    }
}
