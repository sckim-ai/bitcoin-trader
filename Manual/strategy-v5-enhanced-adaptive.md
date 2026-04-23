# V5 — Enhanced Adaptive (듀얼 PSY 적응형)

## 개요
V3 RegimeAdaptive 전략 위에 **PsyHour AND PsyDay 듀얼 확정** 조건을 덧댄 전략. 레거시 C# `EnhancedAdaptiveStrategy.cs`를 그대로 포팅했습니다.

V3와의 **유일한 차이는 두 곳**입니다:

| 단계 | V3 조건 | V5 조건 |
|------|---------|---------|
| 매수 확정 (buy_sign 1→2) | `vol < set_volume × buy_decay AND psy_day < buy_psy` | `vol < set_volume × buy_decay AND psy_hour < buy_psy_hour AND psy_day < buy_psy_day` |
| 매도 확정 (sell_sign 1→2) | `vol < set_volume × sell_decay` | `vol < set_volume × sell_decay AND psy_hour > sell_psy_hour AND psy_day > sell_psy_day` |

나머지 전부 V3와 동일합니다 — 긴급매수, 고정손절, 최대보유, 거래량 기반 손절, 긴급매도, bar-by-bar 수익률 계산, RSI 적응형 파라미터 보간, entry RSI 동결.

## PSY 지표 복습
- **PsyHour**: 최근 N 시간 중 상승 시간 비율을 -1~1 범위로 정규화 (음수=약세, 양수=강세)
- **PsyDay**: 전일 마지막 non-zero PsyHour (일 단위 심리)
- 매수 확정은 "둘 다 약세" (<), 매도 확정은 "둘 다 과열" (>)

## 파라미터

### V3 공유 (33개)
V5는 V3의 매수/매도 볼륨·가격 드롭·감쇠·대기·리스크 파라미터를 **모두 그대로** 재사용합니다. 하지만 **`v3_buy_psy_lo/hi/pow` 3개는 사용하지 않습니다** — V5 전용 듀얼 PSY 파라미터로 교체되었기 때문입니다.

### V5 전용 (12개, 레거시 C# 기본값 기준)

| 파라미터 | Lo | Hi | Pow | 의미 |
|---|---|---|---|---|
| `v5_buy_psy_hour_*` | 0.05 | -0.15 | 1.0 | 매수 PsyHour 임계값 (RSI 과매도→느슨, 과매수→엄격) |
| `v5_buy_psy_day_*` | 0.15 | -0.20 | 1.0 | 매수 PsyDay 임계값 |
| `v5_sell_psy_hour_*` | -0.05 | 0.15 | 1.0 | 매도 PsyHour 임계값 |
| `v5_sell_psy_day_*` | -0.10 | 0.20 | 1.0 | 매도 PsyDay 임계값 |

Lo/Hi는 각각 RSI=20/80 극단일 때의 값이고 중간은 `lo + (hi-lo) × r^pow` (r = clamp((rsi-20)/60, 0, 1)).

## 사용 방법
1. **Simulation** 페이지 → Strategy 셀렉터에서 **V5** 선택
2. Parameters 카드에 V3 공유 파라미터 + V5 듀얼 PSY 12개가 자동 노출
3. 기본값으로 Run Simulation → V3와 결과 비교
4. **Optimization** 페이지에서도 V5 선택 가능 — NSGA-II가 V3의 36개 + V5의 12개 = 48개 파라미터 공간을 탐색

## 검증 (테스트)
구조적 올바름을 보장하는 6개 unit test가 `tests/v5_enhanced_adaptive_test.rs`에 존재:
1. Registry에 "V5" 등록 확인
2. `parameter_ranges()`가 V5 PSY 12개 포함 & `v3_buy_psy_*` 미포함
3. **V3 ⊇ V5 containment**: PSY 임계값 매우 느슨하게 두면 V5 신호 ≥ V3
4. **PSY 전면 차단**: 불가능한 임계값에서 V5는 decay-path 매수 0건
5. **AND 조건 엄격성**: PsyHour만 통과해도 V5는 차단 (듀얼 AND 검증)
6. V5 등록이 V3를 침해하지 않음

## 라이브 트레이딩
라이브 트레이딩 루프(`services/auto_trader.rs`)는 Strategy trait의 `get_latest_signal`을 호출하므로 V5도 즉시 지원됩니다. LiveTrading 페이지 전략 셀렉터에서 선택 가능.

## 참고 — 레거시 C# 소스
- `D:\SW\Bitcoin\Strategies\EnhancedAdaptiveStrategy.cs`
- 기본값: `D:\SW\Bitcoin\NetTradingEngine.cs:251-279` (#region V5: Enhanced Adaptive Parameters)
