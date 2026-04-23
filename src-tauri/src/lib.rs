pub mod models;
pub mod core;
pub mod db;
pub mod migration;
pub mod strategies;
pub mod api;
pub mod auth;
pub mod server;
pub mod notifications;
pub mod services;

pub mod state;

#[cfg(feature = "tauri-app")]
pub mod commands;

#[cfg(feature = "tauri-app")]
mod app {
    use crate::commands::{auth, data, simulation, optimization, trading, migration, notification};
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
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
        };

        let server_state = Arc::new(AppState {
            db: Mutex::new(
                schema::initialize(&db_path).expect("Failed to initialize server database"),
            ),
            registry: StrategyRegistry::new(),
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
        });

        let server_state_clone = server_state.clone();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async {
                    crate::server::start(server_state_clone, 3741).await;
                });
        });

        // Periodic background market-data updater (separate DB connection)
        let updater_db = Arc::new(Mutex::new(
            schema::initialize(&db_path).expect("Failed to initialize updater database"),
        ));
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async move {
                    crate::services::market_updater::run_loop(updater_db).await;
                });
        });

        tauri::Builder::default()
            .manage(app_state)
            .invoke_handler(tauri::generate_handler![
                data::load_csv_data,
                data::backfill_day_psy,
                data::get_candles,
                data::get_data_range,
                data::get_market_data,
                simulation::list_strategies,
                simulation::run_simulation,
                optimization::start_optimization,
                optimization::cancel_optimization,
                optimization::get_optimization_status,
                optimization::list_optimization_runs,
                optimization::get_optimization_run_results,
                optimization::get_optimization_run_history,
                optimization::delete_optimization_run,
                trading::get_current_price,
                trading::get_balance,
                trading::manual_buy,
                trading::manual_sell,
                trading::get_position,
                trading::start_auto_trading,
                trading::stop_auto_trading,
                trading::get_auto_trading_status,
                data::update_market_data,
                data::auto_update_all_markets,
                auth::login,
                auth::register,
                auth::logout,
                auth::list_users,
                auth::delete_user,
                migration::migrate_from_csv,
                notification::save_notification_config,
                notification::test_notification,
            ])
            .run(tauri::generate_context!())
            .expect("error while running tauri application");
    }

    fn dirs_db_path() -> std::path::PathBuf {
        // DB를 사용자 홈 디렉토리 아래에 저장 (src-tauri/ 안에 두면 file watcher 무한루프)
        let mut path = dirs_next::data_local_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        path.push("bitcoin-trader");
        std::fs::create_dir_all(&path).ok();
        path.push("bitcoin_trader.db");
        path
    }
}

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}
