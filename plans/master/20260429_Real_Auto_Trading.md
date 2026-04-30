# Real Auto-Trading (Phase 4A) Master Plan

> **Status (2026-04-30)**: 4A.1~4A.7 완료 + **post-4A.7 정책 변경 적용**.
>
> 사용자 요구로 다음 3가지 정책 변경:
> 1. **일일 손실 / 매매수 회로차단기 비활성화** — session_engine 0a 블록을 주석 처리. 가드 함수와 마이그레이션 011 컬럼은 향후 재활성화를 위해 보존. 자동 stop이 사라졌으므로 Kill switch(수동) + 1h pending timeout이 유일한 자동 안전벨트.
> 2. **시장가 → 지정가 (last bar's close)** — order_executor가 target_price > 0이면 `place_limit_*_typed` 호출. 매수 volume = `floor(KRW/target × 1e8) / 1e8`.
> 3. **다음 봉 close 재주문** — cycle 진입 시 같은 세션의 wait 주문 모두 cancel → 새 close에 다시 주문. tracker가 done 발견 시 `live_trades(real_*_late)` row 추가해 늦은 체결도 P/L에 반영.
>
> 위험 프로필 변화:
> - 시장가의 슬리피지 ↔ 지정가의 미체결 / 가격 추격으로 트레이드오프 이동
> - 자동 자본 보호 사라짐 — 수동 모니터링 책임 사용자에게
>
> 회귀 가드(safety_circuit_breaker_test 8 케이스)는 함수 자체를 테스트하므로 그대로 통과 — 0a 블록을 다시 활성화하면 즉시 작동.
> 자동 검증 진척: `cargo build --tests` 통과, `cum_return_parity` 5/5,
> 신규 통합 테스트 ([safety_circuit_breaker_test](../../src-tauri/tests/safety_circuit_breaker_test.rs)
> 8 케이스, [multi_real_test](../../src-tauri/tests/multi_real_test.rs) 8 케이스,
> [pending_orders_test](../../src-tauri/tests/pending_orders_test.rs) 5 케이스,
> [order_executor_test](../../src-tauri/tests/order_executor_test.rs) 11 케이스,
> [live_signal_test](../../src-tauri/tests/live_signal_test.rs) 13 케이스 — 총 45 케이스).
> 운용 시작 전 [Manual/real-trading-e2e-checklist.md](../../Manual/real-trading-e2e-checklist.md)
> 9개 섹션 모두 통과 필요.

## TL;DR

| 항목 | 내용 |
|---|---|
| **Quick Summary** | 기존 paper multi-session 인프라 위에 “1개 real 세션” 분기를 얹어 실제 Upbit 주문 실행. 레거시 C# `ResolveSignalFromSimulation` 알고리즘 정확 포팅으로 “시뮬 vs 실잔고” 모순 자동 차단. |
| **Deliverables** | (1) Tauri keyring 기반 API 키 보안 저장, (2) `LiveSession.mode='real'` 토글 + multi-real=1 강제, (3) `resolve_live_signal` 헬퍼, (4) 분할 주문 + 체결 응답 파싱, (5) 미체결 추적, (6) 일일 손실 한도/매매횟수 제한, (7) 단위/통합 테스트 |
| **Estimated Effort** | 9~13일 (한 사람 기준) |
| **Risk** | 자금 손실 가능 — 4A.4(실주문) + 4A.6(안전장치) 끝나기 전 운용 금지 |

## 배경 및 목표

### 현 상태 (2026-04-29)

- Paper multi-session: 완전 동작. baseline(preset 백테스트) vs live(Start 후 누적) 분리 표시까지 완료.
- 레거시 C# 프로젝트(`D:\SW\Bitcoin`)의 자동매매 알고리즘이 검증되어 있음.
- Rust `auto_trader.rs`는 single-session 자동매매 인프라 일부 보유(`reconcile_position`, `place_limit_buy/sell`, `execute_cycle`).
- `LiveSession.mode='real'` 모델 enum + `live_trades.is_real` 컬럼은 존재하나 미구현.

### 사용자 요구

1. 실제 Upbit API 키로 자동매매 (paper 검증된 preset 그대로)
2. 거래 내역(history) 표시
3. 수익률 UI (이미 구현된 Backtest/Live 컬럼 자동 적용)
4. **핵심 안전 가드**:
   - ETH 보유 시 첫 매도 신호 → 매도
   - 미보유(현금) 시 첫 매수 신호 → 매수
   - **현금 상태인데 시뮬상 holding이라고 매수해서는 안 됨**
   - **multi-real 1개 제한** (잔고 충돌 방지)
   - **API 키는 Tauri OS Keychain (`keyring` crate)**

## 레거시 알고리즘 (인용)

[D:/SW/Bitcoin/Strategies/StrategyRegistry.cs:30-74] `ResolveSignalFromSimulation`:

```csharp
if (positionState.position == 0) // 미보유 (실제 현금)
{
    if (simSignal == "buy")       return { FinalSignal = "buy", ... };
    if (simSignal == "buy ready") return { FinalSignal = "buy ready", ... };
    return { FinalSignal = "ready" };  // hold/sell 등 무시
}
else // 보유 중
{
    if (simSignal == "sell")       return { FinalSignal = "sell", ... };
    if (simSignal == "sell ready") return { FinalSignal = "sell ready", ... };
    return { FinalSignal = "hold", ... };  // buy 등 무시
}
```

이 한 함수가 사용자 요구의 모든 가드를 표현. Rust 포팅 시 `core::signals::resolve_live_signal(sim_signal, position) -> &'static str` 한 함수로 정리.

## 단계별 작업

### Phase 4A.1 — API 키 보안 저장 (1~2일)

**파일**:
- `src-tauri/Cargo.toml` — `keyring` crate 추가
- `src-tauri/src/commands/upbit_keys.rs` (신규) — save/get/clear/test 4개 command
- `src-tauri/src/lib.rs` — invoke_handler 등록
- `src-tauri/src/api/upbit.rs` — `from_keyring()` 생성자 추가
- `src-tauri/src/services/auto_trader.rs` + `commands/trading.rs` — env-var 경로를 keyring 우선으로 이전 (env fallback 유지)
- `src/lib/api.ts` — Tauri invoke 래퍼
- `src/types/index.ts` — UpbitKeyStatus 타입
- `src/pages/SettingsPage.tsx` — Upbit API Keys 섹션 (입력/Save/Test/Clear)

**검증**: `cargo build`, `npm run vite:build`, 키 저장 후 `test_upbit_connection` (`get_all_balances` 호출) 성공.

**체크리스트**:
- [ ] Cargo.toml에 keyring crate 추가
- [ ] upbit_keys.rs commands (save_upbit_keys / get_upbit_keys / clear_upbit_keys / test_upbit_connection)
- [ ] invoke_handler 등록 + 프런트 타입/API
- [ ] SettingsPage UI (access/secret 입력, Save/Test/Clear 버튼, 마스킹 표시)
- [ ] env-var fallback 유지 (개발 편의 + 기존 운용 호환)
- [ ] 단위 테스트: keyring 저장/로드 round-trip

### Phase 4A.2 — Mode 토글 + multi-real 강제 (1일)

**목표**: 사용자가 paper 세션을 real로 “승격”할 수 있고, real이 1개 초과는 강제 거부.

**파일**:
- `src-tauri/src/db/live_repo.rs` — `set_session_mode(id, mode)` + `count_running_real()`
- `src-tauri/src/commands/live_trading.rs` — `toggle_session_mode` 커맨드 (multi-real=1 검증)
- `src/components/live/SessionTable.tsx` — 행에 “Promote to Real” 버튼 (paper일 때만)
- `src/components/live/PromoteRealDialog.tsx` (신규) — 명시적 confirm 다이얼로그
- `src/pages/LiveTradingPage.tsx` — Kill Switch 버튼 (모든 real 세션 stop + open 주문 cancel)

**체크리스트**:
- [ ] mode 토글 백엔드 + 검증 (실거래 1개 초과 거부)
- [ ] Promote to Real UI + 키 미설정 시 Settings로 안내
- [ ] Kill Switch UI + 백엔드 (`emergency_stop_all_real`)

### Phase 4A.3 — `resolve_live_signal` + session_engine real 분기 (2일)

**파일**:
- `src-tauri/src/core/signals.rs` 또는 `strategies/mod.rs` — `resolve_live_signal` 헬퍼
- `src-tauri/src/services/session_engine.rs` — `mode == "real"` 분기 추가

**핵심 코드**:
```rust
pub fn resolve_live_signal(sim_signal: &str, position: i32) -> &'static str {
    match (position, sim_signal) {
        (0, "buy")        => "buy",
        (0, "buy ready")  => "buy ready",
        (0, _)            => "ready",
        (1, "sell")       => "sell",
        (1, "sell ready") => "sell ready",
        (1, _)            => "hold",
        _ => "ready",
    }
}
```

```rust
// session_engine.rs run_session_cycle 끝부분 추가
if session.mode == "real" {
    let (status, buy_price, _) = reconcile_with_upbit(...).await?;
    let position = if status == "holding" { 1 } else { 0 };
    let final_signal = resolve_live_signal(&result.last_signal_type, position);
    // 4A.4에서 주문 분기 구현
}
```

**체크리스트**:
- [ ] resolve_live_signal + 12 케이스 단위 테스트
- [ ] FilterConfirmedCandles Rust 포팅 (real 분기 전용)
- [ ] reconcile_position 호출 + DbPosition을 LiveSession 컨텍스트에 적용

### Phase 4A.4 — 분할 주문 + 체결 응답 파싱 (2~3일) ⚠️ 최고 위험

**파일**:
- `src-tauri/src/api/upbit.rs` — 주문 응답 파싱 (`OrderResponse` struct: uuid/state/executed_volume/avg_price)
- `src-tauri/src/services/order_executor.rs` (신규) — `execute_split_buy/sell` 포팅
- `src-tauri/src/services/session_engine.rs` — 4A.3에 4A.4 주문 분기 통합
- `src-tauri/src/db/live_repo.rs` — `insert_real_trade` 헬퍼 (is_real=1)

**레거시 동작**:
- `total_krw > 500_000` → 3분할, 2초 간격
- 그 외 → 단일 주문
- 각 chunk 실패 시 로깅 후 다음 chunk 시도
- 주문 실패해도 다음 사이클에서 재시도

**체크리스트**:
- [ ] OrderResponse 파싱 (체결가/체결량 추출)
- [ ] execute_split_buy: 시장가 매수, KRW 잔고의 99.95%를 ceil 분할
- [ ] execute_split_sell: 시장가 매도, 코인 잔고 전량 ceil 분할
- [ ] 체결 결과로 session.current_buy_price/volume 갱신 (실 체결가 반영)
- [ ] live_trades(is_real=1) row 추가 + pnl_pct는 실 체결가 기준
- [ ] 단위 테스트: split chunks 계산, OrderResponse 파싱

### Phase 4A.5 — 미체결 limit 주문 추적 (1~2일)

**시나리오**: 지정가 주문이 호가창 갭으로 미체결 → 다음 사이클에서 어떻게?

**파일**:
- `src-tauri/migrations/010_pending_orders.sql` — 신규 테이블
  ```sql
  CREATE TABLE pending_orders (
    uuid TEXT PRIMARY KEY,
    session_id INTEGER REFERENCES live_sessions(id) ON DELETE CASCADE,
    side TEXT, market TEXT, target_price REAL, volume REAL,
    placed_at TEXT, status TEXT  -- 'wait', 'done', 'cancel'
  );
  ```
- `src-tauri/src/services/pending_order_tracker.rs` (신규) — 매 사이클 시작 시 status check
- `src-tauri/src/api/upbit.rs` — `get_order_status(uuid)`, `cancel_order(uuid)` 메서드

**정책**:
- limit 주문 → pending_orders에 wait로 insert
- 다음 사이클 시작 시 status check
- `done` → live_trades(is_real=1) 확정 + pending_orders status='done' 갱신
- `wait` 1시간 초과 → cancel_order + 다음 사이클에서 재시도

**체크리스트**:
- [ ] migration 010
- [ ] pending_order_tracker 백그라운드 흐름
- [ ] 1시간 timeout cancel 정책
- [ ] 단위 테스트: 미체결 → done/cancel 전이

### Phase 4A.6 — 안전장치 (1일)

**파일**:
- `src-tauri/migrations/011_safety_limits.sql` — `live_sessions.max_daily_loss_pct REAL`, `max_daily_trades INTEGER`
- `src-tauri/src/services/session_engine.rs` — 사이클 시작 시 check, 한도 초과면 자동 stop
- `src/components/live/SessionTable.tsx` — 한도 표시 + 편집

**기본값**:
- `max_daily_loss_pct = -10.0` (오늘 -10% 도달 시 자동 stop)
- `max_daily_trades = 20` (하루 20회 초과 시 stop)

**체크리스트**:
- [ ] migration 011 + 모델 갱신
- [ ] 한도 체크 로직 + 자동 stop + Tauri 알림
- [ ] UI에서 편집 가능

### Phase 4A.7 — 테스트 (1~2일)

**단위**:
- [ ] `resolve_live_signal` 12 케이스 (2 position × 6 signal)
- [ ] reconcile_position 4 케이스 (DB×잔고 매트릭스, 이미 일부 있음)
- [ ] split_orders 경계값 (50만원 직전/직후)
- [ ] OrderResponse 파싱 (성공/부분체결/실패)

**통합**:
- [ ] paper → real 토글 시 데이터 일관성
- [ ] multi-real 거부 시나리오
- [ ] 외부 잔고 변동 시 reconcile 동작
- [ ] kill switch가 모든 real 세션 + open 주문 정지

**E2E (수동)**:
- [ ] 실 키 + 50,000원 자본으로 1일 운용
- [ ] paper 세션과 같은 preset 병행 운용해 결과 비교

## 의존성 / Critical Path

```
4A.1 (키 저장)
   ├─→ 4A.2 (mode 토글) ─→ 4A.3 (resolve + 분기)
   │                              ├─→ 4A.4 (실주문) ─→ 4A.5 (미체결) ─→ 4A.6 (안전장치) ─→ 4A.7 (테스트)
   └────────────────────────────────────────────────────────────────────────┘
```

4A.4가 critical path의 가장 위험한 단계. 4A.6 완료 전 운용 금지.

## 운용 시작 조건

다음 모두 충족 시에만 첫 실거래 실행:
1. 4A.1~4A.7 모두 완료
2. paper 세션 1주일 이상 무사고 운용
3. 4A.7의 E2E 수동 테스트 통과 (50,000원 1일)
4. Kill switch 동작 확인
5. 일일 손실 한도 동작 확인 (인위적으로 트리거)

## 후속 단계 (Phase 4B/C/D)

이번 master plan은 4A까지. 4B (real trades 화면 분리), 4C (히스토리 페이지), 4D (추가 안전장치)는 4A 안정화 후 별도 plan.

---

**작성일**: 2026-04-29
**관련 파일**:
- 레거시: `D:\SW\Bitcoin\LiveTradingService.cs`, `D:\SW\Bitcoin\Strategies\StrategyRegistry.cs`
- Rust: [src-tauri/src/services/auto_trader.rs](../../src-tauri/src/services/auto_trader.rs), [src-tauri/src/services/session_engine.rs](../../src-tauri/src/services/session_engine.rs), [src-tauri/src/api/upbit.rs](../../src-tauri/src/api/upbit.rs)
