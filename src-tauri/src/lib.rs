pub mod models;
pub mod core;
pub mod db;
pub mod migration;
pub mod strategies;
pub mod api;
pub mod auth;
pub mod server;

pub mod state;

#[cfg(feature = "tauri-app")]
pub mod commands;

#[cfg(feature = "tauri-app")]
mod app {
    use crate::commands::{auth, data, simulation, optimization, trading};
    use crate::db::schema;
    use crate::state::AppState;
    use crate::strategies::StrategyRegistry;
    use std::sync::{Arc, Mutex};

    pub fn run() {
        let db_path = dirs_db_path();
        let conn = schema::initialize(&db_path).expect("Failed to initialize database");

        let app_state = AppState {
            db: Mutex::new(conn),
            registry: StrategyRegistry::new(),
        };

        let server_state = Arc::new(AppState {
            db: Mutex::new(
                schema::initialize(&db_path).expect("Failed to initialize server database"),
            ),
            registry: StrategyRegistry::new(),
        });

        let server_state_clone = server_state.clone();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async {
                    crate::server::start(server_state_clone, 3741).await;
                });
        });

        tauri::Builder::default()
            .manage(app_state)
            .invoke_handler(tauri::generate_handler![
                data::load_csv_data,
                data::get_candles,
                data::get_market_data,
                simulation::list_strategies,
                simulation::run_simulation,
                optimization::start_optimization,
                trading::get_current_price,
                trading::get_balance,
                trading::manual_buy,
                trading::manual_sell,
                trading::get_position,
                auth::login,
                auth::register,
                auth::logout,
                auth::list_users,
                auth::delete_user,
            ])
            .run(tauri::generate_context!())
            .expect("error while running tauri application");
    }

    fn dirs_db_path() -> std::path::PathBuf {
        let mut path = std::env::current_dir().unwrap_or_default();
        path.push("bitcoin_trader.db");
        path
    }
}

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}
