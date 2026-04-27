use crate::models::live::{LiveEquityPoint, LiveSession, LiveTrade, Preset};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result};

// ─── Presets ───

#[allow(clippy::too_many_arguments)]
pub fn insert_preset(
    conn: &Connection,
    user_id: i64,
    name: &str,
    strategy_key: &str,
    params_json: &str,
    source: &str,
    source_run_id: Option<i64>,
    market: Option<&str>,
    timeframe: Option<&str>,
    since_ts: Option<&str>,
    until_ts: Option<&str>,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO presets
            (user_id, name, strategy_key, params_json, source, source_run_id,
             market, timeframe, since_ts, until_ts, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![user_id, name, strategy_key, params_json, source, source_run_id,
                market, timeframe, since_ts, until_ts, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_preset(conn: &Connection, id: i64) -> Result<Option<Preset>> {
    conn.query_row(
        "SELECT id, user_id, name, strategy_key, params_json, source, source_run_id,
                market, timeframe, since_ts, until_ts, created_at
         FROM presets WHERE id = ?1",
        [id],
        row_to_preset,
    )
    .optional()
}

pub fn list_presets(conn: &Connection, user_id: i64) -> Result<Vec<Preset>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, name, strategy_key, params_json, source, source_run_id,
                market, timeframe, since_ts, until_ts, created_at
         FROM presets WHERE user_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([user_id], row_to_preset)?;
    rows.collect()
}

fn row_to_preset(row: &rusqlite::Row) -> Result<Preset> {
    Ok(Preset {
        id: row.get(0)?,
        user_id: row.get(1)?,
        name: row.get(2)?,
        strategy_key: row.get(3)?,
        params_json: row.get(4)?,
        source: row.get(5)?,
        source_run_id: row.get(6)?,
        market: row.get(7)?,
        timeframe: row.get(8)?,
        since_ts: row.get(9)?,
        until_ts: row.get(10)?,
        created_at: row.get(11)?,
    })
}

pub fn delete_preset(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM presets WHERE id = ?1", [id])
}

// ─── Live Sessions ───

pub fn insert_session(
    conn: &Connection,
    user_id: i64,
    label: &str,
    preset_id: i64,
    market: &str,
    mode: &str,
    initial_capital: f64,
    start_ts: &str,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO live_sessions
            (user_id, label, preset_id, market, mode, status, initial_capital,
             start_ts, current_position, current_equity, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'stopped', ?6, ?7, 'idle', ?6, ?8)",
        params![user_id, label, preset_id, market, mode, initial_capital, start_ts, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_session(conn: &Connection, id: i64) -> Result<Option<LiveSession>> {
    conn.query_row(
        "SELECT id, user_id, label, preset_id, market, mode, status, initial_capital,
                start_ts, real_started_at, last_cycle_ts, last_signal,
                current_position, current_buy_price, current_buy_volume, current_equity,
                created_at
         FROM live_sessions WHERE id = ?1",
        [id],
        row_to_session,
    )
    .optional()
}

pub fn list_sessions(conn: &Connection, user_id: i64) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, label, preset_id, market, mode, status, initial_capital,
                start_ts, real_started_at, last_cycle_ts, last_signal,
                current_position, current_buy_price, current_buy_volume, current_equity,
                created_at
         FROM live_sessions WHERE user_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([user_id], row_to_session)?;
    rows.collect()
}

pub fn list_running_sessions(conn: &Connection) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, label, preset_id, market, mode, status, initial_capital,
                start_ts, real_started_at, last_cycle_ts, last_signal,
                current_position, current_buy_price, current_buy_volume, current_equity,
                created_at
         FROM live_sessions WHERE status = 'running'",
    )?;
    let rows = stmt.query_map([], row_to_session)?;
    rows.collect()
}

pub fn set_session_status(conn: &Connection, id: i64, status: &str) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET status = ?1 WHERE id = ?2",
        params![status, id],
    )
}

pub fn update_session_cycle(
    conn: &Connection,
    id: i64,
    last_cycle_ts: &str,
    last_signal: &str,
    current_position: &str,
    current_buy_price: Option<f64>,
    current_buy_volume: Option<f64>,
    current_equity: f64,
) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET
            last_cycle_ts = ?1,
            last_signal = ?2,
            current_position = ?3,
            current_buy_price = ?4,
            current_buy_volume = ?5,
            current_equity = ?6
         WHERE id = ?7",
        params![last_cycle_ts, last_signal, current_position,
                current_buy_price, current_buy_volume, current_equity, id],
    )
}

