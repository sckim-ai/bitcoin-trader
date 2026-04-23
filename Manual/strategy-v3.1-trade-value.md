# V3.1 RegimeAdaptive — Trade Value 기반

## 개요
V3 RegimeAdaptive를 포팅하여 신호 기준을 **거래량(BTC/ETH 수량)** → **거래대금(KRW)** 으로 전환한 전략. ETH 전용 트레이딩 환경에서 가격 스케일 변화에 강건.

## V3와의 차이점

### 1. 신호 기준
- **V3**: `volume` (BTC 또는 ETH 수량) 을 urgent/ready/decay 판정에 사용
- **V3.1**: `close × volume` (KRW 거래대금) 을 동일 로직에 사용

### 2. 추가 파라미터 4개 (V3에서 하드코딩이었음)
| 파라미터 | V3 값 | V3.1 범위 | 의미 |
|----------|-------|-----------|------|
| `v31_cutoff_tv_mult` | 1.0 (hard) | 0.5 – 3.0 | 진입 거래대금 대비 몇 배 이상이면 손절 컷오프 |
| `v31_urgent_sell_tv_mult` | 2.0 (hard) | 1.0 – 5.0 | sell_ready_tv 대비 몇 배 이상이면 긴급 매도 |
| `v31_sell_ready_price_rise` | 1.0 (hard) | 0.99 – 1.05 | 매도 준비 가격 상승 비율 |
| `v31_sell_wait_max` | 168 (hard) | 24 – 720 | 매도 확인 대기 최대 바 수 |

## 사용법

### 시뮬레이션
Simulation 페이지 → Strategy 셀렉터에서 **V3.1** 선택 → 파라미터는 자동 노출 (`list_strategies` 가 registry 에서 동적 로드).

REST 로 직접 호출 시:
```
POST /simulation/run
{
  "strategy_key": "V3.1",
  "market": "KRW-ETH",
  "params": { /* TradingParameters — v31_* 필드 */ }
}
```

### 최적화
`OptimizationPage` 에서 strategy selector 에 `V3.1` 선택 — 파라미터 range 는 registry 가 동적으로 노출 (별도 UI 수정 불필요).

## 파라미터 기본값 스케일
ETH/시간 거래대금 기준 (KRW):
- `urgent_buy_tv_lo/hi`: 8.4e10 / 3.0e11
- `buy_tv_lo/hi`: 2.0e10 / 7.0e10
- `sell_tv_lo/hi`: 8.0e9 / 1.26e11

V3 의 BTC-volume 기본값에 ETH 평균가 4,000,000 KRW 를 곱한 수준으로 초기화되어 있으나, **실 운용 전 반드시 NSGA-II 재최적화 필수**.

## 주의사항
- V3 와 `TradingParameters` struct 를 공유함 — 각 전략은 자기 접두어(`v3_*` vs `v31_*`) 필드만 참조
- `SimulationResult.last_set_volume` 필드는 V3.1 에서 "최근 set 된 거래대금(KRW)"을 의미 (필드명은 하위호환)
- 트레이스 로깅: `V31_TRACE_PATH` 환경변수로 CSV 출력 (`V3_TRACE_PATH` 와 별개)

## 관련 파일
- [src-tauri/src/strategies/regime_adaptive_v31.rs](../src-tauri/src/strategies/regime_adaptive_v31.rs)
- [src-tauri/src/models/trading.rs](../src-tauri/src/models/trading.rs)
- [src-tauri/tests/v31_strategy_test.rs](../src-tauri/tests/v31_strategy_test.rs)
