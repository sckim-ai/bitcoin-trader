-- Phase 3: persist the signal_log produced by each session_engine cycle so
-- the frontend can render the per-candle signal strip without re-running
-- the strategy. Same SimulationResult → same trades AND same signals → no
-- alignment drift between the markers and the strip.

ALTER TABLE live_sessions ADD COLUMN signal_log_json TEXT;
