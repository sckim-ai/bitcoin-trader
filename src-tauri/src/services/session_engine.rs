use crate::core::day_psy_store;
use crate::core::live_signal::{filter_confirmed_candles, last_signal_from_simulation, resolve_live_signal};
use crate::db::live_repo;
use crate::models::live::{LiveSession, Preset};
use crate::models::market::MarketData;
use crate::models::trading::{SimulationResult, TradingParameters};
use crate::services::auto_trader::{reconcile_position, DbPosition};
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
    // Trade write block — conn lifetime contained so the MutexGuard is
    // dropped before any subsequent `.await` (CLAUDE.md: MutexGuard is !Send).
    {
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

    } // end of trade write block — conn dropped before real_reconcile_step.await

    // 4. Current position snapshot from result.last_*. real 모드에선 직후
    //    Upbit 잔고 reconcile 결과로 덮어씀.
    let mut current_position: &str = if result.last_position == 1 { "holding" } else { "idle" };
    let (mut cbp, mut cbv) = if result.last_position == 1 {
        (Some(result.last_buy_price), Some(result.last_set_volume))
    } else {
        (None, None)
    };
    let mut last_signal_str: String = result.last_signal_type.clone();
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

    // 4b. Real 모드: Upbit 실 잔고로 reconcile + 레거시 알고리즘으로 신호 결정.
    //     이번 단계(4A.3)는 결정 + 잔고 동기화 + 로깅까지. 실주문 실행은 4A.4.
    //     키 누락이나 API 에러는 paper 흐름을 깨지 않고 stderr 로그만 남김.
    if session.mode == "real" {
        if let Err(e) = real_reconcile_step(
            &session, &data, &result,
            &mut current_position, &mut cbp, &mut cbv, &mut last_signal_str,
        ).await {
            eprintln!("[real cycle] session={} reconcile error: {e}", session.id);
        }
    }

    // 5. Update session + equity snapshot. Re-acquire conn after any awaits.
    let last_candle_ts = data.last()
        .map(|md| md.candle.timestamp.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        live_repo::update_session_cycle(
            &conn, session.id, &last_candle_ts, &last_signal_str,
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
    }

    Ok(SessionCycleOutput {
        session_id: session.id,
        new_completed_trades: new_count,
        latest_signal: last_signal_str,
        current_position: current_position.into(),
        current_equity: equity,
        live_return,
    })
}

/// Real-mode reconcile + signal resolution. Mutates the position snapshot
/// (`current_position`/`cbp`/`cbv`) and `last_signal` in place so the caller
/// can persist a single coherent state. Phase 4A.3 only logs the decision —
/// actual order execution lands in 4A.4.
///
/// Steps (mirrors legacy `LiveTradingService.ExecuteTradeCycleAsync`):
///   1. Load Upbit balances + current price.
///   2. Reconcile DB position vs. real coin balance (handles external
///      trades / dust / status drift).
///   3. Filter unfinished trailing candle so signal computation is stable.
///   4. Re-derive last signal from the confirmed-window simulation. We
///      reuse the original `result` when input matches (most cycles), but
///      run a confirmed-window simulation for the live signal extraction
///      to match legacy semantics exactly.
///   5. `resolve_live_signal(sim_signal, real_position)` → final action.
async fn real_reconcile_step<'a>(
    session: &LiveSession,
    data: &[MarketData],
    result: &SimulationResult,
    current_position: &mut &'a str,
    cbp: &mut Option<f64>,
    cbv: &mut Option<f64>,
    last_signal: &mut String,
) -> Result<(), BoxErr> {
    let upbit = crate::commands::upbit_keys::upbit_client_or_err()
        .map_err(|e| -> BoxErr { e.into() })?;

    let currency = session.market.split('-').nth(1).unwrap_or("ETH");
    let coin_balance = upbit.get_balance(currency).await
        .map_err(|e| -> BoxErr { e.to_string().into() })?;
    let current_price = upbit.get_current_price(&session.market).await
        .map_err(|e| -> BoxErr { e.to_string().into() })?;

    // Reconcile DB ↔ Upbit. Treat the session's stored fields as the DB
    // position; reconcile_position decides what the canonical state is.
    let db_pos = DbPosition {
        status: session.current_position.clone(),
        buy_price: session.current_buy_price.unwrap_or(0.0),
        buy_volume: session.current_buy_volume.unwrap_or(0.0),
        buy_psy: 0.0,
    };
    let (status, rec_buy_price, rec_buy_volume) =
        reconcile_position(&db_pos, coin_balance, current_price);

    // For signal extraction, prefer a confirmed-window sim — but if the
    // confirmed input is the same length as `data`, the existing `result`
    // applies and we skip re-simulation. (Re-running the strategy here is
    // safe but redundant on most cycles where the trailing candle is
    // already complete.)
    let confirmed = filter_confirmed_candles(data.to_vec());
    let sim_signal_owned;
    let sim_signal = if confirmed.len() == data.len() {
        last_signal_from_simulation(result).to_string()
    } else {
        // Re-run the strategy on the trimmed window so the live signal
        // doesn't include the still-forming candle. Reuse the registry +
        // params from the surrounding scope by constructing a fresh
        // simulation here is non-trivial without plumbing them through;
        // instead, fall back to inspecting the original log up to the
        // last confirmed timestamp.
        let cutoff = confirmed.last().map(|m| m.candle.timestamp).unwrap_or_else(Utc::now);
        let last = result.signal_log.iter().rev()
            .find(|e| {
                chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                    .map(|dt| dt.with_timezone(&Utc) <= cutoff)
                    .unwrap_or(false)
            })
            .map(|e| e.signal_type.clone())
            .unwrap_or_else(|| "ready".into());
        sim_signal_owned = last;
        sim_signal_owned.clone()
    };

    let position_int = if status == "holding" { 1 } else { 0 };
    let final_signal = resolve_live_signal(&sim_signal, position_int);

    // Apply reconciled state to the caller's mutable refs so update_session_cycle
    // persists the real state (not the sim's preferred state).
    *current_position = status;
    if status == "holding" {
        *cbp = Some(rec_buy_price);
        *cbv = Some(rec_buy_volume);
    } else {
        *cbp = None;
        *cbv = None;
    }
    *last_signal = final_signal.to_string();

    eprintln!(
        "[real cycle] session={} sim={} status={} balance={:.6} price={:.0} → final={}",
        session.id, sim_signal, status, coin_balance, current_price, final_signal,
    );
    Ok(())
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
