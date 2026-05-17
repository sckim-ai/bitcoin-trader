//! Pending order reconciliation. Runs at the start of every real-mode cycle
//! to follow up on orders that came back as `state="wait"` (or unknown) so
//! their final fill state is reflected in the session.
//!
//! Two timeouts:
//!   - `STALE_AFTER`: how long an order can stay "wait" before we explicitly
//!     cancel it. Default 1h — past this point the price has likely moved
//!     enough that the strategy would no longer want this entry/exit.
//!   - `MIN_RETRY_INTERVAL`: minimum gap between successive get_order checks
//!     for the same uuid (defends Upbit rate limits).

use crate::api::upbit::UpbitClient;
use crate::db::live_repo;
use crate::db::live_repo::PendingOrder;
use chrono::{DateTime, Duration, Timelike, Utc};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Compute the trade-row ts for a late fill: the strategy's transition bar
/// (one full bar BEFORE the cycle that placed the order, floored to the bar
/// boundary). Aligns the R marker on the same candle as the paper marker.
///
/// `placed_at` is the wall-clock when the order was sent — typically the
/// hour cycle's entry +30s. Subtracting 1h and flooring gives the previous
/// bar's start, which is the bar the strategy actually evaluated.
pub fn trigger_bar_ts(placed_at: &str) -> Option<String> {
    let placed = DateTime::parse_from_rfc3339(placed_at).ok()?
        .with_timezone(&Utc);
    let trigger = placed - Duration::hours(1);
    let floored = trigger.with_minute(0)?
        .with_second(0)?
        .with_nanosecond(0)?;
    Some(floored.to_rfc3339())
}

/// 1 hour — a stale `wait` order beyond this is auto-cancelled.
pub const STALE_AFTER: Duration = Duration::hours(1);

