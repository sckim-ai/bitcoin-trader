# 승급 시 다계정 그룹 fan-out — 설계 문서

작성일: 2026-06-08
상태: 승인됨 (구현 대기)

## TL;DR

- **목표**: 라이브 오토트레이딩에서 한 세션을 real로 승급할 때 **여러 Upbit 계정을 한 번에 선택**하고, 만들어진 계정별 real 세션들을 **그룹으로 계속 관리**한다. 동기는 "설정/관리 수고 줄이기".
- **방식 (접근 A)**: 라이브 핵심 로직(엔진·체결·미체결 추적·유니크 인덱스)을 **전혀 건드리지 않고**, 기존 "세션 1개 = 계정 1개" 실행 단위를 재사용한다. 승급 시 계정마다 real 세션을 fan-out(복제) 생성하고 `group_id`로 묶는다.
- **산출물**: 마이그레이션 1개(`group_id` 컬럼), `toggle_session_mode` 다계정화, 그룹 커맨드 3종(stop/demote/delete), 프론트(PromoteRealDialog 다중선택 · SessionTable 그룹 표시 · lib/store/types).
- **예상 규모**: 중간. 백엔드 repo/command + 프론트 UI. 가장 위험한 실주문 경로는 무변경이라 리스크 낮음.

## 배경 / 현재 구조

- 세션 1개 = 프리셋(전략) 1개 + 마켓 1개 + 실계정 **1개**(`live_sessions.upbit_account_id`, 단수).
- `live_scheduler::run_loop`가 매시간 running 세션을 **개별 순회**하며 `run_session_cycle`을 돌린다. 즉 계정당 세션을 따로 두면 각자 사이클을 받는다.
- 부분 유니크 인덱스 `idx_session_account_running_real`이 **같은 계정에 running real 세션 2개**를 차단(계정당 real 1개).
- 현재는 동일 전략을 2계정에 돌리려면 세션을 2개 따로 만들어 각각 승급해야 함(예: 세션 27→계정1, 28→계정2). 이 수작업을 없애는 게 본 기능.
- 승급은 `toggle_session_mode`(모드 토글)이며 status와 독립. 승급 다이얼로그(`PromoteRealDialog.tsx`)는 현재 계정 **단일 선택**.

## 비목표 (YAGNI)

- 단일 세션 1행이 N계정을 직접 구동하는 구조(접근 B) — 채택 안 함.
- 계정별 자본 분산(각 계정 다른 initial_capital) — 동기에서 제외됨. 전 멤버 동일 자본.
- 운영 중 그룹에 계정 추가/제거 편집 — v1 범위 밖. demote→paper 후 재승급으로 대체.
- 그룹 통합 모니터링 대시보드(합산 손익 등) — v1 범위 밖.

## 데이터 모델

마이그레이션 `migrations/018_session_group.sql`:

```sql
-- 018_session_group.sql
-- 다계정 승급 그룹: 같은 group_id를 가진 real 세션들을 한 단위로 관리.
-- group_id 값은 그룹을 만든 "리드 세션"의 id를 그대로 사용한다.
ALTER TABLE live_sessions ADD COLUMN group_id INTEGER;  -- NULL = 단독 세션
CREATE INDEX IF NOT EXISTS idx_live_sessions_group ON live_sessions(group_id);
```

- ALTER ADD COLUMN은 idempotent가 아니므로 `schema.rs`에서 "duplicate column" 에러를 swallow하는 기존 패턴을 따른다.
- `group_id = NULL`: 단독 세션 → **현재 동작과 완전히 동일**(하위호환).
- 단일 계정 승급(N=1): `group_id`를 NULL로 유지 → 기존 단일 승급과 동일.
- 다계정 승급(N≥2): 리드 포함 전원 `group_id = <리드 세션 id>`.
- 유니크 인덱스는 변경 없음. 멤버끼리 계정이 모두 다르므로 위반 없음.

## 백엔드 변경

### 2.1 승급 커맨드 `toggle_session_mode` (commands/live_trading.rs)

`ToggleSessionModeArgs.upbit_account_id: Option<i64>` → `upbit_account_ids: Vec<i64>`로 확장.
paper로 되돌리는 경로는 변경 없음(계정 바인딩 보존). real 승급 경로를 **하나의 트랜잭션**으로 처리:

```
real 승급(accounts = [a0, a1, ..., aN]):
  0. accounts 비어있으면 에러("Upbit 계정을 선택하세요.")
  1. 각 계정에 대해 기존 검증 반복: 존재/enabled/API 키 보유
  2. 트랜잭션 시작
  3. 리드 세션: mode=real, upbit_account_id=a0  (set_session_mode_real 재사용)
  4. N >= 2 인 경우:
       - 리드 세션에 group_id = 리드.id 기록
       - a1..aN 각 계정마다 클론 세션 insert:
           label            = 리드.label                 (동일 라벨)
           preset_id/market/initial_capital/max_order_krw = 리드 값 복사
           mode   = "real"
           status = 리드.status                            (running이면 running)
           upbit_account_id = ai
           group_id         = 리드.id
           current_position/current_buy_price/current_buy_volume = 리드 값(시드)
  5. 유니크 인덱스 위반 발생 시 트랜잭션 롤백 → "계정 [라벨]에 이미 실행 중인 real 세션이 있습니다." 한글 에러
  6. 커밋
  7. 리드 status == "running" 이면, 새로 만든 클론마다 즉시 1사이클 kick
     (start_session의 async spawn 패턴 재사용 — 다음 정시까지 안 기다림)
```

