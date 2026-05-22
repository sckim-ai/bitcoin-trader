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
    baseline_return: Option<f64>,
    baseline_trades: Option<i32>,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO presets
            (user_id, name, strategy_key, params_json, source, source_run_id,
             market, timeframe, since_ts, until_ts,
             baseline_return, baseline_trades, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![user_id, name, strategy_key, params_json, source, source_run_id,
                market, timeframe, since_ts, until_ts,
                baseline_return, baseline_trades, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_preset(conn: &Connection, id: i64) -> Result<Option<Preset>> {
    conn.query_row(
        "SELECT id, user_id, name, strategy_key, params_json, source, source_run_id,
                market, timeframe, since_ts, until_ts,
                baseline_return, baseline_trades, created_at
         FROM presets WHERE id = ?1",
        [id],
        row_to_preset,
    )
    .optional()
}

pub fn list_presets(conn: &Connection, user_id: i64) -> Result<Vec<Preset>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, name, strategy_key, params_json, source, source_run_id,
                market, timeframe, since_ts, until_ts,
                baseline_return, baseline_trades, created_at
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
        baseline_return: row.get(11)?,
        baseline_trades: row.get(12)?,
        created_at: row.get(13)?,
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
    upbit_account_id: Option<i64>,
) -> Result<i64> {
    // real_started_at = created_at = now. start_ts may be earlier (preset's
    // backtest window start). live_return is the cumulative % from
    // real_started_at onward; starts at 0.
    // notify_discord은 DB 컬럼 DEFAULT 0이 적용되며, 명시적 켜기는 호출자가
    // set_session_notify_discord로 후속 처리한다(시그니처 안정성 우선).
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO live_sessions
            (user_id, label, preset_id, market, mode, status, initial_capital,
             start_ts, real_started_at, current_position, current_equity,
             live_return, created_at, upbit_account_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 'stopped', ?6, ?7, ?8, 'idle', ?6, 0.0, ?8, ?9)",
        params![user_id, label, preset_id, market, mode, initial_capital, start_ts, now, upbit_account_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_session(conn: &Connection, id: i64) -> Result<Option<LiveSession>> {
    conn.query_row(
        "SELECT ls.id, ls.user_id, ls.label, ls.preset_id, ls.market, ls.mode, ls.status,
                ls.initial_capital, ls.start_ts, ls.real_started_at, ls.last_cycle_ts,
                ls.last_signal, ls.current_position, ls.current_buy_price, ls.current_buy_volume,
                ls.current_equity, ls.live_return, ls.max_daily_loss_pct, ls.max_daily_trades,
                ls.max_order_krw, ls.created_at,
                ls.upbit_account_id, ua.label AS account_label,
                ua.discord_webhook_url AS account_discord_webhook,
                ls.notify_discord, ls.notify_account_ids
         FROM live_sessions ls
         LEFT JOIN upbit_accounts ua ON ua.id = ls.upbit_account_id
         WHERE ls.id = ?1",
        [id],
        row_to_session,
    )
    .optional()
}

pub fn list_sessions(conn: &Connection, user_id: i64) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT ls.id, ls.user_id, ls.label, ls.preset_id, ls.market, ls.mode, ls.status,
                ls.initial_capital, ls.start_ts, ls.real_started_at, ls.last_cycle_ts,
                ls.last_signal, ls.current_position, ls.current_buy_price, ls.current_buy_volume,
                ls.current_equity, ls.live_return, ls.max_daily_loss_pct, ls.max_daily_trades,
                ls.max_order_krw, ls.created_at,
                ls.upbit_account_id, ua.label AS account_label,
                ua.discord_webhook_url AS account_discord_webhook,
                ls.notify_discord, ls.notify_account_ids
         FROM live_sessions ls
         LEFT JOIN upbit_accounts ua ON ua.id = ls.upbit_account_id
         WHERE ls.user_id = ?1 ORDER BY ls.created_at DESC",
    )?;
    let rows = stmt.query_map([user_id], row_to_session)?;
    rows.collect()
}

pub fn list_running_sessions(conn: &Connection) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT ls.id, ls.user_id, ls.label, ls.preset_id, ls.market, ls.mode, ls.status,
                ls.initial_capital, ls.start_ts, ls.real_started_at, ls.last_cycle_ts,
                ls.last_signal, ls.current_position, ls.current_buy_price, ls.current_buy_volume,
                ls.current_equity, ls.live_return, ls.max_daily_loss_pct, ls.max_daily_trades,
                ls.max_order_krw, ls.created_at,
                ls.upbit_account_id, ua.label AS account_label,
                ua.discord_webhook_url AS account_discord_webhook,
                ls.notify_discord, ls.notify_account_ids
         FROM live_sessions ls
         LEFT JOIN upbit_accounts ua ON ua.id = ls.upbit_account_id
         WHERE ls.status = 'running'",
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

/// Switch a session between paper and real modes. Caller is responsible for
/// validating that API keys are configured before promoting. For real
/// promotion with an explicit account, prefer `set_session_mode_real` which
/// updates mode + account_id atomically.
pub fn set_session_mode(conn: &Connection, id: i64, mode: &str) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET mode = ?1 WHERE id = ?2",
        params![mode, id],
    )
}