/// Internal reconcile loop. Processes a pre-fetched list of pending orders
/// using the given UpbitClient. One bad uuid is logged and skipped — it
/// never aborts the whole pass.
async fn reconcile_inner(
    db: &Arc<Mutex<Connection>>,
    upbit: &UpbitClient,
    pendings: Vec<PendingOrder>,
) -> Result<usize, BoxErr> {
    let now = Utc::now();
    let mut resolved = 0usize;
    for p in pendings {
        let placed = match DateTime::parse_from_rfc3339(&p.placed_at) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                crate::live_log!("[pending_tracker] bad placed_at on {}: {e}", p.uuid);
                continue;
            }
        };

        // Status check first — many "wait" orders flip to "done" within seconds.
        let order = match upbit.get_order(&p.uuid).await {
            Ok(o) => o,
            Err(e) => {
                crate::live_log!("[pending_tracker] get_order({}) failed: {e}", p.uuid);
                let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                let _ = live_repo::touch_pending_check(&conn, &p.uuid, &now.to_rfc3339());
                continue;
            }
        };

        match order.state.as_str() {
            "done" => {
                // All DB work in a sub-scope so the MutexGuard drops before
                // the notifier .await below (CLAUDE.md: MutexGuard !Send).
                let executed = order.executed_volume_f64();
                // Actual volume-weighted average fill price from the Upbit
                // response (trades array → paid_fee fallback). Falls back to
                // the stored limit (p.target_price) only when neither source
                // has usable data. Last resort: order.price (limit) parsed as
                // f64. Computed BEFORE the DB scope so the notification path
                // below can use the same value as what's booked.
                let fee_rate = 0.0005;
                let target_price = p.target_price.unwrap_or(0.0);
                let booked_price = order
                    .avg_fill_price(fee_rate)
                    .or(if target_price > 0.0 { Some(target_price) } else { None })
                    .unwrap_or_else(|| {
                        order.price.as_deref()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0)
                    });
                let (notifier, session_for_notif) = {
                    let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                    // Atomic CAS: claim ownership of this row. await 경계에서 lock이
                    // 풀리는 동안 다른 세션의 cycle reconciler가 같은 wait row를 잡아
                    // 두 번 booking하던 race를 막는다 (phantom row 발생의 근본 원인).
                    // affected_rows == 0 이면 이미 다른 트래커가 처리한 row → skip.
                    let claimed = conn.execute(
                        "UPDATE pending_orders SET status='done', resolved_at=?1, last_checked=?1
                         WHERE uuid=?2 AND status='wait'",
                        rusqlite::params![now.to_rfc3339(), p.uuid],
                    ).unwrap_or(0);
                    if claimed == 0 {
                        crate::live_log!(
                            "[pending_tracker] DONE skip {} — already claimed by another cycle",
                            p.uuid,
                        );
                        continue;
                    }
                    if executed > 0.0 {
                        let session = live_repo::get_session(&conn, p.session_id)
                            .map_err(|e| -> BoxErr { e.to_string().into() })?;
                        // Late fill ts → strategy의 transition 봉 timestamp.
                        // paper marker가 그 봉에 박히므로, R 마커도 같은 봉에 stack.
                        let booked_ts = trigger_bar_ts(&p.placed_at)
                            .unwrap_or_else(|| now.to_rfc3339());
                        if let Some(s) = session {
                            // signal 명을 _late로 구분해 same-cycle synchronous 경로
                            // (real_buy/real_sell)와의 더블북을 SQL에서 식별 가능하게.
                            if p.side == "bid" {
                                let _ = live_repo::insert_trade(
                                    &conn, p.session_id, &booked_ts, "buy",
                                    booked_price, executed,
                                    booked_price * executed * fee_rate,
                                    "real_buy_late", None, None, true,
                                );
                                // 분할 매수 누적: 같은 매수 결정의 sync done이
                                // 이미 booked되어 있으면 session에 그 가격/수량이
                                // 남아있다 → late chunk와 가중평균하여 평균진입가 보존.
                                let (prev_p, prev_v) = if s.current_position == "holding" {
                                    (s.current_buy_price, s.current_buy_volume)
                                } else {
                                    (None, None)
                                };
                                let (avg_price, avg_volume) =
                                    crate::services::session_engine::weighted_avg_buy(
                                        prev_p, prev_v, booked_price, executed,
                                    );
                                let _ = live_repo::update_session_cycle(
                                    &conn, p.session_id,
                                    s.last_cycle_ts.as_deref().unwrap_or(""),
                                    s.last_signal.as_deref().unwrap_or(""),
                                    "holding",
                                    Some(avg_price), Some(avg_volume),
                                    s.current_equity.unwrap_or(s.initial_capital),
                                    s.live_return,
                                );
                            } else if p.side == "ask" {
                                // Cost basis from the snapshot frozen at SELL
                                // placement time (migration 013). Falls back to
                                // current session state for legacy rows that
                                // pre-date the snapshot column.
                                let buy_price = p.cost_basis_price
                                    .or(s.current_buy_price)
                                    .unwrap_or(0.0);
                                let pnl = if buy_price > 0.0 {
                                    (booked_price - buy_price) * executed
                                } else { 0.0 };
                                // 분수형으로 저장 (paper 전략 / sync real_sell과 단위 일치).
                                // 차트는 일률 ×100 표시.
                                let pnl_pct = if buy_price > 0.0 {
                                    (booked_price - buy_price) / buy_price
                                } else { 0.0 };
                                let _ = live_repo::insert_trade(
                                    &conn, p.session_id, &booked_ts, "sell",
                                    booked_price, executed,
                                    booked_price * executed * fee_rate,
                                    "real_sell_late", Some(pnl), Some(pnl_pct), true,
                                );
                                // 분할 매도(보통 3 청크)에서 한 청크가 처리되어도
                                // 남은 청크들이 같은 cost basis 로 P/L 계산할 수
                                // 있도록 current_buy_price 를 보존한다. 잔량이
                                // MIN_ORDER 미만일 때만 idle 로 전이 — 동기 매도
                                // 경로(session_engine.rs)와 동일한 가드.
                                let prev_volume = s.current_buy_volume.unwrap_or(0.0);
                                let remaining = (prev_volume - executed).max(0.0);
                                let still_holding = remaining * booked_price
                                    >= crate::services::order_executor::MIN_ORDER_KRW;
                                let (new_status, new_price, new_volume) = if still_holding {
                                    ("holding", s.current_buy_price, Some(remaining))
                                } else {
                                    ("idle", None, None)
                                };
                                let _ = live_repo::update_session_cycle(
                                    &conn, p.session_id,
                                    s.last_cycle_ts.as_deref().unwrap_or(""),
                                    s.last_signal.as_deref().unwrap_or(""),
                                    new_status,
                                    new_price, new_volume,
                                    s.current_equity.unwrap_or(s.initial_capital),
                                    s.live_return,
                                );
                                crate::live_log!(
                                    "[pending_tracker] LATE SELL booked {:.8} @ {:.0} (P/L: {:.2}%, remaining={:.8}, status={})",
                                    executed, booked_price, pnl_pct * 100.0, remaining, new_status,
                                );
                            }
                        }
                    }
                    // mark_pending_resolved는 위의 atomic CAS가 이미 수행함 (status='done').

                    let notifier = crate::notifications::manager::NotificationManager::from_db(&conn, 1);
                    let session_for_notif = live_repo::get_session(&conn, p.session_id)
                        .ok().flatten();
                    (notifier, session_for_notif)
                }; // ← MutexGuard dropped here
                // Apply account prefix after the lock is released.
                let notifier = notifier.with_account_label(
                    session_for_notif.as_ref().and_then(|s| s.account_label.as_deref()),
                );

                resolved += 1;
                crate::live_log!("[pending_tracker] resolved DONE {} (executed={:.8})",
                    p.uuid, executed);

                if executed > 0.0 {
                    // Use the same booked_price that was inserted into
                    // live_trades above — guarantees DB row and Discord
                    // message agree.
                    let label = session_for_notif.as_ref()
                        .map(|s| s.label.clone())
                        .unwrap_or_else(|| format!("session {}", p.session_id));
                    let note = format!("late fill (placed {})", p.placed_at);
                    let ctx = crate::notifications::manager::TradeContext {
                        session_label: Some(&label),
                        note: Some(&note),
                        ..Default::default()
                    };
                    if p.side == "bid" {
                        notifier.notify_trade_embed(
                            "buy", &p.market, booked_price, executed, None, &ctx, true,
                        ).await;
                    } else if p.side == "ask" {
                        let buy_price = session_for_notif.as_ref()
                            .and_then(|s| s.current_buy_price).unwrap_or(0.0);
                        let pnl_pct = if buy_price > 0.0 {
                            (booked_price - buy_price) / buy_price * 100.0
                        } else { 0.0 };
                        notifier.notify_trade_embed(
                            "sell", &p.market, booked_price, executed, Some(pnl_pct), &ctx, true,
                        ).await;
                    }
                }
                continue;
            }
            "cancel" => {
                let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                live_repo::mark_pending_resolved(&conn, &p.uuid, "cancel", &now.to_rfc3339())
                    .map_err(|e| -> BoxErr { e.to_string().into() })?;
                resolved += 1;
                crate::live_log!("[pending_tracker] resolved CANCEL {} (externally cancelled)", p.uuid);
            }
            _ => {
                // Still 'wait' (or unknown). Cancel if past stale threshold.
                if now - placed > STALE_AFTER {
                    crate::live_log!(
                        "[pending_tracker] STALE {} ({}h+) — sending cancel",
                        p.uuid,
                        STALE_AFTER.num_hours(),
                    );
                    match upbit.cancel_order(&p.uuid).await {
                        Ok(_) => {
                            let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                            live_repo::mark_pending_resolved(
                                &conn, &p.uuid, "cancel", &now.to_rfc3339(),
                            ).map_err(|e| -> BoxErr { e.to_string().into() })?;
                            resolved += 1;
                        }
                        Err(e) => {
                            crate::live_log!("[pending_tracker] cancel({}) failed: {e}", p.uuid);
                            let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                            let _ = live_repo::touch_pending_check(&conn, &p.uuid, &now.to_rfc3339());
                        }
                    }
                } else {
                    let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                    let _ = live_repo::touch_pending_check(&conn, &p.uuid, &now.to_rfc3339());
                }
            }
        }
    }
    Ok(resolved)
}

