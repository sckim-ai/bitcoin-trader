-- Phase 1: Live trading multi-session schema.
-- Presets, sessions, trades, equity snapshots.

CREATE TABLE IF NOT EXISTS presets (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id       INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,
  strategy_key  TEXT NOT NULL,
  params_json   TEXT NOT NULL,
  source        TEXT NOT NULL DEFAULT 'manual',
  source_run_id INTEGER,
  created_at    TEXT NOT NULL,
  UNIQUE(user_id, name)
);

CREATE TABLE IF NOT EXISTS live_sessions (
  id                  INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id             INTEGER NOT NULL DEFAULT 1,
  label               TEXT NOT NULL,
  preset_id           INTEGER NOT NULL REFERENCES presets(id),
  market              TEXT NOT NULL,
  mode                TEXT NOT NULL DEFAULT 'paper',
  status              TEXT NOT NULL DEFAULT 'stopped',
  initial_capital     REAL NOT NULL,
  start_ts            TEXT NOT NULL,
  real_started_at     TEXT,
  last_cycle_ts       TEXT,
  last_signal         TEXT,
  current_position    TEXT NOT NULL DEFAULT 'idle',
  current_buy_price   REAL,
  current_buy_volume  REAL,
  current_equity      REAL,
  created_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_live_sessions_status ON live_sessions(status);

CREATE TABLE IF NOT EXISTS live_trades (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id   INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts           TEXT NOT NULL,
  side         TEXT NOT NULL,
  price        REAL NOT NULL,
  volume       REAL NOT NULL,
  fee          REAL NOT NULL,
  signal       TEXT NOT NULL,
  pnl          REAL,
  pnl_pct      REAL,
  is_real      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_live_trades_session ON live_trades(session_id, ts);

CREATE TABLE IF NOT EXISTS live_equity (
  session_id  INTEGER NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts          TEXT NOT NULL,
  equity      REAL NOT NULL,
  position    TEXT NOT NULL,
  PRIMARY KEY (session_id, ts)
);
