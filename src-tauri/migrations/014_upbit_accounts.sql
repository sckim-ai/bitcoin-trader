-- 014_upbit_accounts.sql
-- 여러 Upbit 계정을 1급 엔티티로 분리. 1 계정 = 1 running real 세션.

CREATE TABLE IF NOT EXISTS upbit_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    label TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, label)
);

-- ALTER ADD COLUMN은 idempotent가 아니므로 schema.rs에서 "duplicate column" 에러 swallow 필요.
ALTER TABLE live_sessions
    ADD COLUMN upbit_account_id INTEGER REFERENCES upbit_accounts(id) ON DELETE SET NULL;

-- 같은 계정에 running real 세션은 1개만. paper는 N개 허용.
CREATE UNIQUE INDEX IF NOT EXISTS idx_session_account_running_real
    ON live_sessions(upbit_account_id)
    WHERE status = 'running' AND mode = 'real' AND upbit_account_id IS NOT NULL;