/// Drive one reconcile pass across ALL sessions: scan pending_orders.status='wait',
/// call get_order for each, and either mark resolved (done/cancel) or attempt
/// timeout cancellation. Errors on individual orders are logged and skipped
/// — one bad uuid shouldn't prevent the rest from being checked.
///
/// NOTE: still used by session_engine.rs until Task 12 migrates the call site
/// to `reconcile_pending_orders_for_session`.
#[allow(dead_code)]
pub async fn reconcile_pending_orders(
    db: &Arc<Mutex<Connection>>,
    upbit: &UpbitClient,
) -> Result<usize, BoxErr> {
    let pendings = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        live_repo::list_pending_wait(&conn)
            .map_err(|e| -> BoxErr { e.to_string().into() })?
    };
    if pendings.is_empty() {
        return Ok(0);
    }
    reconcile_inner(db, upbit, pendings).await
}

/// Per-session reconcile entrypoint for multi-account safety. Fetches only
/// THIS session's pending orders and uses the account's own UpbitClient so
/// that account A's client never touches account B's order UUIDs.
///
/// Returns Ok(0) on empty pendings or if the account's keys aren't configured
/// (logs the error but does not propagate — a missing key config must not
/// crash the whole engine cycle).
pub async fn reconcile_pending_orders_for_session(
    db: &Arc<Mutex<Connection>>,
    session_id: i64,
    upbit_account_id: i64,
) -> Result<usize, BoxErr> {
    let pendings = {
        let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
        live_repo::list_session_pending_wait(&conn, session_id)
            .map_err(|e| -> BoxErr { e.to_string().into() })?
    };
    if pendings.is_empty() {
        return Ok(0);
    }
    let upbit = match crate::commands::upbit_keys::upbit_client_for(upbit_account_id) {
        Ok(c) => c,
        Err(e) => {
            crate::live_log!(
                "[pending_tracker] session {} — cannot build client for account {}: {e}",
                session_id, upbit_account_id,
            );
            return Ok(0);
        }
    };
    reconcile_inner(db, &upbit, pendings).await
}
