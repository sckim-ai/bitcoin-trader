# 라이브 차트 (Live Chart)

## 개요
Live Trading 페이지 상단의 KRW-ETH 캔들차트와 신호 스트립차트입니다. 여러 페이퍼/실거래 세션의 매매 마커를 한 화면에서 동시에 비교할 수 있습니다.

## 화면 구성

### 1. 날짜 범위 선택
- 헤더 우측 `From` 날짜 입력 — 차트가 이 시점부터 현재까지 표시
- 기본값: 최근 7일

### 2. 세션 표시 토글바 (칩)
차트 카드 상단에 가로로 나열된 색 칩 버튼들:
- **세션별 칩**: 색 사각형 + 세션 라벨. 각 세션은 10색 팔레트(주황/노랑/라일락/갈색/진보라/마젠타/진주황/골드/회색/진회색)에서 고정 색 부여
- 캔들 상승봉(초록), 매도-수익(파랑), 매도-손실(빨강) 색은 의미 색으로 점유되어 세션 팔레트에서 제외됨 — 어느 행이 어느 의미인지 헷갈리지 않음
- **클릭 → 토글**: 보임/숨김 전환 (숨김 시 칩이 흐려짐)
- **All / None 버튼**: 모든 세션 일괄 표시/숨김

### 3. Signal session 드롭다운
신호 스트립차트(캔들 하단 색띠)에 그려질 단일 세션을 선택. **현재 보이는(visible) 세션만** 옵션으로 표시됨. 보이는 세션이 없거나 현재 선택이 숨김 처리되면 자동으로 첫 보이는 세션으로 폴백.

### 4. 캔들차트 마커
- **매수**: 봉 아래 위쪽 화살표 (▲), 세션 고유 색, 라벨 = 세션명
- **매도**: 봉 위 아래쪽 화살표 (▽), 위→아래 두 줄 표시
  - 위쪽: 세션명 (세션 고유 색)
  - 아래쪽: 수익률 + 화살표 — 수익=파랑(`+1.93%`) / 손실=빨강(`−0.47%`)
  - 화살표 색은 수익률 마커의 색과 동일 → 한눈에 수익/손실 판독 가능
- 실거래 마커는 라벨에 `(R)` 표시 + 크기 2배

### 5. 신호 스트립차트
캔들차트 바닥의 색띠 — 선택된 세션의 봉별 신호 상태:
- 회색 = ready, 연두 = buy ready, 진한 초록 = buy
- 노랑 = hold, 주황 = sell ready, 빨강 = sell

### 6. 인디케이터 토글
좌측 상단 ToggleChip — SMA / BB / Volume 표시 ON/OFF.

## 세션 추가/제거

### Sessions 테이블
화면 하단 "Live Trading — Paper Sessions" 카드:
- **Show 컬럼 체크박스**: 칩 토글바와 동일한 효과. 어느 쪽에서 토글하든 양쪽에 즉시 반영됨
- 색 사각형은 칩과 같은 색으로 표시되어 어느 행이 어느 마커/스트립인지 식별 가능

### 새 세션 만들기
"New Session" 버튼 → 프리셋 + 라벨 + 초기자본 입력. 생성 즉시 토글바와 테이블에 자동 추가되며 **기본값 visible**.

### 세션 삭제
세션을 지우면 hiddenSessionIds 배열에서도 자동 정리되어 stale ID가 누적되지 않습니다.

## 내부 동작 상세

### 색 안정성
색은 정렬된 `sessionIds` 배열의 인덱스로 결정됩니다. 토글로 일부를 숨겨도 팔레트 계산 입력은 **전체 세션 ID 리스트**를 그대로 사용하므로 색이 재배치되지 않습니다.

### 가시성 상태 저장 위치
- `useLiveTradingStore`의 `hiddenSessionIds: number[]`
- 페이지 재진입 시 유지되며, localStorage 영속화는 하지 않음 (브라우저 새로고침 시 모두 visible로 리셋)

