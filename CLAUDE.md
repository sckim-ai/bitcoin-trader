# CLAUDE.md — Bitcoin Trader (Tauri + React)

> Simple is Best 원칙은 글로벌 CLAUDE.md 참조

---

## Project Overview
Upbit 거래소 기반 BTC 알고리즘 트레이딩 시스템 (Tauri 2 + React 19)
- C# .NET 8 원본 프로젝트(`D:\SW\Bitcoin`)를 Rust+React로 재작성한 버전
- 듀얼 배포: 데스크톱(Tauri IPC) + PWA(Axum HTTP, port 3741)
- 백테스트, NSGA-II 다목적 최적화, ML 예측, 라이브 트레이딩

## Development Commands
```bash
# Frontend
npm run dev          # Tauri dev (vite + tauri dev, hot reload)
npm run vite:dev     # Vite only (port 1420, no Tauri)
npm run vite:build   # Frontend build only (tsc + vite)
npm run build        # Full desktop build (frontend + tauri build)
npm run lint         # ESLint

# Backend (run from src-tauri/)
cargo build                            # Library only
cargo build --features tauri-app       # With Tauri commands
cargo test                             # All tests
cargo test --test scenario_tests       # Single test file
cargo test test_sma_calculation        # Single test by name
```

## Architecture

### Dual API Layer
`src/lib/api.ts`에서 `__TAURI__` 플래그로 Tauri IPC / HTTP REST 분기. CSV 로드, 최적화, 수동 매매는 데스크톱 전용.

### Backend (src-tauri/)
- **commands/**: Tauri 커맨드 핸들러 — `#[tauri::command]` async fn, `State<AppState>`, `Result<T, String>`
- **strategies/**: 6개 전략(V0–V5), `Strategy` trait 구현, `StrategyRegistry`로 등록
- **core/**: 트레이딩 엔진, 기술적 지표(SMA/RSI/MACD/BB/ATR/ADX/Stoch/PSY), NSGA-II 옵티마이저
- **server/**: Axum REST + WebSocket (Tauri와 별도 스레드에서 실행)
- **auth/**: Argon2 해싱, JWT 세션
- **db/schema.rs**: SQLite 초기화 + `migrations/*.sql` 자동 마이그레이션
- **state.rs**: `AppState { db: Mutex<Connection>, registry: StrategyRegistry }`

### Frontend (src/)
- **상태관리**: Zustand — `authStore`, `tradingStore`, `simulationStore`, `marketDataStore`
- **Pages**: DataLoad, Simulation, Optimization, LiveTrading, Settings, Admin
- **UI**: `src/components/ui/` — Button, Card, Input, Select, Badge, MetricCard, DataTable

### Database
SQLite: `%LOCALAPPDATA%/bitcoin-trader/bitcoin_trader.db` (프로젝트 밖에 저장 — Tauri file watcher 무한 재빌드 방지). WAL mode, FK 활성. 기본 계정: `admin`/`admin123`.

### Feature Gate
`tauri-app` Cargo feature가 `commands/` 모듈과 Tauri 코드를 게이트. 라이브러리(strategies, core, models)는 이 피처 없이 빌드 가능.

## 트레이딩 전략 (strategies/)
| 전략 | 설명 |
|------|------|
| V0 (VolumeDecay) | 거래량 감쇠 기반 기본 전략 |
| V1 (EnhancedVolume) | 거래량 패턴 강화 버전 |
| V2 (MultiIndicator) | RSI, MACD, 볼린저밴드, PSY 복합 점수 |
| V3 (RegimeAdaptive) | 시장 국면 자동 판별 후 RSI 보간 적응형 매매 |
| V4 (ML) | 머신러닝 기반 매매 시그널 예측 |
| V5 (EnhancedAdaptive) | V3 확장 적응형 |

---

## 필수 준수 사항

### 1. 프로그램 실행 및 서버 관리
- **Claude는 서버/앱을 직접 시작하지 않음** (사용자가 직접 관리)
- 코드 수정 후 → "서버 재시작 필요" 안내
- 테스트 필요 시 → 사용자에게 확인 요청

### 2. 기능 추가/개선 시 매뉴얼 작성 (필수)
`./Manual/` 폴더에 마크다운으로 작성. 기존 매뉴얼이 있으면 업데이트.

### 3. 코드 수정 시 점검
> 상세 규칙은 글로벌 CLAUDE.md의 "수정 시 점검" 참조

---

## 공통 실수 방지 체크리스트

### Tauri 커맨드 추가 시
1. `commands/` 모듈에 핸들러 작성 (`#[tauri::command]`)
2. `lib.rs`의 `invoke_handler![]` 매크로에 등록
3. `src/lib/api.ts`에 Tauri IPC + HTTP 양쪽 경로 추가
4. `src/types/index.ts` 타입 동기화 (Rust 모델과 일치)

### 새 전략 추가 시
1. `strategies/` 모듈에 `Strategy` trait 구현
2. `strategies/mod.rs`의 `StrategyRegistry::new()`에 등록
3. 기존 테스트 통과 확인

### 라이브 트레이딩 관련 수정 시
- `commands/trading.rs`는 실제 주문 실행 — 변경 시 신중히
- 매수/매도 로직 변경 시 반드시 시뮬레이션으로 검증
- Upbit API 호출은 레이트 리밋 고려

### Mutex + async 주의
- `Mutex<Connection>` 잠금을 `.await` 전에 반드시 해제 (MutexGuard는 Send 아님)
- 패턴: `{ let db = state.db.lock().unwrap(); /* 동기 작업 */ }` → 이후 `.await`

### 테스트 체크리스트
- [ ] `cargo build --features tauri-app` 성공
- [ ] `cargo test` 통과
- [ ] `npm run vite:build` 성공 (프론트엔드 타입 에러 없음)
