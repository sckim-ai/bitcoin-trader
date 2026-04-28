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

## 관련 파일
- [src/pages/LiveTradingPage.tsx](../src/pages/LiveTradingPage.tsx) — 칩 토글바 및 select 로직
- [src/components/live/CandleChart.tsx](../src/components/live/CandleChart.tsx) — 마커 필터링
- [src/components/live/SessionTable.tsx](../src/components/live/SessionTable.tsx) — Show 체크박스 컬럼
- [src/components/live/charts/sessionPalette.ts](../src/components/live/charts/sessionPalette.ts) — 의미 색(초/파/빨) 회피 10색 팔레트 + `colorFor` 안정 매핑
- [src/stores/liveTradingStore.ts](../src/stores/liveTradingStore.ts) — `hiddenSessionIds` 상태