### 신호 스트립이 단일 세션인 이유
스트립은 봉 단위 단일 색을 칠하는 히스토그램입니다. 여러 세션의 신호를 한 줄에 겹치면 색이 섞여 판독 불가능해집니다. 멀티 세션은 캔들차트 마커로 표현하고, 정밀한 봉별 신호는 **한 세션씩** 비교하는 워크플로를 권장합니다.

### 데이터 갱신 흐름 (single-writer)
시뮬레이션 사이클 실행은 **hourly scheduler 단일 경로**로만 일어납니다.
- Scheduler가 매시 정각 cycle 실행 → DB에 trades + signal_log 갱신 → `session:update` 이벤트 emit
- Frontend는 이벤트를 받으면 read-only로 `getSessionSignalLog` + `loadAllSessionTrades` 만 호출 (cycle 재실행 없음)
- 마운트 시점이 hour 경계에 맞지 않는 신규 세션은 첫 사이클까지 빈 차트를 보일 수 있음 — 다음 정각에 자동 채워짐

### Live log 패널
화면 하단 collapse 카드 — REAL cycle 진행 상황을 실시간 스트림으로 표시.
- backend의 `live_log!` 매크로가 stderr + 파일(`%LOCALAPPDATA%/bitcoin-trader/logs/live_<YYYY-MM-DD>.log`) + UI broadcast 채널 셋 다에 같은 라인을 보냄
- 모든 라인에 동일한 `HH:MM:SS.mmm KST` prefix
- prefix별 색: `[real BUY]`=초록 / `[real SELL]`=빨강 / `[real cycle]`=하늘 / `[pending_tracker]`=보라 / `[realcycle]`=호박 / `[order_executor]`=노랑
- 200줄 ring buffer, 새 라인이 오면 자동 스크롤
- Pause 버튼: 자동 스크롤 일시정지하고 과거 라인 검토 가능. Resume 시 자동 스크롤 복구
- Clear 버튼: 화면만 비움 (파일 로그는 보존)

### 매수 한도 옵션 (BUY cap)
새 세션 생성 시 "BUY Cap (KRW)" 입력 필드:
- **비워두면 (기본)**: 매수 시 Upbit 계좌 전체 KRW × 0.9995 사용 — 다중 REAL 세션 운용 시 첫 매수가 전 자본 동원 가능
- **값 입력**: 매수마다 `min(잔고, 한도) × 0.9995`로 제한. 매도는 항상 보유 전량
- DB: `live_sessions.max_order_krw REAL` (NULL=무제한). 기존 세션은 자동으로 NULL=무제한
- 추후 SessionTable에서 한도 변경(`set_session_order_cap` 커맨드) 가능 — UI 추가 예정

### 마커 시간 정합성 (paper ↔ R 봉 정렬)
한 번의 매수 결정에서 발생한 paper marker와 R 마커가 차트에서 **같은 봉에 stack**됩니다.
- paper ts = strategy의 buy transition 봉 timestamp (signal_log.last)
- R ts (동기 booking) = `data.last().candle.timestamp` (= 같은 transition 봉)
- R ts (비동기 late booking) = `placed_at - 1h`을 정시 floor (= 그 cycle이 평가했던 confirmed 봉)
- limit 발사 가격은 봉의 close가 아닌 **execution-time current_price** — 즉시 체결 가능성 ↑, "결정→실행" 시간 갭 최소화

### 분할 fill 합침 (R 마커가 봉당 1개로 보이는 이유)
REAL 매수/매도는 `execute_split_buy/sell`에서 50만원 임계로 N chunk로 갈라져 발사됩니다. 같은 결정에서:
- 일부 chunk는 같은 cycle 내 done → `real_buy`/`real_sell` row
- 일부 chunk는 wait → 다음 cycle에 `pending_order_tracker`가 done을 잡아 `real_buy_late`/`real_sell_late` row 추가