/// Promote a session to real mode AND bind it to a specific Upbit account in
/// a single UPDATE. The partial unique index
/// `idx_session_account_running_real` rejects a second running real session
/// on the same account at the DB level — callers should catch UNIQUE
/// violation and surface a friendly message.
pub fn set_session_mode_real(
    conn: &Connection,
    id: i64,
    account_id: i64,
) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET mode = 'real', upbit_account_id = ?1 WHERE id = ?2",
        params![account_id, id],
    )
}

/// Set the per-session BUY cap. `None` clears the cap (full balance).
pub fn set_session_max_order_krw(conn: &Connection, id: i64, cap: Option<f64>) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET max_order_krw = ?1 WHERE id = ?2",
        params![cap, id],
    )
}

/// paper 세션 디스코드 알림 토글. real 세션에도 호출 가능하나 알림 정책상 효과 없음
/// (real은 이 플래그와 무관하게 항상 알림 발송).
pub fn set_session_notify_discord(conn: &Connection, id: i64, value: bool) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET notify_discord = ?1 WHERE id = ?2",
        params![value as i64, id],
    )
}

/// Paper 세션의 알림 fan-out 계정 목록을 갱신. 빈 array는 알림 off로 해석된다.
/// JSON 직렬화 실패는 파라미터 입력 오류일 때만 발생하므로 호출자에게 그대로 전파.
pub fn set_session_notify_account_ids(
    conn: &Connection,
    id: i64,
    ids: &[i64],
) -> Result<usize> {
    let json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE live_sessions SET notify_account_ids = ?1 WHERE id = ?2",
        params![json, id],
    )
}

/// Stop every running real session. Returns the affected ids so the caller
/// can emit per-session events. Mode stays 'real' — the user explicitly
/// promoted these and we don't want to silently demote on emergency stop.
pub fn stop_all_real_sessions(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM live_sessions WHERE mode = 'real' AND status = 'running'",
    )?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>>>()?;
    drop(stmt);
    if !ids.is_empty() {
        conn.execute(
            "UPDATE live_sessions SET status = 'stopped'
             WHERE mode = 'real' AND status = 'running'",
            [],
        )?;
    }
    Ok(ids)
}

#[allow(clippy::too_many_arguments)]
pub fn update_session_cycle(
    conn: &Connection,
    id: i64,
    last_cycle_ts: &str,
    last_signal: &str,
    current_position: &str,
    current_buy_price: Option<f64>,
    current_buy_volume: Option<f64>,
    current_equity: f64,
    live_return: f64,
) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET
            last_cycle_ts = ?1,
            last_signal = ?2,
            current_position = ?3,
            current_buy_price = ?4,
            current_buy_volume = ?5,
            current_equity = ?6,
            live_return = ?7
         WHERE id = ?8",
        params![last_cycle_ts, last_signal, current_position,
                current_buy_price, current_buy_volume, current_equity, live_return, id],
    )
}

pub fn delete_session(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM live_sessions WHERE id = ?1", [id])
}

