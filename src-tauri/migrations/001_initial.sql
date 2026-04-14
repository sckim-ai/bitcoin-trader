CREATE TABLE IF NOT EXISTS market_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    market TEXT NOT NULL,
    timeframe TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    open REAL NOT NULL, high REAL NOT NULL, low REAL NOT NULL, close REAL NOT NULL,
    volume REAL NOT NULL,
    UNIQUE(market, timeframe, timestamp)
);
CREATE INDEX IF NOT EXISTS idx_market_data_lookup ON market_data(market, timeframe, timestamp);

CREATE TABLE IF NOT EXISTS indicators (
    market_data_id INTEGER PRIMARY KEY REFERENCES market_data(id),
    sma_10 REAL, sma_25 REAL, sma_60 REAL, rsi_14 REAL,
    macd REAL, macd_signal REAL, macd_hist REAL,
    bb_upper REAL, bb_middle REAL, bb_lower REAL,
    atr_14 REAL, adx_14 REAL, di_plus REAL, di_minus REAL,
    stoch_k REAL, stoch_d REAL, psy_hour REAL, psy_day REAL
);

CREATE TABLE IF NOT EXISTS strategy_configs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL DEFAULT 1,
    strategy_key TEXT NOT NULL, name TEXT NOT NULL,
    parameters TEXT NOT NULL, is_active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS positions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL DEFAULT 1,
    market TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'idle',
    buy_price REAL, buy_volume REAL, buy_psy REAL, buy_timestamp TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, market)
);

CREATE TABLE IF NOT EXISTS trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL DEFAULT 1,
    market TEXT NOT NULL, side TEXT NOT NULL, order_type TEXT NOT NULL,
    price REAL NOT NULL, volume REAL NOT NULL, fee REAL NOT NULL DEFAULT 0,
    strategy_key TEXT, signal_type TEXT, pnl REAL, pnl_pct REAL,
    executed_at TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_trades_user_time ON trades(user_id, executed_at);

CREATE TABLE IF NOT EXISTS optimization_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL DEFAULT 1,
    strategy_key TEXT NOT NULL, population_size INTEGER NOT NULL,
    generations INTEGER NOT NULL, objectives TEXT NOT NULL,
    constraints TEXT, status TEXT NOT NULL DEFAULT 'running',
    started_at TEXT NOT NULL DEFAULT (datetime('now')), completed_at TEXT
);

CREATE TABLE IF NOT EXISTS optimization_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER NOT NULL REFERENCES optimization_runs(id),
    generation INTEGER NOT NULL, rank INTEGER NOT NULL,
    parameters TEXT NOT NULL, total_return REAL, win_rate REAL,
    max_drawdown REAL, total_trades INTEGER, crowding_distance REAL
);
