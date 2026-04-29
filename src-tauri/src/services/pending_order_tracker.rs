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
                let conn = db.lock().map_err(|e| -> BoxErr { e.to_string().into() })?;
                live_repo::mark_pending_resolved(&conn, &p.uuid, "done", &now.to_rfc3339())
                    .map_err(|e| -> BoxErr { e.to_string().into() })?;
                resolved += 1;
                eprintln!("[pending_tracker] resolved DONE {} (executed={:.8})",
                    p.uuid, order.executed_volume_f64());
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
