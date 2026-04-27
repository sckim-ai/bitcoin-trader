use crate::api::upbit::UpbitClient;
use crate::db::live_repo;
use crate::models::live::{LiveSession, Preset};
use crate::models::trading::TradingParameters;
use crate::services::auto_trader::fetch_and_prepare_data;
use crate::strategies::StrategyRegistry;
use chrono::Utc;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Output of a single session cycle — what to log/emit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionCycleOutput {
    pub session_id: i64,
    pub new_completed_trades: usize,
    pub latest_signal: String,
    pub current_position: String,
    pub current_equity: f64,
}

/// Run one cycle for a single session. Idempotent — re-running on the same
/// data set produces no new DB rows (diff-based insert).
pub async fn run_session_cycle(
    db: &Arc<Mutex<Connection>>,
    client: &UpbitClient,
    session: &LiveSession,
    preset: &Preset,
    registry: &StrategyRegistry,
) -> Result<SessionCycleOutput, BoxErr> {
    // 1. Fetch candles covering session.start_ts ~ now.
    //    Phase 1 shortcut: pull last 500 hourly bars from Upbit + DB-cached
    //    day-psy; trim to session.start_ts at the strategy call site.
    let data = fetch_and_prepare_data(client, db, &session.market, 500).await?;
    let session_start = chrono::DateTime::parse_from_rfc3339(&session.start_ts)
        .map_err(|e| -> BoxErr { e.to_string().into() })?
        .with_timezone(&Utc);
    let data: Vec<_> = data.into_iter()
        .filter(|md| md.candle.timestamp >= session_start)
        .collect();

    if data.len() < 15 {
        return Ok(SessionCycleOutput {
            session_id: session.id,
            new_completed_trades: 0,
            latest_signal: "insufficient_data".into(),
            current_position: session.current_position.clone(),
            current_equity: session.current_equity.unwrap_or(session.initial_capital),
        });
    }

    // 2. Run full simulation from session start to latest candle.
    let strategy = registry.get(&preset.strategy_key)
        .ok_or_else(|| -> BoxErr { format!("strategy not found: {}", preset.strategy_key).into() })?;
    let params: TradingParameters = serde_json::from_str(&preset.params_json)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;
    let result = strategy.run_simulation(&data, &params);

    // 3. Diff: only new completed trades.
    let new_count = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        let existing = live_repo::count_completed_trades(&conn, session.id)
            .map_err(|e| -> BoxErr { e.to_string().into() })?;
        result.trades.len().saturating_sub(existing)
    };

    // TradeRecord는 볼륨을 노출하지 않는다. Phase 1 live_trades는 표시 용도로만
    // 쓰이므로, 세션 초기 자본을 체결가로 나눈 러프한 값을 저장한다.
    // (Phase 4 실전 주문 시에는 실제 체결량이 별도 경로로 기록된다.)
    let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
    if new_count > 0 {
        let offset = result.trades.len() - new_count;
        let fee_rate = params.v3_fee_rate;
        for t in &result.trades[offset..] {
            let rough_volume = if t.buy_price > 0.0 {
                session.initial_capital / t.buy_price
            } else {
                0.0
            };
            // Insert buy row
            live_repo::insert_trade(
                &conn, session.id, &t.buy_timestamp, "buy",
                t.buy_price, rough_volume,
                t.buy_price * rough_volume * fee_rate,
                &t.buy_signal, None, None, false,
            ).map_err(|e| -> BoxErr { e.to_string().into() })?;
            // Insert sell row — pnl/pnl_pct
            live_repo::insert_trade(
                &conn, session.id, &t.sell_timestamp, "sell",
                t.sell_price, rough_volume,
                t.sell_price * rough_volume * fee_rate,
                &t.sell_signal,
                Some((t.sell_price - t.buy_price) * rough_volume),
                Some(t.pnl_pct),
                false,
            ).map_err(|e| -> BoxErr { e.to_string().into() })?;
        }
    }

    // 4. Current position snapshot from result.last_*.
    let current_position = if result.last_position == 1 { "holding" } else { "idle" };
    let (cbp, cbv) = if result.last_position == 1 {
        (Some(result.last_buy_price), Some(result.last_set_volume))
    } else {
        (None, None)
    };
    // Equity = initial × ∏(1 + pnl_pct − 2·fee) over closed trades.
    // 단순화: bar 기반 누적 폐기 → trade 기반. holding 중 미실현은 프런트엔드
    // (deriveSessionPnl)가 tick으로 처리하므로 이중 계상 없음.
    let fee = params.v3_fee_rate;
    let equity = result.trades.iter().fold(session.initial_capital, |acc, t| {
        acc * (1.0 + t.pnl_pct - 2.0 * fee)
    });

    // 5. Update session + equity snapshot.
    let last_candle_ts = data.last()
        .map(|md| md.candle.timestamp.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    live_repo::update_session_cycle(
        &conn, session.id, &last_candle_ts, &result.last_signal_type,
        current_position, cbp, cbv, equity,
    ).map_err(|e| -> BoxErr { e.to_string().into() })?;
    live_repo::upsert_equity(&conn, session.id, &last_candle_ts, equity, current_position)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;

    // Persist the same simulation's signal_log so the frontend chart strip
    // reads the canonical per-candle signal sequence — guaranteed to align
    // with the trades inserted above (both come from `result`).
    let log_json = serde_json::to_string(&result.signal_log).unwrap_or_else(|_| "[]".into());
    live_repo::update_session_signal_log(&conn, session.id, &log_json)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;

    Ok(SessionCycleOutput {
        session_id: session.id,
        new_completed_trades: new_count,
        latest_signal: result.last_signal_type,
        current_position: current_position.into(),
        current_equity: equity,
    })
}

#[cfg(test)]
mod tests {
    use crate::db::live_repo::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/002_users.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/006_live_trading.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/007_preset_context.sql")).unwrap();
        conn.execute_batch(include_str!("../../migrations/008_session_signal_log.sql")).unwrap();
        conn
    }

    /// Unit-level test: diff behaviour with a mocked trade sequence.
    /// (run_session_cycle 전체는 Upbit API + 전략이 얽혀 있어 통합 테스트로 분리)
    #[test]
    fn test_diff_inserts_only_new_trades() {
        let conn = setup_conn();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy",  3e6, 0.3, 450.0, "buy",  None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0), Some(3.0), false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "buy",  3.2e6, 0.3, 480.0, "buy",  None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T04:00:00Z", "sell", 3.3e6, 0.3, 495.0, "sell", Some(2.5), Some(2.5), false).unwrap();

        let existing = count_completed_trades(&conn, sid).unwrap();
        assert_eq!(existing, 2);
    }
}
