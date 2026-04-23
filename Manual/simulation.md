# 시뮬레이션 (Simulation)

## 개요
선택한 전략을 CSV/DB에 적재된 과거 캔들 데이터 위에서 백테스트합니다. 전략별 파라미터를 UI에서 직접 조정해 결과를 비교할 수 있습니다.

## 사용 방법

### 1. 전제 조건
- **DataLoad** 페이지에서 시뮬레이션 대상 market/timeframe 데이터가 적재되어 있어야 함
- 데스크탑·웹(PWA) 양쪽 모두 동작 (PWA는 HTTP `/api/simulation/run` 호출)

### 2. 시뮬레이션 실행
1. Simulation 페이지 진입
2. **상단 컨트롤 바**에서:
   - **Strategy**: V0~V5 선택
   - **Market**: BTC / ETH (**default 파라미터도 market별로 다름** — ETH는 volume 임계값이 BTC × 13)
   - **Timeframe**: Hour / Day / Week
   - **Since / Until** (선택): 날짜 picker 클릭 시 달력 UI. 비워두면 **최초 봉부터 현재까지 전체** 사용
   - 컨트롤 아래 힌트: `Data: 55,205 bars · 2020-01-01 → 2026-04-20` (실제 DB 범위)
3. **Parameters** 카드에 해당 전략의 파라미터가 자동으로 펼쳐짐
   - 기본값은 백엔드 `TradingParameters::default()`에서 자동 로드
   - 이름 prefix 기반 섹션: **Buy / Sell / Risk / V1 / V2 / V3 · Buy / V3 · Sell / ...**
4. 필요한 값을 `min ~ max` 범위 내에서 조정 (step 단위로 증감)
5. **Run Simulation** 클릭 → 결과:
   - **MetricCard 10종**: Total Return, Market Return, Win Rate, Profit Factor, Max Drawdown, Sharpe Ratio, Sortino Ratio, Annual Return, Trades, Max Consec. Losses
   - **Equity Curve**: 거래마다 누적된 자본 곡선 (ECharts)
   - **Trade History 테이블**: **Buy Time / Sell Time** (ISO timestamp), 가격, PnL %, 보유 봉 수, Buy Signal + Sell Signal 분리 표시
   - **Signal Timeline**: 상태 전이 로그 (ready → buy ready → buy → hold → sell ready → sell). 레거시 `DetermineSignalType` 체계 포팅. 배지 색상으로 종류 구분, 400px 스크롤
5. 값을 초기화하려면 Parameters 카드 우상단 **Reset** 버튼

### 3. 섹션 분류 규칙 (name prefix)
| 이름 prefix | 섹션 |
|-------------|------|
| `urgent_buy_*`, `buy_*` | Buy |
| `urgent_sell_*`, `sell_*` | Sell |
| `trailing_stop_*`, `max_hold_*`, `fee_rate`, `fixed_*` | Risk |
| `v1_*` … `v5_*` | V1 … V5 (추가로 `buy`/`sell` 서브 섹션) |

### 4. 전략을 변경하면
- `setSelectedStrategy`가 해당 전략의 **defaults**로 params를 리셋
- 이전 결과(`result`)도 함께 초기화 (다른 전략 결과가 남아 혼선되지 않도록)

### 5. 파라미터 제공 방식
- 각 전략 구현체의 `Strategy::parameter_ranges()`가 이름·min·max·step을 정의
- `list_strategies` 커맨드/라우트가 **range + default 값**을 한 번에 반환
- UI는 받은 스키마로 동적 렌더 — 새 전략 추가 시 UI 수정 불필요

## Signal 종류 — 왜 `decay_buy/decay_sell`만 보이는가?

TradeRecord 테이블의 Buy/Sell Signal 컬럼이 V0/V1/V3에서 대부분 `decay_*`로 채워지는 건 **정상 동작**입니다. 엔진 로직:

- **urgent_buy** 경로: `volume ≥ urgent_threshold` **AND** `price_change ≤ -urgent_price_drop_ratio`. 두 조건 동시 충족은 드묾 (시간당 1000 BTC 볼륨 + 2% 급락이 같은 봉에 발생해야 함)
- **decay_buy** 경로: `buy_ready`(볼륨·가격 완화 조건) → 볼륨 감쇠(`set_volume × decay_ratio` 이하로 떨어지면 확정) → 매수. 시장 정상 흐름에서 자연스럽게 발동

마찬가지로 매도:
- **urgent_sell**: 급등 + 고볼륨 (드묾)
- **fixed_stop_loss / fixed_take_profit / trailing_stop / max_hold**: default는 모두 0(비활성). **활성화하려면 Parameters 카드 Risk 섹션에서 값을 0보다 크게 설정**
- **decay_sell**: 기본 경로

**정확한 진입·이탈 흐름을 보려면 Signal Timeline 카드**를 확인하세요. `ready → buy ready → buy → hold → sell ready → sell` 상태 전이가 모두 기록됩니다.

