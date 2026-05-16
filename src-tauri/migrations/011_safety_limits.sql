-- Phase 4A.6: per-session daily safety limits.
--
-- max_daily_loss_pct: today's realized loss% (sum of pnl on is_real=1 sells
--                     since UTC midnight, divided by initial_capital × 100).
--                     If this drops below the configured threshold, the
--                     session auto-stops. Default -10% — a typical "circuit
--                     breaker" used in algo trading.
--
-- max_daily_trades:   number of is_real=1 sell rows since UTC midnight. If
--                     reached, the session auto-stops. Default 20 — covers
--                     normal V3-style strategies (~1 trade/day) with plenty
--                     of headroom while still catching runaway loops.
--
-- Both columns are NOT NULL with conservative defaults so existing sessions
-- (created before this migration) inherit safe limits without manual editing.

ALTER TABLE live_sessions ADD COLUMN max_daily_loss_pct REAL NOT NULL DEFAULT -10.0;
ALTER TABLE live_sessions ADD COLUMN max_daily_trades   INTEGER NOT NULL DEFAULT 20;
