-- Per-session BUY order cap. NULL means no cap (use full KRW balance × 0.9995).
-- Default NULL preserves current behaviour for existing sessions; new sessions
-- can opt into a hard cap via the New Session dialog.
ALTER TABLE live_sessions ADD COLUMN max_order_krw REAL;