/// Replace this session's paper-trade rows with a clean slate. Real-trade
/// rows (`is_real=1`) are preserved — those represent actual Upbit fills and
/// should never be deleted. Used by session_engine before inserting the
/// current simulation's trades, guaranteeing live_trades always matches
/// the latest result.trades + result.signal_log.
pub fn delete_paper_trades(conn: &Connection, session_id: i64) -> Result<usize> {
    conn.execute(
        "DELETE FROM live_trades WHERE session_id = ?1 AND is_real = 0",
        [session_id],
    )
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
        live_return: row.get(16)?,
        max_daily_loss_pct: row.get(17)?,
        max_daily_trades: row.get(18)?,
        max_order_krw: row.get(19)?,
        created_at: row.get(20)?,
        upbit_account_id: row.get(21)?,
        account_label: row.get(22)?,
        account_discord_webhook: row.get(23)?,
        notify_discord: row.get::<_, i64>(24)? != 0,
        // JSON 파싱 실패 시 빈 array — 옛 row(컬럼 누락) 또는 잘못 저장된 데이터에
        // 대한 방어. 파싱 실패가 알림을 켜는 사고를 만들지 않도록 fail-closed.
        notify_account_ids: serde_json::from_str(
            row.get::<_, String>(25).as_deref().unwrap_or("[]"),
        )
        .unwrap_or_default(),
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

// ─── Safety limits (Phase 4A.6) ───

/// Sum of pnl on is_real=1 sells since UTC midnight today, divided by
/// `initial_capital * 100` to express as a percentage. Negative when in
/// loss. Used by the daily-loss circuit breaker.
pub fn today_realized_pnl_pct(
    conn: &Connection,
    session_id: i64,
    initial_capital: f64,
) -> Result<f64> {
    if initial_capital <= 0.0 {
        return Ok(0.0);
    }
    // SQLite "now" is UTC; date('now') gives YYYY-MM-DD. We compare against
    // the date prefix of `ts` (RFC3339 begins with YYYY-MM-DD). Both strings
    // are UTC-anchored so prefix comparison is correct.
    let today: String = conn.query_row("SELECT date('now')", [], |r| r.get(0))?;
    let pnl_sum: Option<f64> = conn.query_row(
        "SELECT COALESCE(SUM(pnl), 0)
           FROM live_trades
          WHERE session_id = ?1 AND side = 'sell' AND is_real = 1
            AND substr(ts, 1, 10) = ?2",
        params![session_id, today],
        |r| r.get(0),
    )?;
    Ok(pnl_sum.unwrap_or(0.0) / initial_capital * 100.0)
}

/// Number of is_real=1 sells today (UTC). Used by the per-day trade-count
/// circuit breaker.
pub fn today_real_trades_count(conn: &Connection, session_id: i64) -> Result<i64> {
    let today: String = conn.query_row("SELECT date('now')", [], |r| r.get(0))?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM live_trades
          WHERE session_id = ?1 AND side = 'sell' AND is_real = 1
            AND substr(ts, 1, 10) = ?2",
        params![session_id, today],
        |r| r.get(0),
    )?;
    Ok(n)
}

// ─── Pending Orders (Phase 4A.5) ───

#[derive(Debug, Clone)]
pub struct PendingOrder {
    pub uuid: String,
    pub session_id: i64,
    pub side: String,            // "bid" | "ask"
    pub market: String,
    pub ord_type: String,        // "price" | "market" | "limit"
    pub target_price: Option<f64>,
    pub requested: f64,          // bid → KRW amount, ask → coin volume
    pub placed_at: String,
    pub status: String,          // "wait" | "done" | "cancel"
    pub last_checked: Option<String>,
    pub resolved_at: Option<String>,
    /// Cost basis at SELL-placement time (NULL for BUYs).
    /// Frozen here so late fills price correctly against the cost basis
    /// that was current at the moment the SELL decision was made.
    pub cost_basis_price: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
pub fn insert_pending_order(
    conn: &Connection,
    uuid: &str,
    session_id: i64,
    side: &str,
    market: &str,
    ord_type: &str,
    target_price: Option<f64>,
    requested: f64,
    placed_at: &str,
    status: &str,
    cost_basis_price: Option<f64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO pending_orders
            (uuid, session_id, side, market, ord_type, target_price, requested,
             placed_at, status, cost_basis_price)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(uuid) DO UPDATE SET status = excluded.status",
        params![uuid, session_id, side, market, ord_type, target_price, requested,
                placed_at, status, cost_basis_price],
    )?;
    Ok(())
}

