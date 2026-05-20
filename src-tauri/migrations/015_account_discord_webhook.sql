-- 015_account_discord_webhook.sql
-- 계정마다 별도 디스코드 webhook을 설정할 수 있게 함.
-- NULL은 fallback — `notification_configs`의 글로벌 webhook을 사용.

-- ALTER ADD COLUMN은 idempotent가 아니므로 schema.rs에서 "duplicate column" 에러 swallow 필요.
ALTER TABLE upbit_accounts
    ADD COLUMN discord_webhook_url TEXT;
