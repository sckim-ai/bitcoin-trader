-- 017_session_notify_accounts.sql
-- Paper 세션 알림 채널을 N개 계정으로 fan-out 가능하게 함.
-- 값: JSON array of upbit_accounts.id (예: "[1,3]"). 빈 array "[]" = 알림 off.
-- 016에서 도입한 boolean notify_discord 컬럼은 더 이상 참조하지 않음(컬럼 자체는
-- backward-compat 위해 유지하되 새 로직은 notify_account_ids만 본다).
-- Real 세션은 이 컬럼을 무시한다 (기존대로 자기 계정 webhook만 사용).

ALTER TABLE live_sessions
    ADD COLUMN notify_account_ids TEXT NOT NULL DEFAULT '[]';