pub fn delete_session(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM live_sessions WHERE id = ?1", [id])
}

/// Save the latest cycle's signal_log JSON. Overwrites the previous each
/// cycle so reads always see the freshest mapping.
pub fn update_session_signal_log(
    conn: &Connection,
    id: i64,
    signal_log_json: &str,
) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET signal_log_json = ?1 WHERE id = ?2",
        params![signal_log_json, id],
    )
}

/// Returns the persisted signal_log JSON for a session. None when the column
/// is NULL (session has never run a cycle).
pub fn get_session_signal_log(conn: &Connection, id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT signal_log_json FROM live_sessions WHERE id = ?1",
        [id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|opt| opt.flatten())
}

fn row_to_session(row: &rusqlite::Row) -> Result<LiveSession> {
    Ok(LiveSession {
        id: row.get(0)?,
        user_id: row.get(1)?,
        label: row.get(2)?,
        preset_id: row.get(3)?,
        market: row.get(4)?,
        mode: row.get(5)?,
        status: row.get(6)?,
        initial_capital: row.get(7)?,
        start_ts: row.get(8)?,
        real_started_at: row.get(9)?,
        last_cycle_ts: row.get(10)?,
        last_signal: row.get(11)?,
        current_position: row.get(12)?,
        current_buy_price: row.get(13)?,
        current_buy_volume: row.get(14)?,
        current_equity: row.get(15)?,
        created_at: row.get(16)?,
    })
}

// ─── Live Trades ───

pub fn insert_trade(
    conn: &Connection,
    session_id: i64,
    ts: &str,
    side: &str,
    price: f64,
    volume: f64,
    fee: f64,
    signal: &str,
    pnl: Option<f64>,
    pnl_pct: Option<f64>,
    is_real: bool,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO live_trades
            (session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct,
                if is_real { 1 } else { 0 }],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_trades(conn: &Connection, session_id: i64) -> Result<Vec<LiveTrade>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real
         FROM live_trades WHERE session_id = ?1 ORDER BY ts ASC, id ASC",
    )?;
    let rows = stmt.query_map([session_id], |row| Ok(LiveTrade {
        id: row.get(0)?,
        session_id: row.get(1)?,
        ts: row.get(2)?,
        side: row.get(3)?,
        price: row.get(4)?,
        volume: row.get(5)?,
        fee: row.get(6)?,
        signal: row.get(7)?,
        pnl: row.get(8)?,
        pnl_pct: row.get(9)?,
        is_real: row.get::<_, i64>(10)? != 0,
    }))?;
    rows.collect()
}

/// Count completed trades = number of sell rows.
/// Phase 1 inserts only completed pairs (buy + sell both), so sell count =
/// completed-trade count.
pub fn count_completed_trades(conn: &Connection, session_id: i64) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM live_trades WHERE session_id = ?1 AND side = 'sell'",
        [session_id],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

// ─── Live Equity ───

pub fn upsert_equity(conn: &Connection, session_id: i64, ts: &str, equity: f64, position: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO live_equity (session_id, ts, equity, position) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(session_id, ts) DO UPDATE SET equity = ?3, position = ?4",
        params![session_id, ts, equity, position],
    )?;
    Ok(())
}