DB엔 두 row가 남지만 (집계/감사 목적 보존), 차트는 [mergeSplitFills.ts](../src/components/live/charts/mergeSplitFills.ts)가 같은 `(ts, side, is_real)`을 한 마커로 합쳐 표시합니다. 가격은 volume 가중평균, volume/fee/pnl은 합계, pnl_pct는 volume 가중평균(같은 buy_price 기준 fill이면 평균 매도가 대비 손익률과 수학적으로 동일).

분할 매수의 평균진입가도 backend에서 누적됩니다 — `weighted_avg_buy` 헬퍼([session_engine.rs](../src-tauri/src/services/session_engine.rs))가 sync booking과 late booking에서 모두 호출되어 `live_sessions.current_buy_price`가 가중평균으로 갱신됩니다. 이 평균이 다음 매도의 pnl_pct 기준이 되므로, chunk 가격이 갈려도 손익률이 정확.

### pnl_pct 단위 (분수형 single source of truth)
`live_trades.pnl_pct` 컬럼은 **분수형**(예: 0.0194 = 1.94%)으로 저장합니다. paper 전략의 `(sell - buy) / buy`와 동일 단위. real sell 두 경로(sync/late)도 분수형으로 통일. 화면 표시 시 일률 ×100을 적용해 `+1.94%` 같은 라벨로 그립니다. 로그(`[real SELL] (P/L: ...%)`)와 알림 embed는 호출부에서 표시 시점에 `× 100.0` 적용. `pnl` 컬럼은 KRW 절대값이며 회로차단기(`today_realized_pnl_pct`)는 이쪽을 사용 — 분리 보존.

### 확정 봉 정책 (closed-bar policy)
차트에 표시되는 매수/매도 마커, 신호 스트립, 실거래 주문 결정 모두 **마감된 봉만**으로 평가됩니다.
- [session_engine.rs](../src-tauri/src/services/session_engine.rs) `run_session_cycle`이 cycle 진입 즉시 `filter_confirmed_candles`로 미마감 현재 봉을 잘라내고 그 결과로만 strategy를 시뮬레이션
- 따라서 "16:00봉이 형성 중"인 16:30 시점에 사이클이 돌면 16:00봉은 평가에서 제외 → 15:00봉까지가 기준
- 17:00 정각이 지난 다음 사이클부터 16:00봉이 마감된 데이터로 평가에 포함됨 → 차트 마커도 그때 박힘
- 페이퍼/실거래 모두 같은 확정 윈도우 사용 → "차트엔 매수 화살표가 보이는데 세션은 buy ready / idle" 같은 갭 발생 안 함
- 트레이드오프: "이 봉 끝나면 살 것 같다" 미리보기는 표시되지 않음 — 신호 안정성(미마감 봉의 tick 출렁임에 마커가 깜빡이지 않음)과 일관성 우선

이 단방향 흐름은 이전 `refresh_session_cycle` IPC 명령이 별도 SQLite connection으로 같은 세션의 cycle을 동시 실행하던 race condition을 제거합니다 (live_trades에 paper trade row가 ×2로 쌓이던 증상).

## 관련 파일
- [src/pages/LiveTradingPage.tsx](../src/pages/LiveTradingPage.tsx) — 칩 토글바 및 select 로직
- [src/components/live/CandleChart.tsx](../src/components/live/CandleChart.tsx) — 마커 필터링
- [src/components/live/SessionTable.tsx](../src/components/live/SessionTable.tsx) — Show 체크박스 컬럼
- [src/components/live/charts/sessionPalette.ts](../src/components/live/charts/sessionPalette.ts) — 의미 색(초/파/빨) 회피 10색 팔레트 + `colorFor` 안정 매핑
- [src/stores/liveTradingStore.ts](../src/stores/liveTradingStore.ts) — `hiddenSessionIds` 상태
