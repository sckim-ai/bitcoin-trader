-- Phase 4A.5: pending order tracking.
--
-- Why:
--   Upbit market orders are usually filled immediately, but the immediate
--   API response can return state="wait" with executed_volume=0 — the actual
--   match settles a few seconds later. Limit orders may stay in "wait"
--   indefinitely (book gap). In both cases we need to:
--     (1) follow up with /v1/order?uuid=... on subsequent cycles
--     (2) cancel after a stale timeout (default 1h) and let the strategy
--         retry on the next signal
--
-- Lifecycle:
--   wait   — placed but not confirmed done by Upbit
--   done   — fully filled (live_trades(is_real=1) is the source of truth
--            for the actual fill numbers; this row is now historical)
--   cancel — cancelled either explicitly (kill switch) or by the timeout

CREATE TABLE IF NOT EXISTS pending_orders (
  uuid          TEXT PRIMARY KEY,
  session_id    INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  side          TEXT NOT NULL,    -- "bid" | "ask"
  market        TEXT NOT NULL,
  ord_type      TEXT NOT NULL,    -- "price" | "market" | "limit"
  target_price  REAL,             -- limit only; NULL for market orders
  requested     REAL NOT NULL,    -- bid → KRW amount, ask → coin volume
  placed_at     TEXT NOT NULL,    -- RFC3339 — used for timeout calc
  status        TEXT NOT NULL DEFAULT 'wait',
  last_checked  TEXT,             -- RFC3339, last get_order timestamp
  resolved_at   TEXT              -- when status flipped to done/cancel
);

CREATE INDEX IF NOT EXISTS idx_pending_orders_session_status
  ON pending_orders(session_id, status);
CREATE INDEX IF NOT EXISTS idx_pending_orders_status_placed
  ON pending_orders(status, placed_at);
