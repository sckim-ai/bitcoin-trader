use crate::strategies::StrategyRegistry;
use rusqlite::Connection;
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
}
