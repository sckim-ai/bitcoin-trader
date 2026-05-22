-- 016_session_notify_discord.sql
-- 세션별 Discord 알림 토글. real 세션은 기존대로 항상 알림이 가지만(이번 컬럼과 무관),
-- paper 세션은 notify_discord=1일 때만 상태 변경 알림이 디스코드로 전송된다.
-- 기본값 0 — paper 세션 N개가 동시에 돌 때 노이즈 방지.

ALTER TABLE live_sessions
    ADD COLUMN notify_discord INTEGER NOT NULL DEFAULT 0;