/// `wait`-state orders for a single session. Used at the start of every
/// real cycle to find orders that need cancelling so we can re-peg at the
/// new bar's close.
pub fn list_session_pending_wait(
    conn: &Connection,
    session_id: i64,
) -> Result<Vec<PendingOrder>> {
    let mut stmt = conn.prepare(
        "SELECT uuid, session_id, side, market, ord_type, target_price, requested,
                placed_at, status, last_checked, resolved_at, cost_basis_price
         FROM pending_orders WHERE session_id = ?1 AND status = 'wait'
         ORDER BY placed_at ASC",
    )?;
    let rows = stmt.query_map([session_id], row_to_pending)?;
    rows.collect()
}

/// All `wait`-state orders across all sessions. Tracker reconciles these
/// at the start of every cycle.
pub fn list_pending_wait(conn: &Connection) -> Result<Vec<PendingOrder>> {
    let mut stmt = conn.prepare(
        "SELECT uuid, session_id, side, market, ord_type, target_price, requested,
                placed_at, status, last_checked, resolved_at, cost_basis_price
         FROM pending_orders WHERE status = 'wait' ORDER BY placed_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_pending)?;
    rows.collect()
}

fn row_to_pending(row: &rusqlite::Row) -> Result<PendingOrder> {
    Ok(PendingOrder {
        uuid: row.get(0)?,
        session_id: row.get(1)?,
        side: row.get(2)?,
        market: row.get(3)?,
        ord_type: row.get(4)?,
        target_price: row.get(5)?,
        requested: row.get(6)?,
        placed_at: row.get(7)?,
        status: row.get(8)?,
        last_checked: row.get(9)?,
        resolved_at: row.get(10)?,
        cost_basis_price: row.get(11)?,
    })
}

pub fn mark_pending_resolved(
    conn: &Connection,
    uuid: &str,
    new_status: &str,    // "done" | "cancel"
    resolved_at: &str,
) -> Result<usize> {
    conn.execute(
        "UPDATE pending_orders SET status = ?1, resolved_at = ?2, last_checked = ?2
         WHERE uuid = ?3",
        params![new_status, resolved_at, uuid],
    )
}

