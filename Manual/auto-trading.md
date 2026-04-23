# 자동매매 (Auto-Trading)

## 개요
매시간 정각에 선택한 전략으로 자동으로 매매 신호를 생성하고 주문을 실행합니다.

## 사용 방법

### 1. 전제 조건
- Upbit API 키가 환경변수에 설정되어 있어야 함 (`UPBIT_ACCESS_KEY`, `UPBIT_SECRET_KEY`)
- 데스크탑 모드(Tauri)에서만 동작

### 2. 자동매매 시작
1. Live Trading 페이지에서 전략 선택 (V0~V3, V5)
2. **Auto Start** 버튼 클릭
3. 모니터링이 자동으로 시작됨

### 3. 자동매매 루프 동작
```
매 정각:
1. 포지션 정합성 검증 (DB vs 실제 Upbit 잔고)
2. Upbit에서 200개 시간봉 캔들 fetch
3. 기술 지표 계산 (18종)
4. 선택한 전략으로 신호 생성 (Buy/Sell/Hold)
5. 주문 실행 (50만원 초과 시 3분할)
6. DB에 포지션/거래 기록
7. 알림 발송 (Discord/Telegram/FCM)
8. 다음 정각까지 대기
```

### 4. 전략별 특성
| 전략 | 특성 |
|------|------|
| V0 | Volume Decay — 거래량 급증 + 감쇠 패턴 |
| V1 | Enhanced Volume — V0 + ATR 기반 적응형 |
| V2 | Multi-Indicator — RSI/MACD/BB 복합 스코어링 |
| V3 | Regime Adaptive — RSI 기반 동적 임계값 보간 |
| V5 | Enhanced Adaptive — PSY 이중 확인 + ATR 트레일링 |

### 5. 자동매매 중지
**Auto Stop** 버튼 클릭. 현재 사이클 완료 후 안전하게 종료.

## 이벤트 시스템
프론트엔드는 Tauri 이벤트로 실시간 상태를 수신:
- `auto-trade:log` — 로그 메시지
- `auto-trade:trade` — 주문 체결
- `auto-trade:position` — 포지션 변경
- `auto-trade:status` — 시작/중지 상태

---

# 데이터 자동 업데이트 (Data Auto-Update)

## 개요
Upbit API에서 최신 캔들 데이터를 가져와 DB에 저장합니다.

## 사용 방법
1. Live Trading 페이지에서 **새로고침 버튼** (↻) 클릭
2. 6개 조합 자동 업데이트: (BTC, ETH) × (hour, day, week)
3. 중복 데이터는 자동 무시 (INSERT OR IGNORE)

## API
- `update_market_data(market, timeframe)` — 특정 마켓/타임프레임 업데이트
- `auto_update_all_markets()` — 전체 업데이트
