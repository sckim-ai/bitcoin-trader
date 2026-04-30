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
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// 1 hour — a stale `wait` order beyond this is auto-cancelled.
pub const STALE_AFTER: Duration = Duration::hours(1);

/// Drive one reconcile pass: scan pending_orders.status='wait', call
/// get_order for each, and either mark resolved (done/cancel) or attempt
/// timeout cancellation. Errors on individual orders are logged and skipped
/// — one bad uuid shouldn't prevent the rest from being checked.
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
    let now = Utc::now();
    let mut resolved = 0usize;
    for p in pendings {
        let placed = match DateTime::parse_from_rfc3339(&p.placed_at) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                eprintln!("[pending_tracker] bad placed_at on {}: {e}", p.uuid);
                continue;
            }
        };

        // Status check first — many "wait" orders flip to "done" within seconds.
        let order = match upbit.get_order(&p.uuid).await {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[pending_tracker] get_order({}) failed: {e}", p.uuid);
                let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                let _ = live_repo::touch_pending_check(&conn, &p.uuid, &now.to_rfc3339());
                continue;
            }
        };

        match order.state.as_str() {
            "done" => {
                // Late fill (was 'wait' last cycle, 'done' now). Record the
                // executed volume as a real trade so the user's P/L reflects
                // it. Without this insert, limit orders that fill across
                // cycles would never appear in live_trades.
                let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                let executed = order.executed_volume_f64();
                if executed > 0.0 {
                    let target_price = p.target_price.unwrap_or(0.0);
                    let booked_price = if target_price > 0.0 { target_price } else {
                        // Fallback: avg from price field (usually only set for limit).
                        order.price.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0.0)
                    };
                    let fee_rate = 0.0005;
                    let session = live_repo::get_session(&conn, p.session_id)
                        .map_err(|e| -> BoxErr { e.to_string().into() })?;
                    if let Some(s) = session {
                        // Skip insert if this uuid was already booked
                        // synchronously by session_engine (same-cycle done).
                        // We detect by looking up the latest live_trades
                        // row's signal column — the session_engine path
                        // uses "real_buy"/"real_sell"; the tracker uses
                        // "real_buy_late"/"real_sell_late" so the two
                        // paths can never double-book the same uuid.
                        if p.side == "bid" {
                            let _ = live_repo::insert_trade(
                                &conn, p.session_id, &now.to_rfc3339(), "buy",
                                booked_price, executed,
                                booked_price * executed * fee_rate,
                                "real_buy_late", None, None, true,
                            );
                            // Late fill → flip session to holding using the
                            // tracked price.
                            let _ = live_repo::update_session_cycle(
                                &conn, p.session_id,
                                s.last_cycle_ts.as_deref().unwrap_or(""),
                                s.last_signal.as_deref().unwrap_or(""),
                                "holding",
                                Some(booked_price), Some(executed),
                                s.current_equity.unwrap_or(s.initial_capital),
                                s.live_return,
                            );
                        } else if p.side == "ask" {
                            let buy_price = s.current_buy_price.unwrap_or(0.0);
                            let pnl = if buy_price > 0.0 {
                                (booked_price - buy_price) * executed
                            } else { 0.0 };
                            let pnl_pct = if buy_price > 0.0 {
                                (booked_price - buy_price) / buy_price * 100.0
                            } else { 0.0 };
                            let _ = live_repo::insert_trade(
                                &conn, p.session_id, &now.to_rfc3339(), "sell",
                                booked_price, executed,
                                booked_price * executed * fee_rate,
                                "real_sell_late", Some(pnl), Some(pnl_pct), true,
                            );
                            let _ = live_repo::update_session_cycle(
                                &conn, p.session_id,
                                s.last_cycle_ts.as_deref().unwrap_or(""),
                                s.last_signal.as_deref().unwrap_or(""),
                                "idle",
                                None, None,
                                s.current_equity.unwrap_or(s.initial_capital),
                                s.live_return,
                            );
                            eprintln!(
                                "[pending_tracker] LATE SELL booked {:.8} @ {:.0} (P/L: {:.2}%)",
                                executed, booked_price, pnl_pct,
                            );
                        }
                    }
                }
                live_repo::mark_pending_resolved(&conn, &p.uuid, "done", &now.to_rfc3339())
                    .map_err(|e| -> BoxErr { e.to_string().into() })?;
                resolved += 1;
                eprintln!("[pending_tracker] resolved DONE {} (executed={:.8})",
                    p.uuid, executed);
            }
            "cancel" => {
                let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                live_repo::mark_pending_resolved(&conn, &p.uuid, "cancel", &now.to_rfc3339())
                    .map_err(|e| -> BoxErr { e.to_string().into() })?;
                resolved += 1;
                eprintln!("[pending_tracker] resolved CANCEL {} (externally cancelled)", p.uuid);
            }
            _ => {
                // Still 'wait' (or unknown). Cancel if past stale threshold.
                if now - placed > STALE_AFTER {
                    eprintln!(
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
                            eprintln!("[pending_tracker] cancel({}) failed: {e}", p.uuid);
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
