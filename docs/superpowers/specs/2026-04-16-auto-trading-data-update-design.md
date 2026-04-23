# Auto-Trading Loop + Data Auto-Update Design

## TL;DR
레거시 C# LiveTradingService의 자동매매 루프와 DataUpdateManager를 Tauri+Rust로 포팅.
V4(ML) 전략 제외, V0~V3/V5 전략의 자동 매매 지원.

## 1. Auto-Trading Loop

### Architecture
- **새 파일**: `src-tauri/src/services/auto_trader.rs` — 자동매매 핵심 로직
- **수정**: `state.rs` — `AutoTradingHandle` 추가
- **수정**: `commands/trading.rs` — start/stop/status 커맨드 추가
- **수정**: `lib.rs` — 새 커맨드 등록

### AutoTrader Flow (매시간 루프)
```
1. Position Reconciliation — DB 포지션 vs 실제 Upbit 잔고 비교 보정
2. Fetch Candles — Upbit API에서 200개 hourly 캔들 fetch
3. Calculate Indicators — indicators::calculate_all()
4. Generate Signal — strategy.get_latest_signal(data, params, position)
5. Execute Trade — BUY: 분할주문(50만원 초과 시 3분할), SELL: 전량 매도
6. Update DB — positions, trades 테이블 갱신
7. Send Notifications — NotificationManager로 알림 발송
8. Emit Events — Tauri event system으로 프론트엔드 알림
9. Wait — 다음 정각까지 대기
```

### AppState Extension
```rust
pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: StrategyRegistry,
    pub auto_trading: Mutex<Option<AutoTradingHandle>>,
}

pub struct AutoTradingHandle {
    pub cancel_token: Arc<AtomicBool>,
    pub market: String,
    pub strategy_key: String,
}
```

### Tauri Events
- `auto-trade:log` — 로그 메시지 (timestamp, level, message)
- `auto-trade:trade` — 주문 체결 (side, market, price, volume, pnl)
- `auto-trade:position` — 포지션 변경 (status, buy_price, buy_volume)
- `auto-trade:status` — 시작/중지 상태

### Signal Generation Fix
현재 `get_latest_signal`이 항상 Hold를 반환하는 stub. 엔진의 마지막 상태를 기반으로 실제 신호를 생성하도록 수정:
- 시뮬레이션 결과의 last state + 현재 포지션을 비교하여 Buy/Sell/Hold 결정

## 2. Data Auto-Update

### Architecture
- **수정**: `commands/data.rs` — `update_market_data`, `auto_update_all_markets` 추가
- **수정**: `api/upbit.rs` — `get_candles_before` 추가 (to 파라미터 지원)

### Flow
1. DB에서 마지막 timestamp 조회
2. Upbit API로 해당 시점 이후 캔들 fetch (200개씩 페이징)
3. INSERT OR IGNORE로 중복 제거 후 저장
4. 삽입된 신규 캔들 수 반환

### 지원 마켓/타임프레임
- Markets: KRW-BTC, KRW-ETH
- Timeframes: hour(60분), day, week

## 3. Frontend Changes

### LiveTradingPage 수정
- 전략 선택 드롭다운 추가
- Start/Stop Auto Trading 버튼 추가 (기존 모니터링과 별도)
- Tauri event listener로 실시간 로그/상태 수신
- 자동매매 상태 표시 (실행 중 전략, 마지막 신호 등)

### tradingStore 수정
- `isAutoTrading`, `autoTradingStrategy` 상태 추가
- `startAutoTrading`, `stopAutoTrading`, `getAutoTradingStatus` 액션 추가

### api.ts 수정
- `startAutoTrading`, `stopAutoTrading`, `getAutoTradingStatus` 함수 추가
- `updateMarketData`, `autoUpdateAllMarkets` 함수 추가

## 4. Test Plan

### Rust 테스트 (auto_trader.rs)
1. 캔들→지표→신호 파이프라인 정상 작동
2. 포지션 reconciliation 로직 (DB vs 실제 잔고 불일치 시)
3. 분할 주문 계산 (금액별 분할 수)
4. 신호별 주문 실행 분기 (Buy/Sell/Hold)
5. 포지션/거래 DB 저장 검증
6. 취소 토큰으로 루프 중지
7. 다음 정각 대기 시간 계산

### Rust 테스트 (data update)
1. API 캔들 → DB 저장 (신규)
2. 중복 캔들 무시 (INSERT OR IGNORE)
3. 빈 DB에서 초기 업데이트
4. 기존 데이터 이후 incremental 업데이트

### 통합 테스트
1. 전체 사이클: 데이터 업데이트 → 신호 생성 → 주문 결정
2. 에러 핸들링: API 실패 시 graceful retry
