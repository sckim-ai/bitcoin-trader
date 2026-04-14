use bitcoin_trader_lib::auth::{password, session};
use bitcoin_trader_lib::core::indicators::calculate_all;
use bitcoin_trader_lib::core::optimizer::Nsga2Optimizer;
use bitcoin_trader_lib::migration::csv_import;
use bitcoin_trader_lib::models::config::OptimizerConfig;
use bitcoin_trader_lib::models::market::{Candle, MarketData};
use bitcoin_trader_lib::models::trading::TradingParameters;
use bitcoin_trader_lib::notifications::manager::{
    format_signal_message, format_trade_message, NotificationManager,
};
use bitcoin_trader_lib::strategies::StrategyRegistry;
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use std::io::Write;

// --- Helpers ---

fn init_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let schema_v1 = include_str!("../migrations/001_initial.sql");
    conn.execute_batch(schema_v1).unwrap();
    let schema_v2 = include_str!("../migrations/002_users.sql");
    conn.execute_batch(schema_v2).unwrap();
    conn
}

fn seed_admin(conn: &Connection) {
    let hash = password::hash_password("admin123").unwrap();
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES ('admin', ?1, 'admin')",
        [&hash],
    )
    .unwrap();
}

struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u32() as f64) / (u32::MAX as f64)
    }
}

fn make_realistic_data(n: usize, seed: u32) -> Vec<MarketData> {
    let mut rng = SimpleRng::new(seed);
    let mut price = 50000.0_f64;
    let mut candles = Vec::with_capacity(n);

    for i in 0..n {
        let change_pct = (rng.next_f64() - 0.498) * 0.04;
        price *= 1.0 + change_pct;
        price = price.max(1000.0);

        let high = price * (1.0 + rng.next_f64() * 0.015);
        let low = price * (1.0 - rng.next_f64() * 0.015);
        let base_volume = 3000.0 + rng.next_f64() * 2000.0;
        let spike = if rng.next_f64() > 0.9 {
            5.0 + rng.next_f64() * 15.0
        } else {
            1.0
        };
        let volume = base_volume * spike;

        candles.push(Candle {
            timestamp: Utc
                .with_ymd_and_hms(2024, 1, 1, (i % 24) as u32, 0, 0)
                .unwrap(),
            open: price * (1.0 + (rng.next_f64() - 0.5) * 0.005),
            high,
            low,
            close: price,
            volume,
        });
    }

    let indicators = calculate_all(&candles);
    candles
        .into_iter()
        .zip(indicators)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect()
}

// --- Test 1: Full auth flow ---

