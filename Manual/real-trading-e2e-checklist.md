# 실거래 E2E 검증 체크리스트 (Phase 4A.7)

> 마스터플랜 [plans/master/20260429_Real_Auto_Trading.md](../plans/master/20260429_Real_Auto_Trading.md)의 운용 시작 조건을 검증하는 단계별 가이드. 이 체크리스트의 모든 항목이 통과해야 첫 실거래 운용이 안전합니다.

## 사전 준비

- [ ] Tauri 앱 최신 빌드 (master plan 4A.6 commit `c7ed90f` 이상)
- [ ] Upbit 계정 + API 키 (자산 권한 활성화)
- [ ] **운용 자본 ≤ 50,000 KRW** — 첫 운용은 매우 작게
- [ ] paper 세션 1주일 이상 무사고 운용 이력

## 1. API 키 보안 (4A.1)

| # | 단계 | 기대 결과 |
|---|---|---|
| 1.1 | Settings → Upbit API Keys, 키 입력 → Save | 토스트 “Saved to OS keychain.”, 배지 `Configured · source: keyring` |
| 1.2 | Test connection 클릭 | `Connection OK — account holds N currencies.` |
| 1.3 | 앱 종료 후 재실행 → 같은 화면 재방문 | 배지 그대로 `Configured` (키체인에서 자동 로드) |
| 1.4 | Clear → 다시 Settings 진입 | 배지 `Not configured` |

## 2. Mode 토글 + multi-real=1 (4A.2)

| # | 단계 | 기대 결과 |
|---|---|---|
| 2.1 | paper 세션 1개 생성 | SessionTable에 `paper` 배지 |
| 2.2 | `→ Real` 클릭 → PromoteRealDialog | “Daily safety limits: -10% loss · 20 trades (auto-stop)” 표시 |
| 2.3 | 체크박스 미체크로 “Promote to Real” 시도 | 버튼 비활성화 |
| 2.4 | 체크 + 버튼 → 1초 내 다이얼로그 닫힘 | SessionTable Mode가 `REAL` 로 변경 |
| 2.5 | 다른 paper 세션을 → Real 시도 | 다이얼로그 내 빨간 에러: `Multi-real not allowed: 1 other session(s)` |
| 2.6 | 첫 real 세션 → Paper로 demote → 두 번째 시도 | 정상 promote |
| 2.7 | 키 저장 안 한 상태에서 promote 시도 | 다이얼로그 내 에러: `Upbit API keys not configured. Open Settings → Upbit API Keys` |

## 3. Kill Switch (4A.2)

| # | 단계 | 기대 결과 |
|---|---|---|
| 3.1 | real 세션 running 상태 → 헤더에 “Kill switch (1)” 빨간 버튼 표시 | OK |
| 3.2 | Kill switch 클릭 → 확인 다이얼로그 → 승인 | alert: “1개 real 세션이 정지되었습니다.” |
| 3.3 | SessionTable | real 세션 status가 `stopped`, mode는 `REAL` 그대로 (보존) |
| 3.4 | running real 0개 → 헤더의 Kill switch 버튼 사라짐 | OK |

## 4. 신호 결정 가드 (4A.3) — stderr 로그 확인

> Tauri dev console 또는 stderr 출력으로 검증. real 세션 매시간 정시에 다음 로그가 찍혀야 합니다.

| # | 시뮬 결과 | 실잔고 | 기대 final_signal | 주문 |
|---|---|---|---|---|
| 4.1 | sim=hold | idle (현금) | **ready** | 없음 |
| 4.2 | sim=buy | idle | **buy** | place_market_buy |
| 4.3 | sim=sell | idle | **ready** | 없음 |
| 4.4 | sim=hold | holding | **hold** | 없음 |
| 4.5 | sim=buy | holding | **hold** | 없음 (이중 매수 차단) |
| 4.6 | sim=sell | holding | **sell** | place_market_sell |

로그 형식:
```
[real cycle] session=N sim=<X> status=<Y> coin=... krw=... price=... → final=<Z>
```

## 5. 주문 실행 + 미체결 추적 (4A.4 + 4A.5)

