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
    use crate::commands::{auth, data, simulation, optimization, trading, migration, notification, live_trading, upbit_keys, history};
    use crate::db::schema;
    use crate::state::AppState;
    use crate::strategies::StrategyRegistry;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    pub fn run() {
        let db_path = dirs_db_path();
        let conn = schema::initialize(&db_path).expect("Failed to initialize database");

        // Phase 2: start the Upbit ticker broker and share its handle.
        // Tauri Emitter and Axum SSE both subscribe to it.
        let broker = crate::services::tick_broker::start(vec!["KRW-ETH".to_string()]);

        let app_state = AppState {
            db: Mutex::new(conn),
            registry: StrategyRegistry::new(),
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
            paper_session_ids: Mutex::new(std::collections::HashMap::new()),
            tick_broker: Some(broker.clone()),
        };

        let server_state = Arc::new(AppState {
            db: Mutex::new(
                schema::initialize(&db_path).expect("Failed to initialize server database"),
            ),
            registry: StrategyRegistry::new(),
            auto_trading: Mutex::new(None),
            optimization: Mutex::new(None),
            paper_session_ids: Mutex::new(std::collections::HashMap::new()),
            tick_broker: Some(broker.clone()),
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

        // Live trading scheduler (separate DB connection).
        let scheduler_db = Arc::new(Mutex::new(
            schema::initialize(&db_path).expect("Failed to initialize scheduler database"),
        ));
        let scheduler_cancel = Arc::new(AtomicBool::new(false));

        tauri::Builder::default()
            .manage(app_state)
            .setup(move |app| {
                let app_handle = app.handle().clone();
                let db_clone = scheduler_db.clone();
                let cancel_clone = scheduler_cancel.clone();
                std::thread::spawn(move || {
                    tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(async move {
                            crate::services::live_scheduler::run_loop(app_handle, db_clone, cancel_clone).await;
                        });
                });

                // Phase 2: re-broadcast ticker events to the frontend via Tauri IPC.
                let tick_handle = app.handle().clone();
                let mut tick_rx = broker.subscribe();
                tauri::async_runtime::spawn(async move {
                    use tauri::Emitter;
                    loop {
                        match tick_rx.recv().await {
                            Ok(t) => { let _ = tick_handle.emit("market:tick", &t); }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                // fell behind — next recv will yield the latest
                                continue;
                            }
                            Err(_) => break,  // channel closed
                        }
                    }
                });

                // Live cycle log stream → frontend "live:log" event. Mirrors the
                // tick broker pattern. Lines come from `live_log!` / file_logger
                // and reach LiveTradingPage's LiveLogPanel in real time.
                let log_handle = app.handle().clone();
                let mut log_rx = crate::core::file_logger::init_log_broadcast();
                tauri::async_runtime::spawn(async move {
                    use tauri::Emitter;
                    loop {
                        match log_rx.recv().await {
                            Ok(line) => { let _ = log_handle.emit("live:log", &line); }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                });
                Ok(())
            })
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
                optimization::get_optimization_run_generation,
                optimization::delete_optimization_run,
                trading::get_current_price,
                trading::get_balance,
                trading::manual_buy,
                trading::manual_sell,
                trading::manual_market_order,
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
                notification::test_trade_notifications,
                live_trading::save_preset,
                live_trading::list_presets,
                live_trading::delete_preset,
                live_trading::create_session,
                live_trading::set_session_order_cap,
                live_trading::list_sessions,
                live_trading::start_session,
                live_trading::stop_session,
                live_trading::delete_session,
                live_trading::list_session_trades,
                live_trading::get_session_signal_log,
                live_trading::toggle_session_mode,
                live_trading::emergency_stop_all_real,
                live_trading::list_pending_orders,
                upbit_keys::save_upbit_keys,
                upbit_keys::get_upbit_key_status,
                upbit_keys::clear_upbit_keys,
                upbit_keys::test_upbit_connection,
                history::list_real_trades,
                history::real_pnl_summary,
                history::export_real_trades_csv,
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
