# Market Data 동기화

Upbit에서 BTC/ETH 캔들 데이터를 받아 SQLite에 저장. hour/day/week 3종.

## 자동 동작

### 시작 시 + 60초 주기 백그라운드 업데이트
앱 구동 직후 `services/market_updater::run_loop`이 별도 스레드에서 시작.
- 대상: `KRW-BTC`, `KRW-ETH` × `hour`/`day`/`week` (총 6쌍)
- 한 사이클 끝나면 60초 sleep 후 반복
- 별도 SQLite 연결 사용 (Tauri 메인 스테이트와 분리)

### sync_market 동작
각 (market, timeframe)에 대해 두 단계로 진행:
1. **Forward sync**: Upbit에서 최신 200개 fetch → `INSERT OR IGNORE`
2. **Back-fill**: DB의 `MIN(timestamp)`이 `2020-01-01T00:00:00`보다 늦으면 그 시점부터 거꾸로 페이지네이션해서 채움

→ 부분만 받아진 경우(예: 200개만 있던 BTC/hour)도 다음 사이클에 자동 복구.

## 수동 트리거

### LiveTrading 페이지 — Refresh 버튼
`auto_update_all_markets` 호출. 6쌍 모두 `sync_market` 실행.

### DataLoad 페이지 — 마켓/타임프레임 변경
`marketDataStore.loadCandles`이 캔들 조회 후 비어있으면 `update_market_data` 호출.

## 차트 X축 라벨

`CandlestickChart`는 `timeframe` prop을 받아 lightweight-charts 옵션을 분기:
- `hour`: `timeVisible=true`, x축 `MM-DD HH:00`, crosshair `YYYY-MM-DD HH:00`
- `day`/`week`: `timeVisible=false`, x축/crosshair 모두 `YYYY-MM-DD`

## 데이터 가상화 (Bar Limit)

`get_candles`에 `limit?: u32` 파라미터를 추가. SQL은 `ORDER BY timestamp DESC LIMIT n` 후 ASC로 재정렬 → **항상 최신 N개**만 반환.
- DataLoad 페이지에 `100/500/1000/5000` 셀렉터, 기본 500
- 시뮬레이션/최적화 호출자는 `None`을 넘겨 전체 사용 (분석에는 풀 데이터 필요)
- IPC 페이로드 크기와 차트 초기 렌더 시간 모두 감소

## Live Price (캔들 완성 전 실시간 표시)

DataLoad 페이지에서 2초 간격으로 `get_current_price(KRW-{market})` 폴링 → `livePrice` state.
`CandlestickChart`는 `livePrice` prop을 받아 마지막 봉을 `series.update()`로 업데이트:
- close = livePrice
- high = max(기존 high, livePrice)
- low = min(기존 low, livePrice)

**중요:** `get_current_price`는 ticker 엔드포인트(인증 불필요)를 호출하지만, 기존엔 `create_client()`가 `UPBIT_ACCESS_KEY` 미설정 시 Err 반환해서 폴링이 조용히 실패했음. → `create_public_client()`로 분리해서 키 없이도 동작.

새 봉 생성은 백그라운드 업데이터(60초 주기)에 맡김. 30초 주기로 `refreshCandles`가 DB 재조회 → 백그라운드 UPSERT 결과(아래)를 차트에 반영.

`fittedRef`로 첫 로드만 `fitContent()` → 백그라운드 갱신 시 차트 스크롤 점프 방지.

## In-progress 캔들 갱신 (UPSERT)

Upbit는 진행 중인 봉(현재 시간)도 부분 OHLCV로 반환. 기존 `INSERT OR IGNORE`는 같은 timestamp가 이미 있으면 새 값을 무시 → **방금 닫힌 봉도 부분 데이터로 고정**되는 버그.

→ `INSERT ... ON CONFLICT(market, timeframe, timestamp) DO UPDATE SET open/high/low/close/volume = excluded.*`로 변경. 닫힌 캔들은 값이 동일해서 사실상 no-op, in-progress 봉만 매 fetch마다 갱신.

## 트러블슈팅

### 특정 마켓의 데이터가 부분만 있을 때
백그라운드 업데이터가 한 사이클(최대 1분 + Upbit 페이지네이션 시간)이면 자동 백필. 즉시 채우려면 LiveTrading 페이지의 Refresh 버튼.

### Upbit 레이트 리밋
요청 사이 150~200ms sleep. 한 사이클이 90초 가량 걸릴 수 있음.

### DB 위치
`%LOCALAPPDATA%/bitcoin-trader/bitcoin_trader.db` (프로젝트 밖 — Tauri file watcher 무한루프 방지)
