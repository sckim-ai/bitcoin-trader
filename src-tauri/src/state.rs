use crate::strategies::StrategyRegistry;
use rusqlite::Connection;
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
}

impl AppState {
    /// Create an empty AppState with an in-memory DB (for middleware type resolution).
    /// The actual state is provided by the Router.
    pub fn empty() -> Self {
        Self {
            db: Mutex::new(Connection::open_in_memory().expect("in-memory DB")),
            registry: StrategyRegistry::new(),
        }
    }
}