pub fn list_equity(conn: &Connection, session_id: i64) -> Result<Vec<LiveEquityPoint>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, ts, equity, position FROM live_equity
         WHERE session_id = ?1 ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map([session_id], |row| Ok(LiveEquityPoint {
        session_id: row.get(0)?,
        ts: row.get(1)?,
        equity: row.get(2)?,
        position: row.get(3)?,
    }))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let s1 = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(s1).unwrap();
        let s2 = include_str!("../../migrations/002_users.sql");
        conn.execute_batch(s2).unwrap();
        let s6 = include_str!("../../migrations/006_live_trading.sql");
        conn.execute_batch(s6).unwrap();
        let s7 = include_str!("../../migrations/007_preset_context.sql");
        conn.execute_batch(s7).unwrap();
        let s8 = include_str!("../../migrations/008_session_signal_log.sql");
        conn.execute_batch(s8).unwrap();
        conn
    }

    // ─── Presets ───

    #[test]
    fn test_preset_insert_and_get() {
        let conn = setup_db();
        let id = insert_preset(&conn, 1, "V3-A", "V3", r#"{"foo":1}"#, "manual", None, None, None, None, None).unwrap();
        let preset = get_preset(&conn, id).unwrap().expect("preset should exist");
        assert_eq!(preset.name, "V3-A");
        assert_eq!(preset.strategy_key, "V3");
        assert_eq!(preset.source, "manual");
    }

    #[test]
    fn test_preset_list_ordering() {
        let conn = setup_db();
        insert_preset(&conn, 1, "A", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        insert_preset(&conn, 1, "B", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let list = list_presets(&conn, 1).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "B"); // 최신이 먼저
    }

    #[test]
    fn test_preset_delete() {
        let conn = setup_db();
        let id = insert_preset(&conn, 1, "X", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let n = delete_preset(&conn, id).unwrap();
        assert_eq!(n, 1);
        assert!(get_preset(&conn, id).unwrap().is_none());
    }

    #[test]
    fn test_preset_unique_name_per_user() {
        let conn = setup_db();
        insert_preset(&conn, 1, "dup", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let result = insert_preset(&conn, 1, "dup", "V3", "{}", "manual", None, None, None, None, None);
        assert!(result.is_err(), "duplicate name should fail");
    }

    // ─── Sessions ───

    #[test]
    fn test_session_insert_and_defaults() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();
        let s = get_session(&conn, sid).unwrap().expect("session exists");
        assert_eq!(s.label, "S1");
        assert_eq!(s.status, "stopped");
        assert_eq!(s.current_position, "idle");
        assert_eq!(s.mode, "paper");
        assert!((s.current_equity.unwrap() - 1_000_000.0).abs() < 0.01);
    }

    #[test]
    fn test_session_status_transitions() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();

        assert_eq!(list_running_sessions(&conn).unwrap().len(), 0);
        set_session_status(&conn, sid, "running").unwrap();
        assert_eq!(list_running_sessions(&conn).unwrap().len(), 1);
        set_session_status(&conn, sid, "stopped").unwrap();
        assert_eq!(list_running_sessions(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_session_cycle_update() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();

        update_session_cycle(&conn, sid, "2026-04-24T05:00:00Z", "buy", "holding",
            Some(3_200_000.0), Some(0.312), 1_100_000.0).unwrap();

        let s = get_session(&conn, sid).unwrap().unwrap();
        assert_eq!(s.last_signal.as_deref(), Some("buy"));
        assert_eq!(s.current_position, "holding");
        assert!((s.current_buy_price.unwrap() - 3_200_000.0).abs() < 0.01);
        assert!((s.current_equity.unwrap() - 1_100_000.0).abs() < 0.01);
    }

    // ─── Trades / Equity ───

    #[test]
    fn test_trade_insert_and_list() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3_000_000.0, 0.3, 450.0, "buy", None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "sell", 3_100_000.0, 0.3, 465.0, "sell", Some(30_000.0), Some(3.3), false).unwrap();

        let trades = list_trades(&conn, sid).unwrap();
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].side, "buy");
        assert_eq!(trades[1].side, "sell");
        assert_eq!(trades[1].pnl, Some(30_000.0));
    }

    #[test]
    fn test_count_completed_trades() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 0);
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3e6, 0.3, 450.0, "buy", None, None, false).unwrap();
        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 0);
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(30e3), Some(3.3), false).unwrap();
        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 1);
    }

    #[test]
    fn test_equity_upsert() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1_000_000.0, "idle").unwrap();
        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1_050_000.0, "holding").unwrap();
        upsert_equity(&conn, sid, "2026-04-24T02:00:00Z", 1_080_000.0, "holding").unwrap();

        let points = list_equity(&conn, sid).unwrap();
        assert_eq!(points.len(), 2);
        assert!((points[0].equity - 1_050_000.0).abs() < 0.01);
        assert!((points[1].equity - 1_080_000.0).abs() < 0.01);
    }

    #[test]
    fn test_delete_session_cascades() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3e6, 0.3, 450.0, "buy", None, None, false).unwrap();
        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1e6, "idle").unwrap();

        delete_session(&conn, sid).unwrap();

        assert_eq!(list_trades(&conn, sid).unwrap().len(), 0);
        assert_eq!(list_equity(&conn, sid).unwrap().len(), 0);
    }
}