| # | 단계 | 기대 결과 |
|---|---|---|
| 5.1 | real 세션 first cycle → final=buy | `[real BUY] OK — vol=... @ ...` 로그, live_trades에 is_real=1 row + pending_orders에 `wait` 또는 `done` row |
| 5.2 | DB 직접 확인: `SELECT side, is_real, signal FROM live_trades WHERE session_id=N ORDER BY id DESC LIMIT 5` | 최상단 `buy / 1 / real_buy` |
| 5.3 | 다음 cycle (1시간 후) | `[pending_tracker] resolved DONE <uuid>` 로그 (시장가는 보통 즉시 done) |
| 5.4 | 매도 신호 발생 시 (다음 사이클 또는 강제 시뮬) | `[real SELL] OK — vol=... (P/L: ±X.XX%)` |

## 6. 회로 차단기 — 일일 손실 한도 (4A.6)

> 실제 -10% 손실을 기다리는 대신 인위적으로 트리거. **임시 SQL 조작 후 원복** 권장.

| # | 단계 | 기대 결과 |
|---|---|---|
| 6.1 | DB에 인위적 손실 trade 삽입: `INSERT INTO live_trades (session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real) VALUES (N, datetime('now'), 'sell', 3000000, 0.05, 750, 'test_loss', -150000, -15.0, 1);` | OK |
| 6.2 | 다음 정시 cycle (또는 즉시 트리거 가능하면 강제 cycle) | stderr: `[real cycle] session=N CIRCUIT BREAKER tripped: daily loss -15.00% ≤ limit -10.00%` |
| 6.3 | SessionTable | session.status = `stopped`, 주문 발생 안 함 |
| 6.4 | 검증 후 SQL 원복: `DELETE FROM live_trades WHERE signal='test_loss';` | OK |

## 7. 회로 차단기 — 매매 횟수 한도 (4A.6)

| # | 단계 | 기대 결과 |
|---|---|---|
| 7.1 | DB에 20개 가짜 sells 삽입: `INSERT INTO live_trades (session_id, ts, side, price, volume, fee, signal, pnl, pnl_pct, is_real) SELECT N, datetime('now'), 'sell', 3000000, 0.001, 1.5, 'test_count', 0, 0, 1 FROM (WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM c WHERE x<20) SELECT x FROM c);` | 20 rows |
| 7.2 | 다음 cycle | stderr: `daily trade count 20 ≥ limit 20` → stopped |
| 7.3 | 원복: `DELETE FROM live_trades WHERE signal='test_count';` | OK |

## 8. 외부 잔고 변동 reconcile (4A.3)

| # | 단계 | 기대 결과 |
|---|---|---|
| 8.1 | real 세션 idle 상태 → Upbit 모바일 앱에서 수동으로 ETH 1만원어치 매수 | OK |
| 8.2 | 다음 cycle → SessionTable | session.current_position = `holding`, current_buy_price = 매수 시점 가격 (reconcile 보정) |
| 8.3 | 시뮬 신호가 buy여도 추가 매수 안 함 (이미 holding) | `final=hold` 로그 |
| 8.4 | Upbit에서 수동 매도 → 다음 cycle | session 자동 idle 복귀 |

## 9. paper ↔ real 데이터 일관성 (자동 테스트로 보강됨)

→ `cargo test --test multi_real_test`로 검증 (8 케이스). 자동.

---

## 운용 시작 결정

위 9개 섹션 모두 통과 + 사용자가 “결과를 신뢰할 수 있다”고 판단할 때만:

1. ✅ Settings → Test connection OK
2. ✅ paper 세션 1주일 무사고
3. ✅ 위 1~8 섹션 모두 통과
4. ✅ Kill switch 동작 1회 직접 확인
5. ✅ 회로 차단기 동작 1회 직접 확인 (인위 트리거 + 원복)

**그 외 한 가지라도 미통과 시 운용 금지.**

## 후속 — Phase 4B 이후

이 체크리스트는 Phase 4A 끝점. 실거래 안정 운용 후:
- **4B**: real trades 화면 분리 표시 (paper 시뮬 vs 실거래 시각 분리)
- **4C**: 히스토리 페이지 + CSV export
- **4D**: 추가 안전장치 (분당 매매 제한, 슬리피지 보정 등)
