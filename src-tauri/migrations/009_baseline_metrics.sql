-- Phase 3: Baseline (preset) vs Live (since real_started_at) 분리.
--
-- Why:
--   라이브 화면은 두 정보를 함께 보여줘야 함 — preset의 백테스트 메트릭(정적
--   기준선)과 사용자가 Start 누른 이후 발생한 실제 매매의 누적 수익률(동적).
--   기존 schema는 (1) preset에 백테스트 메트릭을 저장할 자리가 없었고
--   (2) live_sessions의 current_equity가 since~now 통합 결과만 가져
--   "라이브 분만"을 분리할 수 없었음.
--
-- What:
--   - presets.baseline_return / baseline_trades  : preset 저장 시점의 백테스트 결과
--   - live_sessions.live_return                 : real_started_at 이후 trades 누적 %
--   - 기존 세션 backfill: real_started_at NULL/빈값 → created_at으로 채움
--     (이 처리가 없으면 새 session_engine 코드가 NULL을 만나 "전체가 라이브"로 잘못 잡음)

ALTER TABLE presets ADD COLUMN baseline_return REAL;
ALTER TABLE presets ADD COLUMN baseline_trades INTEGER;

ALTER TABLE live_sessions ADD COLUMN live_return REAL NOT NULL DEFAULT 0;

UPDATE live_sessions
   SET real_started_at = created_at
 WHERE real_started_at IS NULL OR real_started_at = '';