pub fn touch_pending_check(conn: &Connection, uuid: &str, when: &str) -> Result<usize> {
    conn.execute(
        "UPDATE pending_orders SET last_checked = ?1 WHERE uuid = ?2",
        params![when, uuid],
    )
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
        conn.execute_batch(include_str!("../../migrations/001_initial.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/002_users.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/006_live_trading.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/007_preset_context.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/008_session_signal_log.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/009_baseline_metrics.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/010_pending_orders.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/011_safety_limits.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/012_order_caps.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/013_pending_cost_basis.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/014_upbit_accounts.sql")).unwrap();
        conn
    }

    // ─── Presets ───

    #[test]
    fn test_preset_insert_and_get() {
        let conn = setup_db();
        let id = insert_preset(&conn, 1, "V3-A", "V3", r#"{"foo":1}"#, "manual", None, None, None, None, None, None, None).unwrap();
        let preset = get_preset(&conn, id).unwrap().expect("preset should exist");
        assert_eq!(preset.name, "V3-A");
        assert_eq!(preset.strategy_key, "V3");
        assert_eq!(preset.source, "manual");
    }

    #[test]
    fn test_preset_list_ordering() {
        let conn = setup_db();
        insert_preset(&conn, 1, "A", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        insert_preset(&conn, 1, "B", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let list = list_presets(&conn, 1).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "B"); // 최신이 먼저
    }

    #[test]
    fn test_preset_delete() {
        let conn = setup_db();
        let id = insert_preset(&conn, 1, "X", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let n = delete_preset(&conn, id).unwrap();
        assert_eq!(n, 1);
        assert!(get_preset(&conn, id).unwrap().is_none());
    }

    #[test]
    fn test_preset_unique_name_per_user() {
        let conn = setup_db();
        insert_preset(&conn, 1, "dup", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let result = insert_preset(&conn, 1, "dup", "V3", "{}", "manual", None, None, None, None, None, None, None);
        assert!(result.is_err(), "duplicate name should fail");
    }

    // ─── Sessions ───

    #[test]
    fn test_session_insert_and_defaults() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z", None).unwrap();
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
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z", None).unwrap();

        assert_eq!(list_running_sessions(&conn).unwrap().len(), 0);
        set_session_status(&conn, sid, "running").unwrap();
        assert_eq!(list_running_sessions(&conn).unwrap().len(), 1);
        set_session_status(&conn, sid, "stopped").unwrap();
        assert_eq!(list_running_sessions(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_set_session_mode_and_count() {
        // count_real_sessions removed; replicate with inline SQL.
        let count_real = |c: &Connection, exclude: Option<i64>| -> i64 {
            match exclude {
                Some(id) => c.query_row(
                    "SELECT COUNT(*) FROM live_sessions WHERE mode = 'real' AND id != ?1",
                    [id], |r| r.get(0),
                ).unwrap(),
                None => c.query_row(
                    "SELECT COUNT(*) FROM live_sessions WHERE mode = 'real'",
                    [], |r| r.get(0),
                ).unwrap(),
            }
        };

        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let s1 = insert_session(&conn, 1, "A", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();
        let s2 = insert_session(&conn, 1, "B", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();

        assert_eq!(count_real(&conn, None), 0);

        set_session_mode(&conn, s1, "real").unwrap();
        assert_eq!(count_real(&conn, None), 1);
        assert_eq!(count_real(&conn, Some(s1)), 0);

        set_session_mode(&conn, s2, "real").unwrap();
        assert_eq!(count_real(&conn, None), 2);
        assert_eq!(count_real(&conn, Some(s1)), 1);
    }

    #[test]
    fn test_stop_all_real_sessions() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let s1 = insert_session(&conn, 1, "A", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();
        let s2 = insert_session(&conn, 1, "B", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();

        // s1 = running real, s2 = running paper, s3 = stopped real
        let s3 = insert_session(&conn, 1, "C", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();
        set_session_mode(&conn, s1, "real").unwrap();
        set_session_status(&conn, s1, "running").unwrap();
        set_session_status(&conn, s2, "running").unwrap();
        set_session_mode(&conn, s3, "real").unwrap();
        // s3 stays 'stopped'

        let stopped = stop_all_real_sessions(&conn).unwrap();
        assert_eq!(stopped, vec![s1]); // only running real
        assert_eq!(get_session(&conn, s1).unwrap().unwrap().status, "stopped");
        assert_eq!(get_session(&conn, s2).unwrap().unwrap().status, "running"); // paper untouched
        // mode preserved on s1
        assert_eq!(get_session(&conn, s1).unwrap().unwrap().mode, "real");
    }

    #[test]
    fn test_session_cycle_update() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z", None).unwrap();

        update_session_cycle(&conn, sid, "2026-04-24T05:00:00Z", "buy", "holding",
            Some(3_200_000.0), Some(0.312), 1_100_000.0, 0.0).unwrap();

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
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();

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
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();

        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 0);
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3e6, 0.3, 450.0, "buy", None, None, false).unwrap();
        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 0);
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(30e3), Some(3.3), false).unwrap();
        assert_eq!(count_completed_trades(&conn, sid).unwrap(), 1);
    }

    #[test]
    fn test_equity_upsert() {
        let conn = setup_db();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();

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
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z", None).unwrap();
        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy", 3e6, 0.3, 450.0, "buy", None, None, false).unwrap();
        upsert_equity(&conn, sid, "2026-04-24T01:00:00Z", 1e6, "idle").unwrap();

        delete_session(&conn, sid).unwrap();

        assert_eq!(list_trades(&conn, sid).unwrap().len(), 0);
        assert_eq!(list_equity(&conn, sid).unwrap().len(), 0);
    }
}
