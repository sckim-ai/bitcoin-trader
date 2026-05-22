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

## 알림 채널별 노출 정책
다중 사용자가 같은 디스코드 채널을 보는 환경을 가정해, 채널별로 노출 수준을 다르게 보냅니다(`notify_trade_embed` / `send_split`).

| 채널 | 노출 내용 |
|------|-----------|
| **Discord (embed)** | 코인, **매수/매도 비율 %** (= 체결금액 / 총평가), 손익 %(매도), 시간, 세션 라벨, 세션 수익 % |
| **Telegram / FCM (plain)** | 본인 채널 가정 — 체결가·수량·총평가까지 모두 포함 |

- "매수/매도 비율"은 `(price × volume) / total_value_krw × 100`. 잔고 조회가 실패해 `total_value_krw`가 없으면 이 필드는 생략됩니다.
- 매수/매도 대기, 주문 등록 알림(`notify_ready_embed`, `notify_order_registered_embed`)은 현재가/지정가를 그대로 노출합니다(시장가는 어차피 공개 정보).

## Paper 세션 알림 (선택)
Real 세션은 항상 디스코드 알림이 발송되지만, paper 세션은 **세션별 채널 선택**으로 제어합니다 (`live_sessions.notify_account_ids` — JSON array of Upbit account ids).

### 활성화 (세션 리스트에서 채널 선택)
- **Live Trading** 페이지의 세션 테이블 **Mode 컬럼** 하단에 paper 세션마다 `discord (N)` / `discord off` 버튼이 표시됩니다.
- 클릭하면 **채널 선택 모달**이 열립니다 — 등록된 Upbit 계정 목록 중 다중 체크박스로 발송 대상을 고릅니다.
- 각 계정 옆에 webhook 상태가 표시됩니다:
  - `전용 채널` — 계정에 설정된 webhook으로 발송
  - `글로벌 fallback` — 계정에 webhook이 없어 Settings의 글로벌 채널로 fallback
- 저장 시 즉시 반영(optimistic update). 실패 시 이전 값으로 롤백.
- 모든 체크박스를 해제하고 저장하면 알림 off.
- Real 세션에는 이 버튼이 표시되지 않습니다(real은 항상 자기 계정 webhook으로 알림 발송).

### Fan-out 동작
- 선택된 계정 N개에 대해 매 cycle 끝의 알림이 **N번 발송**됩니다 (각 메시지의 prefix는 해당 계정 라벨).
- 텔레그램/FCM은 paper 알림에서 **비활성화**됩니다 (다중 채널 fan-out 의도에 맞춰 Discord 전용). 본인 채널로 paper 알림을 받고 싶다면 자기 계정에 webhook을 등록하고 그 계정을 선택하세요.

### Paper 알림 이벤트
| 전이 | 알림 |
|------|------|
| idle → holding | 📄 PAPER 매수 체결 (시뮬레이션 가격) |
| holding → idle | 📄 PAPER 매도 체결 (P/L %) |
| ready 신호 전이 | 📄 PAPER 매수 대기 / 매도 대기 |
| (주문 등록은 paper에 해당 없음) | — |

### 시각적 구분
- 모든 paper embed의 제목 앞에 **`📄 PAPER`** prefix.
- 색상이 회색 톤으로 dim됨 — buy=`#88AA88`, sell=`#AA8888`, ready/wait=`#999999`.
- 실주문 알림과 시각적으로 명확히 구분되어 다중 사용자 시청 환경에서 오해를 방지합니다.

### 가격/수량의 정확도
- paper는 실 잔고가 없으므로 메시지 표시값은 시뮬레이션 추정치입니다:
  - 매수가/매도가: 시뮬레이션의 trade 가격
  - 수량: `initial_capital / buy_price` 기준 추정 (paper trade row 저장값과 일치)
  - 총평가: equity (이번 cycle 시뮬레이션의 누적 결과)

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