## Default 값 — Rust 엔진 의미에 맞게 calibration
Rust 엔진은 `price_change <= -threshold` (percent)로 해석하고 volume은 `candle_acc_trade_volume` (BTC 단위)을 그대로 사용. 이는 레거시 C#의 **multiplier 의미**(`prev * ratio > current`)와 다르므로, 레거시 JSON 값을 그대로 이식하면 "100% 하락 시 매수"처럼 작동 불가능한 조건이 됩니다. 실제 Upbit BTC/hour 볼륨 분포(avg~221, max~8224 BTC)에 맞춰 재보정된 값:

| 전략 | 상태 |
|------|------|
| V0 (VolumeDecay) | volume 임계값 1000/200 BTC, price drop 2%/0.5%. default로 실 데이터에서 거래 발생 (회귀 테스트 존재) |
| V1 (EnhancedVolume) | V0 + ATR trailing 확장. 마찬가지로 실 데이터에서 매수 신호 발생 |
| V2 (MultiIndicator) | RSI/MACD/BB 복합 점수 기반. default 임계값 0.6으로 시장에 따라 매매 분산 |
| V3 (RegimeAdaptive) | RSI 보간 `lo↔hi`. volume 300~1500 BTC, price drop 0.5~2%, decay 0.2~0.3 |
| V4 (ML) | train_window=2160, retrain=720. default로도 학습 가능 |
| V5 (EnhancedAdaptive) | PSY threshold + ATR + 거래량 |

Default가 마음에 들지 않으면 UI에서 직접 조정하거나, Optimization 페이지로 시장 조건에 맞는 최적해를 탐색 후 그 값을 입력.

## V3 Legacy parity (ETH/hour, legacy JSON config)
`tests/v3_legacy_config_test.rs` 는 legacy C# 프로젝트의 `TradingConfig_V3_RegimeAdaptive_20260401_140149.json` (ETH 튜닝) 를 그대로 Rust V3 엔진에 돌려 수익률을 검증합니다.

**✅ 완전 bit-exact parity 달성** (2026-04-21):
- Legacy: 59 trades / 909.29% / MaxDD 10.67%
- Rust:   59 trades / **909.29%** / MaxDD **10.67%**
- Trace diff: **0 rows** (10,878 bar 전체 한 숫자도 다르지 않음)

**Parity 확보 조건** (`v3_replay_legacy` 테스트가 모두 적용):
1. `merged_data_hour.csv` 와 `merged_data_day.csv` 를 SQLite에 주입 (KST→UTC 변환)
2. Window 범위로 indicator 재계산 (legacy `CalculateAll(window)` scope 재현)
3. `merged_data_hour.csv`의 `ETH_day_new_psy` 컬럼 직접 파싱 → KST date 별 last-non-zero 값을 `day_psy_map`으로 전달
4. `calculate_all_with_day_psy(candles, Some(&map))` 호출 — `psy_day[i] = map[kst_date(i) - 1d]`

**Legacy 데이터 주입 방법**:
```bash
# SQLite에 legacy CSV 직접 주입 (KST→UTC 자동 변환)
sqlite3 $LOCALAPPDATA/bitcoin-trader/bitcoin_trader.db <<EOF
.mode csv
.import "D:/SW/Bitcoin/merged_data_hour.csv" legacy_csv
DELETE FROM market_data WHERE market='ETH' AND timeframe='hour';
INSERT INTO market_data (market, timeframe, timestamp, open, high, low, close, volume)
SELECT 'ETH', 'hour',
    strftime('%Y-%m-%dT%H:%M:%SZ', datetime(time, '-9 hours')),
    CAST(ETH_hour_open AS REAL), CAST(ETH_hour_high AS REAL),
    CAST(ETH_hour_low AS REAL),  CAST(ETH_hour_close AS REAL),
    CAST(ETH_hour_volume AS REAL)
FROM legacy_csv WHERE CAST(ETH_hour_close AS REAL) > 0;
DROP TABLE legacy_csv;
EOF
```

**핵심 semantic 3가지** (C-1 + 이후 수정으로 이식 완료):
1. Engine: `prev × ratio > curr` multiplier, `goto CalculateReturns`, bar-by-bar 수익률 compounding, `buyPrice = close` (fee는 return 에서 차감)
2. PSY: legacy `NewPsy = (upD×upC/t - downD×downC/t)/period` (weighted, period=10, range `[-1,+1]`)
3. **PsyDay = 전일 KST date의 "마지막 non-zero hour-PSY"** — `MarketDataConverter.cs:54-69` 포팅. `calculate_all` 내부에서 hour-PSY map 자동 구축

## 초기 데이터 구동
`commands/data.rs::sync_market` 로직이 DB 비어있거나 oldest bar가 `SINCE="2020-01-01T00:00:00"` 이후면 Upbit API 페이지네이션으로 2020년부터 전체 backfill. 이후엔 증분 업데이트만.

## 레거시 parity 테스트 (tests/legacy_parity_test.rs)

레거시 C# `NetTradingEngine.RunSimulation` 을 Rust로 pure-function 포팅 후 **Rust `core::engine`과 동일 시나리오에서 병렬 실행**해 결과를 비교합니다.

