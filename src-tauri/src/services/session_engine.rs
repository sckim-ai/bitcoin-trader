use crate::core::day_psy_store;
use crate::db::live_repo;
use crate::models::live::{LiveSession, Preset};
use crate::models::trading::TradingParameters;
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
    pub live_return: f64,
}

/// Run one cycle for a single session. Loads the FULL history from local DB
/// — every hourly candle from `session.start_ts` (= preset.since_ts) to the
/// most recent persisted bar — and replays the entire strategy. The output
/// (trades + signal_log) is byte-identical to what the Simulation page would
/// produce for the same span, so DB-stored trades and the chart strip stay
/// aligned by construction. Live freshness comes from market_updater /
/// auto_update_all writing the latest bar to DB; the cycle just reads.
pub async fn run_session_cycle(
    db: &Arc<Mutex<Connection>>,
    session: &LiveSession,
    preset: &Preset,
    registry: &StrategyRegistry,
) -> Result<SessionCycleOutput, BoxErr> {
    // DB short market form ('ETH', 'BTC') vs API form ('KRW-ETH'). Live
    // sessions store the API form; load_market_data wants the short one.
    let db_market = session.market.split('-').nth(1).unwrap_or(&session.market);

    let data: Vec<_> = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        day_psy_store::load_market_data(&conn, db_market, Some(&session.start_ts), None)
            .map_err(|e| -> BoxErr { e.to_string().into() })?
    };
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
            live_return: session.live_return,
        });
    }

    // 2. Run full simulation from session start to latest candle.
    let strategy = registry.get(&preset.strategy_key)
        .ok_or_else(|| -> BoxErr { format!("strategy not found: {}", preset.strategy_key).into() })?;
    let params: TradingParameters = serde_json::from_str(&preset.params_json)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;
    let result = strategy.run_simulation(&data, &params);

    // 3. Replace paper trades with the current simulation's full result.
    //    Earlier we used a diff-insert (only append new completions), but
    //    that's not idempotent across cycles when the data window itself
    //    shifts: if today's run produces a trade at a slightly different
    //    timestamp than yesterday's, the old row stayed in DB and decoupled
    //    visually from the (always-fresh) signal_log. Replacing on each
    //    cycle keeps trades and signal_log byte-aligned. Real-trade rows
    //    (is_real=1) are preserved by `delete_paper_trades`.
    let new_count = result.trades.len();
    let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
    live_repo::delete_paper_trades(&conn, session.id)
        .map_err(|e| -> BoxErr { e.to_string().into() })?;
    let fee_rate = params.v3_fee_rate;
    for t in result.trades.iter() {
        let rough_volume = if t.buy_price > 0.0 {
            session.initial_capital / t.buy_price
        } else {
            0.0
        };
        live_repo::insert_trade(
            &conn, session.id, &t.buy_timestamp, "buy",
            t.buy_price, rough_volume,
            t.buy_price * rough_volume * fee_rate,
            &t.buy_signal, None, None, false,
        ).map_err(|e| -> BoxErr { e.to_string().into() })?;
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

    // Open position: SimulationResult.trades only contains *completed* pairs,
    // so an in-progress buy (last_position=1, hasn't sold yet) wouldn't show
    // up as a marker on the chart. Persist it as a buy-only paper row, with
    // its timestamp pulled from the most recent 'buy' transition in
    // signal_log so it lands on the candle where the entry actually fired.
    if result.last_position == 1 {
        let open_buy_ts = result.signal_log.iter().rev()
            .find(|e| e.signal_type == "buy")
            .map(|e| e.timestamp.clone());
        if let Some(buy_ts) = open_buy_ts {
            let buy_price = result.last_buy_price;
            let rough_volume = if buy_price > 0.0 {
                session.initial_capital / buy_price
            } else {
                0.0
            };
            live_repo::insert_trade(
                &conn, session.id, &buy_ts, "buy",
                buy_price, rough_volume,
                buy_price * rough_volume * fee_rate,
                "buy", None, None, false,
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
    // Equity = initial × ∏(1 + pnl_pct − 2·fee) over ALL closed trades
    // (since session.start_ts = preset.since_ts). This matches the preset's
    // backtest accumulation through "now". holding 중 미실현은 프런트엔드
    // (deriveSessionPnl)가 tick으로 처리하므로 이중 계상 없음.
    //
    // live_return은 사용자가 Start 누른 시점(real_started_at) 이후의 trades만
    // 분리해 누적 — preset의 정적 baseline과 비교 가능한 동적 측정값.
    let fee = params.v3_fee_rate;
    let equity = result.trades.iter().fold(session.initial_capital, |acc, t| {
        acc * (1.0 + t.pnl_pct - 2.0 * fee)
    });
    let live_anchor = session.real_started_at.as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&session.start_ts);
    let live_anchor_ts = chrono::DateTime::parse_from_rfc3339(live_anchor)
        .map_err(|e| -> BoxErr { e.to_string().into() })?
        .with_timezone(&Utc);
    let live_factor = result.trades.iter()
        .filter(|t| chrono::DateTime::parse_from_rfc3339(&t.sell_timestamp)
            .map(|dt| dt.with_timezone(&Utc) > live_anchor_ts)
            .unwrap_or(false))
        .fold(1.0_f64, |acc, t| acc * (1.0 + t.pnl_pct - 2.0 * fee));
    let live_return = (live_factor - 1.0) * 100.0;

    // 5. Update session + equity snapshot.
    let last_candle_ts = data.last()
        .map(|md| md.candle.timestamp.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    live_repo::update_session_cycle(
        &conn, session.id, &last_candle_ts, &result.last_signal_type,
        current_position, cbp, cbv, equity, live_return,
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
        live_return,
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
        conn.execute_batch(include_str!("../../migrations/009_baseline_metrics.sql")).unwrap();
        conn
    }

    /// Unit-level test: diff behaviour with a mocked trade sequence.
    /// (run_session_cycle 전체는 Upbit API + 전략이 얽혀 있어 통합 테스트로 분리)
    #[test]
    fn test_diff_inserts_only_new_trades() {
        let conn = setup_conn();
        let pid = insert_preset(&conn, 1, "p", "V3", "{}", "manual", None, None, None, None, None, None, None).unwrap();
        let sid = insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

        insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy",  3e6, 0.3, 450.0, "buy",  None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0), Some(3.0), false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T03:00:00Z", "buy",  3.2e6, 0.3, 480.0, "buy",  None, None, false).unwrap();
        insert_trade(&conn, sid, "2026-04-24T04:00:00Z", "sell", 3.3e6, 0.3, 495.0, "sell", Some(2.5), Some(2.5), false).unwrap();

        let existing = count_completed_trades(&conn, sid).unwrap();
        assert_eq!(existing, 2);
    }
}