- 클론의 `current_position`은 시드값일 뿐이며, 첫 사이클의 `reconcile_position`이 **각 계정 실잔고로 보정**한다(검증된 기존 경로). 따라서 paper 포지션 복사가 안전하다.
- 트랜잭션으로 묶어, 중간 계정에서 유니크 위반이 나면 리드 승급과 클론 생성이 **모두 롤백**되어 부분 적용을 방지.

### 2.2 repo 함수 (db/live_repo.rs)

- `insert_session`에 `group_id: Option<i64>` 파라미터 추가(또는 별도 `set_session_group(conn, id, group_id)` 헬퍼). 기존 호출부(create_session)는 None 전달.
- `set_session_group(conn, id, group_id)` — 리드에 group_id 기록용.
- `list_group_session_ids(conn, group_id) -> Vec<i64>` — 그룹 멤버 조회.
- `LiveSession` 모델 + `list_sessions`/`get_session` SELECT에 `group_id` 추가.

### 2.3 그룹 커맨드 3종 (신규, commands/live_trading.rs)

기존 단일 커맨드를 재사용하는 얇은 래퍼:

```rust
stop_session_group(group_id)    // 멤버 전원 set_session_status("stopped") + paper_session_ids에서 제거
demote_session_group(group_id)  // 멤버 전원 set_session_mode("paper")  (계정 바인딩 보존)
delete_session_group(group_id)  // 멤버 전원 delete_session + paper_session_ids 정리
```

- 각 래퍼는 `list_group_session_ids`로 멤버 id를 모아 기존 로직을 루프.
- `lib.rs`의 `invoke_handler![]`에 3종 등록.

## 프론트엔드 변경

### 3.1 PromoteRealDialog.tsx
- `select` 단일 선택 → **체크박스 다중 선택** 목록.
- `onConfirm(accountId: number)` → `onConfirm(accountIds: number[])`.
- 비활성/타 real 실행 중 계정은 체크 불가(기존 disable 로직 유지).
- 선택 0개면 Promote 버튼 비활성.
- 안내 문구 업데이트: 선택한 N개 계정에 각각 real 세션이 생성되어 그룹으로 묶인다는 설명.

### 3.2 SessionTable.tsx
- `group_id`가 같은 세션을 묶어 **그룹 헤더 행 + 멤버 행**으로 렌더(또는 멤버 행 들여쓰기 + 그룹 배지).
- 그룹 헤더에 그룹 단위 stop/demote/delete 버튼. 멤버 개별 행의 기존 버튼도 유지(부분 조작용).
- `group_id = null` 세션은 현재처럼 단독 행.
- 계정 컬럼으로 동일 라벨 멤버를 구분.

### 3.3 lib/live.ts · store · types
- `toggleSessionMode(id, mode, accountIds?: number[])` 시그니처 변경 + Tauri/HTTP 양쪽 경로.
- 그룹 커맨드 3종 래퍼 추가(Tauri+HTTP).
- `liveTradingStore`에 그룹 액션 연결.
- `types/index.ts` `LiveSession`에 `group_id: number | null` 추가.

## 엣지 케이스 & 불변식

| 상황 | 처리 |
|------|------|
| 다계정 승급 중 1개 계정이 이미 real 실행 중 | 트랜잭션 롤백 → 아무 세션도 생성/변경 안 됨 + 한글 에러 |
| 그룹 일부만 stop/조작 | 멤버 개별 버튼 유지(그룹 버튼은 일괄용) |
| 그룹 demote→paper 후 재승급 | 계정 바인딩 보존 채로 paper, 다시 그룹 승급 가능 |
| 계정 삭제(ON DELETE SET NULL) | 멤버 `upbit_account_id`=NULL → 다음 사이클 real 가드가 에러 로그(기존 동작) |
| 라벨 중복 | 동일 라벨 유지 + 계정 컬럼/그룹 묶음으로 구분 |
| N=1 승급 | group_id NULL 유지 → 기존 단일 승급과 동일(하위호환) |

## 테스트 계획

- repo: `group_id` 저장/조회, `list_group_session_ids` 정확성.
- 승급 fan-out: N계정 입력 → N개 세션, 전원 `group_id == 리드.id`, 계정 매핑 정확.
- 트랜잭션 롤백: 중복(이미 real 실행 중) 계정 포함 시 0개 생성·리드 mode 미변경.
- 그룹 커맨드: stop/demote/delete가 멤버 전원에 적용됨.
- 하위호환: 단일 계정 승급은 group_id NULL, 기존 동작 유지.
- `cargo build --features tauri-app` + `cargo test` + `npm run vite:build` 통과.

## 매뉴얼

`Manual/auto-trading.md`(또는 신규 섹션)에 다계정 그룹 승급/그룹 관리 흐름 추가.

## 공통 실수 방지 체크리스트 (CLAUDE.md)

- [ ] Tauri 커맨드 추가: 핸들러 작성 → `invoke_handler![]` 등록 → `api.ts` Tauri+HTTP → `types/index.ts` 동기화
- [ ] Mutex+async: `.await` 전 MutexGuard 해제
- [ ] 라이브 트레이딩 핵심 경로(엔진/체결) 무변경 확인