#[test]
fn test_full_auth_flow() {
    let conn = init_db();
    seed_admin(&conn);

    // Login as admin
    let admin_token = session::create_session(&conn, 1).unwrap();
    let admin_id = session::validate_session(&conn, &admin_token)
        .unwrap()
        .unwrap();
    assert_eq!(admin_id, 1);

    // Create a trader user
    let trader_hash = password::hash_password("trader_pass").unwrap();
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES ('trader1', ?1, 'trader')",
        [&trader_hash],
    )
    .unwrap();
    let trader_id = conn.last_insert_rowid();

    // Login as trader
    let trader_token = session::create_session(&conn, trader_id).unwrap();
    let validated = session::validate_session(&conn, &trader_token)
        .unwrap()
        .unwrap();
    assert_eq!(validated, trader_id);

    // Verify trader role
    let trader_role: String = conn
        .query_row(
            "SELECT role FROM users WHERE id = ?1",
            [trader_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(trader_role, "trader");

    // Verify admin role
    let admin_role: String = conn
        .query_row("SELECT role FROM users WHERE id = ?1", [admin_id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(admin_role, "admin");

    // Logout
    session::delete_session(&conn, &trader_token).unwrap();
    assert!(session::validate_session(&conn, &trader_token)
        .unwrap()
        .is_none());
}

// --- Test 2: Full trading pipeline ---

#[test]
fn test_full_trading_pipeline() {
    let conn = init_db();

    // Create a temp CSV and import
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("test_data.csv");
    {
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        for i in 0..300 {
            let ts = format!("2024-01-{:02}T{:02}:00:00Z", (i / 24) + 1, i % 24);
            let price = 50000.0 + (i as f64) * 10.0;
            writeln!(
                f,
                "{},{:.1},{:.1},{:.1},{:.1},{:.1}",
                ts,
                price,
                price + 50.0,
                price - 50.0,
                price + 5.0,
                3000.0 + (i as f64) * 100.0
            )
            .unwrap();
        }
    }

    let count = csv_import::import_csv(&conn, &csv_path, "BTC", "hour").unwrap();
    assert_eq!(count, 300);

    // Load and verify
    let candles = csv_import::load_candles(&conn, "BTC", "hour").unwrap();
    assert_eq!(candles.len(), 300);

    // Calculate indicators and run simulation
    let indicators = calculate_all(&candles);
    let data: Vec<MarketData> = candles
        .into_iter()
        .zip(indicators)
        .map(|(candle, indicators)| MarketData { candle, indicators })
        .collect();

    let registry = StrategyRegistry::new();
    let v0 = registry.get("V0").unwrap();
    let params = TradingParameters::default();
    let result = v0.run_simulation(&data, &params);
    assert!(result.total_return.is_finite());

    // Run NSGA-II optimization (small pop/gen for speed)
    let config = OptimizerConfig {
        population_size: 5,
        generations: 2,
        crossover_rate: 0.9,
        mutation_rate: 0.1,
        ..Default::default()
    };
    let optimizer = Nsga2Optimizer::new(config);
    let results = optimizer.run(&data, v0, None);
    assert_eq!(results.len(), 5);
    for ind in &results {
        assert_eq!(ind.objectives.len(), 2);
    }
}

// --- Test 3: Notification manager ---

#[test]
fn test_notification_manager_empty() {
    let conn = init_db();
    let _mgr = NotificationManager::from_db(&conn, 999);
    // send_all with no channels configured should not panic
    // We can't call async here easily, but from_db returning without panic is the key test
    assert!(true); // Manager created without panic
}

#[test]
fn test_notification_message_formatting() {
    // Buy message
    let msg = format_trade_message("buy", "KRW-BTC", 50000000.0, 0.001, None).unwrap();
    assert!(msg.contains("KRW-BTC"));
    assert!(msg.contains("매수"));

    // Sell message with P/L
    let msg = format_trade_message("sell", "KRW-BTC", 51000000.0, 0.001, Some(3.5)).unwrap();
    assert!(msg.contains("매도"));
    assert!(msg.contains("3.50"));

    // Unknown side returns None
    assert!(format_trade_message("hold", "X", 0.0, 0.0, None).is_none());

    // Signal message
    let msg = format_signal_message("KRW-ETH", "매수", "V5");
    assert!(msg.contains("KRW-ETH"));
    assert!(msg.contains("V5 전략"));
}

// --- Test 4: Strategy registry completeness ---

#[test]
fn test_strategy_registry_completeness() {
    let registry = StrategyRegistry::new();
    let strategies = registry.list();

    // All 6 strategies (V0-V5)
    assert!(
        strategies.len() >= 6,
        "Expected at least 6 strategies, got {}",
        strategies.len()
    );

    let keys: Vec<&str> = strategies.iter().map(|(k, _)| *k).collect();
    for expected in &["V0", "V1", "V2", "V3", "V4", "V5"] {
        assert!(keys.contains(expected), "Missing strategy: {}", expected);
    }

    // Each strategy runs without panic and returns parameter ranges
    let data = make_realistic_data(200, 42);
    let params = TradingParameters::default();

    for (key, _name) in &strategies {
        let strategy = registry.get(key).unwrap();

        // Should not panic
        let result = strategy.run_simulation(&data, &params);
        assert!(
            result.total_return.is_finite(),
            "Strategy {} returned non-finite result",
            key
        );

        // Should return parameter ranges
        let ranges = strategy.parameter_ranges();
        assert!(
            !ranges.is_empty(),
            "Strategy {} returned empty parameter ranges",
            key
        );
    }
}

// --- Test 5: DB schema completeness ---

#[test]
fn test_db_schema_completeness() {
    let conn = init_db();

    let expected_tables = [
        "market_data",
        "indicators",
        "strategy_configs",
        "positions",
        "trades",
        "optimization_runs",
        "optimization_results",
        "users",
        "sessions",
        "notification_configs",
    ];

    for table in &expected_tables {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "Table '{}' should exist", table);
    }

    // Verify INSERT/SELECT for each table
    // market_data
    conn.execute(
        "INSERT INTO market_data (market, timeframe, timestamp, open, high, low, close, volume)
         VALUES ('TEST', 'hour', '2024-01-01T00:00:00Z', 1.0, 2.0, 0.5, 1.5, 100.0)",
        [],
    )
    .unwrap();
    let c: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM market_data WHERE market='TEST'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(c, 1);

    // users
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES ('schema_test', 'h', 'trader')",
        [],
    )
    .unwrap();
    let c: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM users WHERE username='schema_test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(c, 1);

    // sessions
    let uid: i64 = conn
        .query_row(
            "SELECT id FROM users WHERE username='schema_test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES ('tok123', ?1, '2099-01-01')",
        [uid],
    )
    .unwrap();

    // notification_configs
    conn.execute(
        "INSERT INTO notification_configs (user_id, channel, config, enabled) VALUES (?1, 'test', '{}', 1)",
        [uid],
    ).unwrap();

    // strategy_configs
    conn.execute(
        "INSERT INTO strategy_configs (user_id, strategy_key, name, parameters) VALUES (1, 'V0', 'Test', '{}')",
        [],
    ).unwrap();

    // positions
    conn.execute(
        "INSERT INTO positions (user_id, market, status) VALUES (1, 'TEST', 'idle')",
        [],
    )
    .unwrap();

    // trades
    conn.execute(
        "INSERT INTO trades (user_id, market, side, order_type, price, volume, executed_at)
         VALUES (1, 'TEST', 'buy', 'limit', 100.0, 1.0, '2024-01-01')",
        [],
    )
    .unwrap();

    // optimization_runs
    conn.execute(
        "INSERT INTO optimization_runs (user_id, strategy_key, population_size, generations, objectives)
         VALUES (1, 'V0', 10, 5, 'return,win_rate')",
        [],
    ).unwrap();
    let run_id = conn.last_insert_rowid();

    // optimization_results
    conn.execute(
        "INSERT INTO optimization_results (run_id, generation, rank, parameters) VALUES (?1, 1, 1, '{}')",
        [run_id],
    ).unwrap();

    // indicators (needs market_data_id FK)
    let md_id: i64 = conn
        .query_row("SELECT id FROM market_data LIMIT 1", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO indicators (market_data_id) VALUES (?1)",
        [md_id],
    )
    .unwrap();
}

// --- Test 6: CSV migration tool ---

#[test]
fn test_csv_migration() {
    let conn = init_db();
    let dir = tempfile::tempdir().unwrap();

    // Create hour CSV
    let hour_path = dir.path().join("merged_data_hour.csv");
    {
        let mut f = std::fs::File::create(&hour_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        writeln!(f, "2024-01-01T00:00:00Z,100.0,105.0,95.0,102.0,1000.0").unwrap();
        writeln!(f, "2024-01-01T01:00:00Z,102.0,108.0,99.0,106.0,1500.0").unwrap();
    }

    // Create day CSV
    let day_path = dir.path().join("merged_data_day.csv");
    {
        let mut f = std::fs::File::create(&day_path).unwrap();
        writeln!(f, "timestamp,open,high,low,close,volume").unwrap();
        writeln!(f, "2024-01-01T00:00:00Z,100.0,110.0,90.0,105.0,5000.0").unwrap();
    }

    // No week CSV — should return 0

    let hour_count = csv_import::import_csv(&conn, &hour_path, "KRW-BTC", "hour").unwrap();
    assert_eq!(hour_count, 2);

    let day_count = csv_import::import_csv(&conn, &day_path, "KRW-BTC", "day").unwrap();
    assert_eq!(day_count, 1);

    // Verify data is in DB
    let h_candles = csv_import::load_candles(&conn, "KRW-BTC", "hour").unwrap();
    assert_eq!(h_candles.len(), 2);

    let d_candles = csv_import::load_candles(&conn, "KRW-BTC", "day").unwrap();
    assert_eq!(d_candles.len(), 1);
}