**시나리오 10종** (결정론적 합성 데이터, seed 고정):
| 이름 | 설명 |
|------|------|
| `steady_uptrend` | 500봉 0.2%/봉 상승 |
| `steady_downtrend` | 500봉 0.15%/봉 하락 |
| `sideways_noise` | 100 ± 2% 횡보 |
| `v_shape_recovery` | 100→50→130 V자 회복 |
| `volatile_spikes` | 5% 확률 -5% 급락 + 2500 볼륨 스파이크 |
| `low_volume` | 볼륨 50 (모든 임계값 미달) |
| `bull_with_drawdown` | 상승 후 30% 조정 후 재상승 |
| `crash_and_flat` | 20봉 동안 -40% 붕괴 후 flat |
| `gradual_climb_high_vol` | 완만 상승 + 볼륨 1500 |
| `alternating_pumps` | sinusoidal 가격 + 위상별 볼륨 |

**검증 항목 7개**:
1. `v0_parity_trade_count_across_scenarios` — 10 시나리오에서 legacy vs Rust trade 수 일치
2. `v0_parity_return_sign_and_magnitude` — 수익 부호 일치, 크기는 ±2%p 절대 또는 ±30% 상대 허용
3. `v0_parity_with_risk_gates_activity_only` — risk gate 활성 시 둘 다 매매 발생 여부만 일치 (count 차이는 레거시의 `BuyImmediateVolume` 부재로 구조적)
4. `all_strategies_scenario_smoke` — V0/V1/V2/V3/V5 × 10 시나리오에서 finite metric + sane range
5. `all_strategies_deterministic_per_scenario` — 같은 입력 두 번 실행 시 bit-identical
6. `signal_log_invariants` — buy 카운트 = trades + open_position, sell 카운트 = trades, 인덱스 monotonic
7. `fee_rate_monotonic_across_strategies` — 수수료 ↑ 시 수익률 ↓ (또는 동일)

**V4 제외 이유**: ML walk-forward 훈련이 합성 데이터에서 비결정적이라 parity 불가.

**Legacy V0 parity 달성**: 아래 3개 메커니즘이 Rust V0 엔진에 포팅되어 레거시와 동일 로직으로 동작합니다.
- `buy_immediate_volume_threshold` / `sell_immediate_volume_threshold` 필드 추가. 대기(ready) 중 fresh volume surge가 wait counter 리셋 → 장시간 대기 후 decay confirm 가능.
- `buy_confirm_psy_threshold` 조건이 decay confirm 경로에 포함. Default 1.0은 PSY ∈ [−1,1] 상한이라 필터 off — 사용자가 조정 시 활성화.
- `urgent_buy` / `urgent_sell`이 `buy_sign`/`sell_sign` 상태 머신 외부에서 **unconditional** 동작. 대기 상태를 override해서 즉시 매매 가능.

이 변경은 V1(EnhancedVolume)에도 **자동 반영** — V1은 같은 `core::engine::run_simulation`을 호출합니다. V2/V3/V5는 자체 엔진을 쓰며 smoke + 결정론 테스트로 회귀를 잡습니다.

## 회귀 테스트 (tests/real_data_test.rs)
로컬 `%LOCALAPPDATA%/bitcoin-trader/bitcoin_trader.db`에 BTC/hour 캔들이 있을 때만 실행:
- `v0_default_produces_trades_on_real_btc_hour` — V0 default가 실 데이터에서 ≥1 거래 발생해야 함
- `all_strategies_default_produce_trades_or_signals` — V0/V1/V3 default가 최소 1 buy signal 이상

향후 누군가 default를 바꿔 "거래 0건" 상태를 재유입시키면 즉시 fail.

## 관련 파일
- 백엔드 커맨드: [src-tauri/src/commands/simulation.rs](../src-tauri/src/commands/simulation.rs)
- Axum 라우트: [src-tauri/src/server/routes.rs](../src-tauri/src/server/routes.rs)
- 레지스트리: [src-tauri/src/strategies/mod.rs](../src-tauri/src/strategies/mod.rs)
- 파라미터 스키마: [src-tauri/src/models/config.rs](../src-tauri/src/models/config.rs) `ParameterRange`
- 프론트 페이지: [src/pages/SimulationPage.tsx](../src/pages/SimulationPage.tsx)
- 스토어: [src/stores/simulationStore.ts](../src/stores/simulationStore.ts)

## 신규 전략 추가 시 파라미터 UI 자동 반영 체크리스트
1. `strategies/` 에 `Strategy` trait 구현 — `parameter_ranges()` 를 **반드시** 의미 있는 값으로 채움
2. `StrategyRegistry::new()` 에 등록
3. `core::optimizer::get_parameter/set_parameter` 의 match arm에 새 파라미터 이름·필드 매핑 추가 (없으면 default 0.0 반환/무시됨)
4. `TradingParameters` 에 필드 추가 + `Default` 값 적절히 설정
5. 이상 완료되면 UI는 자동으로 새 섹션·입력창을 렌더 (프론트 수정 불필요)
