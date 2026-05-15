# Multi Upbit Accounts: 계정별 독립 자동매매

**Date**: 2026-05-15
**Status**: Design v2 (코드 리뷰 반영 — multi-real 정책 제거, pending tracker 재구조화, async/Mutex 흐름 명시)
**Scope**: 여러 Upbit 계정을 등록하고, 계정마다 독립적으로 1개의 자동매매 세션을 운영
**Trading target**: 현재 ETH 단독 자동매매 구조 유지, 계정 격리만 추가

---

## 1. 목적과 동기

현재 시스템은 OS keyring에 **단일 Upbit 키 쌍**만 저장한다. 사용자는 가족/법인 등 **복수 Upbit 계정**으로 각기 다른(또는 같은) 전략을 동시 운영하고 싶다. 키링 ID가 상수(`upbit_access_key`)고 `LiveSession`에 계정 참조가 없는 게 유일한 구조적 장벽이다.

이 설계는 **계정을 1급 엔티티로 분리**하고 세션이 계정을 참조하게 만들어, 백엔드 핫패스에 최소 변경으로 멀티 계정 자동매매를 가능하게 한다.

## 2. 핵심 결정사항

| 항목 | 결정 | 근거 |
|------|------|------|
| 계정-세션 바인딩 | **1 계정 = 1 running 세션** | 사용자 멘탈 모델 단순, 같은 계정 두 세션이 KRW 잔고 충돌하는 경계 케이스 원천 차단 |
| 강제 방식 | **DB 부분 유니크 인덱스** (`WHERE status='running' AND upbit_account_id IS NOT NULL`) | NULL은 SQLite에서 서로 다르게 취급되므로 명시적 IS NOT NULL 필요 |
| 기존 multi-real=1 정책 | **제거** — `count_real_sessions` 호출 삭제 | 본 설계의 핵심 목표 (멀티 계정 동시 real)와 직접 충돌하는 기존 invariant |
| 키 저장소 | OS keyring, ID에 `_<account_id>` 접미사 | 자격증명 관리자에서 식별 가능, DELETE 시 정확한 항목 제거 |
| 계정 관리 UI | **별도 `/accounts` 페이지** | Settings에서 분리 — 계정이 단일 폼이 아닌 컬렉션이 됨 |
| 마이그레이션 | **자동 흡수** | 기존 단일 키를 `account_id=1, label='기본'`으로 자동 변환, 사용자 작업 0 |
| 운영 안전 | running 세션 있는 계정 **삭제·키수정 거부** | in-flight 주문과의 race 제거 |
| 알림 | **글로벌 webhook + `[label]` prefix** | 기존 Discord 라우팅 유지, 비용 최소 |
| 거래소 추상화 | **이번 작업 범위 외** | 멀티 거래소는 별개 설계, 본 작업은 Upbit 내 멀티 계정만 |

## 3. 데이터 모델

### 3.1 신규 테이블 (migration 014)

```sql
CREATE TABLE upbit_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    label TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, label)
);

ALTER TABLE live_sessions
    ADD COLUMN upbit_account_id INTEGER REFERENCES upbit_accounts(id) ON DELETE SET NULL;

-- A 시나리오 강제: 같은 계정에 running 세션 두 개 불가
-- NULL은 SQLite unique에서 서로 다르게 취급되므로 IS NOT NULL 명시
CREATE UNIQUE INDEX idx_session_account_running
    ON live_sessions(upbit_account_id)
    WHERE status = 'running' AND upbit_account_id IS NOT NULL;
```

**근거**:
- `enabled` 플래그: 키는 보존하면서 일시적으로 새 세션 막기. 삭제와 분리.
- `UNIQUE (user_id, label)`: 같은 사용자가 동일 라벨 두 개 못 만들도록 — UI에서도 의미 있는 에러.
- 부분 인덱스: stopped 세션 이력은 계정당 N개 가능, running만 1개로 제한.
- **`upbit_account_id`는 nullable**: 마이그레이션 안전성 (옛 paper-only 사용자의 세션은 NULL 가능). 신규 세션은 백엔드/UI에서 NOT NULL 강제.

### 3.1.1 기존 multi-real=1 정책 제거

현재 `db/live_repo.rs:174` `count_real_sessions()` 함수와 그 호출처(`commands/live_trading.rs::toggle_session_mode`)는 **전역 real 세션 1개** invariant를 강제한다. 본 설계는 "1 계정 = 1 running 세션, 계정 N개 = real N개"를 의미하므로 이 invariant를 **제거**한다:

- `count_real_sessions()` 함수 자체 삭제 (또는 deprecate)
- `toggle_session_mode`에서 promotion 시 호출하던 검사 제거
- 대신 신규 검사: `mode='real'`로 전환할 때 `upbit_account_id`가 NOT NULL이고 enabled이며 키링 항목 존재해야 함
- 부분 유니크 인덱스가 같은 계정에 두 번째 running real 시도를 DB 레벨에서 차단
- 기존 주석("stopped real session occupies the slot")도 함께 제거

### 3.2 Keyring 네이밍

| 기존 | 변경 후 |
|------|---------|
| `upbit_access_key` | `upbit_access_key_<account_id>` |
| `upbit_secret_key` | `upbit_secret_key_<account_id>` |

Service name(`bitcoin-trader`)은 유지. 환경변수 fallback(`UPBIT_ACCESS_KEY`/`UPBIT_SECRET_KEY`)은 마이그레이션이 끝난 시스템에선 더 이상 사용되지 않으므로 코드에서 제거 가능 — 단, **dev 워크플로 보호를 위해 한 릴리즈는 유지**하고 다음 단계에서 제거.

### 3.3 마이그레이션 (자동 흡수)

`db/schema.rs::initialize`의 **`seed_admin(&conn)?` 호출 다음**에 idempotent 후처리. `upbit_accounts.user_id REFERENCES users(id)` FK 때문에 user=1이 먼저 시드되어 있어야 한다.

```rust
fn migrate_legacy_upbit_key(conn: &Connection) -> Result<()> {
    // 1. account_id=1 이미 존재하면 skip (재실행 안전)
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM upbit_accounts WHERE id = 1", [], |r| r.get(0))?;
    if exists > 0 { return Ok(()); }

    // 2. 옛 keyring 항목 우선, 없으면 env 변수 → 그래도 없으면 무동작
    let read_kr = |k: &str| keyring::Entry::new("bitcoin-trader", k)
        .ok().and_then(|e| e.get_password().ok());
    let (access, secret) = match (read_kr("upbit_access_key"), read_kr("upbit_secret_key")) {
        (Some(a), Some(s)) => (a, s),
        _ => match (std::env::var("UPBIT_ACCESS_KEY").ok(), std::env::var("UPBIT_SECRET_KEY").ok()) {
            (Some(a), Some(s)) if !a.is_empty() && !s.is_empty() => (a, s),
            _ => return Ok(()),
        },
    };

    // 3. accounts 행 생성 (user_id=1 = seeded admin)
    conn.execute(
        "INSERT INTO upbit_accounts (id, user_id, label) VALUES (1, 1, '기본')", [])?;
    // 4. 새 네이밍으로 keyring 저장
    keyring::Entry::new("bitcoin-trader", "upbit_access_key_1")
        .and_then(|e| e.set_password(&access)).ok();
    keyring::Entry::new("bitcoin-trader", "upbit_secret_key_1")
        .and_then(|e| e.set_password(&secret)).ok();
    // 5. 옛 keyring 항목 삭제 (env 출처면 no-op)
    let _ = keyring::Entry::new("bitcoin-trader", "upbit_access_key")
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new("bitcoin-trader", "upbit_secret_key")
        .and_then(|e| e.delete_credential());
    // 6. 기존 세션을 계정 1에 연결 (paper/real 무관)
    conn.execute(
        "UPDATE live_sessions SET upbit_account_id = 1 WHERE upbit_account_id IS NULL", [])?;
    Ok(())
}
```

**Idempotency**: account_id=1 존재 확인으로 재부팅 시 무동작. 옛 키도 env도 없으면(클린 설치) 아무 일도 안 일어남 → 사용자가 Accounts 페이지에서 첫 계정 등록.

**환경변수 정책**: 마이그레이션 시점에 **1회만** 참조. 마이그레이션 후 런타임에서는 env 변수 일절 무시 — 멀티 계정에서 같은 env가 여러 계정으로 흘러들어가는 사고 방지. 클린 설치 + env만 있는 dev 환경에선 이 1회 마이그레이션이 dev 키를 account_id=1로 흡수해 줌.

## 4. 아키텍처

### 4.1 전체 구조

```
┌──────────── Frontend (React) ────────────┐
│  /accounts (신규)                          │
│   - AccountsPage                          │
│   - AccountCard, AddAccountDialog         │
│                                           │
│  /live (기존)                              │
│   - NewSessionDialog: 계정 드롭다운 추가    │
│   - SessionCard: [label] 배지 추가         │
│                                           │
│  /settings (기존)                          │
│   - Upbit 키 섹션 제거, /accounts 링크로  │
└───────────────┬───────────────────────────┘
                │ Tauri IPC / HTTP
┌───────────────┴───────────────────────────┐
│  Backend (Rust)                           │
│  commands/upbit_accounts.rs (신규)         │
│   - list / add / update / delete / test   │
│                                           │
│  commands/upbit_keys.rs (변경)             │
│   - load_upbit_keys(account_id) 시그니처   │
│   - 옛 단일 키 커맨드는 deprecate          │
│                                           │
│  services/session_engine.rs (호출 변경)    │
│   - upbit_client = client_for(session.account_id) │
│                                           │
│  db/schema.rs (migration 014 + 자동흡수)   │
└───────────────────────────────────────────┘
```

### 4.2 백엔드 변경점

#### 4.2.1 시그니처 변경

```rust
// 기존
pub fn load_upbit_keys() -> (Option<String>, Option<String>, &'static str)
pub fn upbit_client_or_err() -> Result<UpbitClient, String>

// 변경 후
pub fn load_upbit_keys(account_id: i64) -> Result<(String, String), String>
pub fn upbit_client_or_err(account_id: i64) -> Result<UpbitClient, String>
```

**Result로 변경**: 멀티 계정에서 "키 없음"은 항상 에러 상황(매핑 인덱스 잘못된 것). 옛 시그니처의 Option/source는 단일 키 fallback이 있을 때만 의미가 있었음.

#### 4.2.2 호출처 갱신

| 파일 | 변경 |
|------|------|
| `services/session_engine.rs` | cycle 시작 시 `session.upbit_account_id` → client |
| `services/auto_trader.rs` | `reconcile_position` 등에 account_id 인자 추가 |
| `services/pending_order_tracker.rs` | **재구조화 필요** — 자세히는 §4.2.5 |
| `commands/trading.rs` (수동매매) | **세션 컨텍스트의 account_id 사용** — `LiveLogPanel`에서 수동 매매는 항상 활성 세션에 종속되므로 프론트는 session_id만 전달, 백엔드가 `live_sessions.upbit_account_id` 조회 |
| `commands/live_trading.rs` | 세션 생성 시 account_id 검증 (enabled, 키 존재) |

#### 4.2.3 신규 Tauri 커맨드 (`commands/upbit_accounts.rs`)

```rust
#[tauri::command] pub async fn list_upbit_accounts(state: State<AppState>) -> Result<Vec<UpbitAccount>, String>
#[tauri::command] pub async fn add_upbit_account(label: String, access_key: String, secret_key: String, state: State<AppState>) -> Result<UpbitAccount, String>
#[tauri::command] pub async fn update_upbit_account(id: i64, label: Option<String>, access_key: Option<String>, secret_key: Option<String>, state: State<AppState>) -> Result<(), String>
#[tauri::command] pub async fn delete_upbit_account(id: i64, state: State<AppState>) -> Result<(), String>
#[tauri::command] pub async fn test_upbit_account_connection(id: i64) -> Result<usize, String>
#[tauri::command] pub async fn set_upbit_account_enabled(id: i64, enabled: bool, state: State<AppState>) -> Result<(), String>
```

**`add`** 동작 — `AppState.db`가 `std::sync::Mutex<Connection>`이라 MutexGuard를 `.await` 너머로 들 수 없다. 따라서 진짜 원자 트랜잭션 대신 **짧은 동기 작업 → lock 해제 → 비동기 검증 → 실패 시 보상 삭제** 패턴:

```rust
// Phase 1: 동기 — DB row + keyring write (lock은 이 블록 안에서만)
let new_id = {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("INSERT INTO upbit_accounts (user_id, label) VALUES (?1, ?2)",
        params![user_id, label])?;
    conn.last_insert_rowid()
}; // ← lock 해제
keyring::Entry::new("bitcoin-trader", &format!("upbit_access_key_{new_id}"))?
    .set_password(&access)?;
keyring::Entry::new("bitcoin-trader", &format!("upbit_secret_key_{new_id}"))?
    .set_password(&secret)?;

// Phase 2: 비동기 — Upbit 연결 테스트
let test_result = UpbitClient::new(access, secret).get_all_balances().await;

// Phase 3: 실패 시 보상 (DB row + keyring 모두 정리)
if let Err(e) = test_result {
    let _ = keyring::Entry::new("bitcoin-trader", &format!("upbit_access_key_{new_id}"))
        .and_then(|kr| kr.delete_credential());
    let _ = keyring::Entry::new("bitcoin-trader", &format!("upbit_secret_key_{new_id}"))
        .and_then(|kr| kr.delete_credential());
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM upbit_accounts WHERE id = ?1", params![new_id])?;
    return Err(format!("Upbit 연결 실패: {e}"));
}
```

**진짜 원자성은 불가능**: keyring과 네트워크는 DB 트랜잭션에 합칠 수 없음. 단계 사이 크래시 시 잔존하는 keyring 항목은 다음 부팅 시 `list_upbit_accounts`가 keyring 존재 여부를 함께 반환하므로 UI에서 "키 없는 계정" 또는 "고아 keyring"으로 식별 가능. 실용적으로는 보상 삭제로 충분.

**`update`/`delete` 가드**:
```rust
let has_running: i64 = conn.query_row(
    "SELECT COUNT(*) FROM live_sessions WHERE upbit_account_id = ?1 AND status = 'running'",
    [id], |r| r.get(0))?;
if has_running > 0 { return Err("실행 중인 세션이 있습니다. 먼저 세션을 중지하세요.".into()); }
```

라벨만 변경하는 update는 가드 면제(주문 경로 영향 없음).

#### 4.2.5 `pending_order_tracker` 재구조화

현재 `reconcile_pending_orders(db, upbit)`는 `list_pending_wait()`로 **모든 세션의 wait 주문**을 받아 단일 `UpbitClient`로 처리한다(`services/pending_order_tracker.rs:44-50`, `db/live_repo.rs:471`). 멀티 계정에서는 A 계정 client로 B 계정 주문 UUID를 `get_order/cancel_order`하게 되어 권한 오류·잘못된 취소가 발생할 수 있다.

**변경안**: tracker가 `pending_orders` → `live_sessions` JOIN으로 `upbit_account_id`를 함께 가져오고, 같은 account_id끼리 그룹핑해서 계정별 client로 처리한다.

```rust
pub async fn reconcile_pending_orders_all(db: &Arc<Mutex<Connection>>) -> Result<usize, BoxErr> {
    // JOIN 결과: Vec<(PendingOrder, Option<i64> /* account_id */)>
    let grouped: HashMap<i64, Vec<PendingOrder>> = {
        let conn = db.lock()?;
        live_repo::list_pending_wait_with_account(&conn)?
            .into_iter()
            .filter_map(|(p, aid)| aid.map(|id| (id, p)))  // NULL account는 skip (legacy)
            .fold(HashMap::new(), |mut acc, (aid, p)| {
                acc.entry(aid).or_default().push(p); acc
            })
    };
    let mut resolved = 0;
    for (account_id, pendings) in grouped {
        let client = match upbit_client_or_err(account_id) {
            Ok(c) => c,
            Err(e) => { live_log!("[pending_tracker] account {account_id} client error: {e}"); continue; }
        };
        resolved += reconcile_with_client(db, &client, pendings).await?;
    }
    Ok(resolved)
}
```

새 함수 `live_repo::list_pending_wait_with_account`:
```sql
SELECT po.uuid, po.session_id, ..., ls.upbit_account_id
FROM pending_orders po
LEFT JOIN live_sessions ls ON po.session_id = ls.id
WHERE po.status = 'wait'
ORDER BY po.placed_at ASC;
```

호출처(`session_engine` 또는 스케줄러)가 cycle 시작마다 `reconcile_pending_orders_all(db)`만 호출 — `upbit` 인자 제거.

#### 4.2.4 `lib.rs::invoke_handler`

신규 커맨드 6개 등록. 기존 `save_upbit_keys`/`clear_upbit_keys`/`test_upbit_connection`은 마이그레이션 안전을 위해 유지하되 deprecated 주석 추가.

### 4.3 프론트엔드 변경점

#### 4.3.1 `src/pages/AccountsPage.tsx` (신규)

```
┌─ Header ─────────────────────────┐
│ Upbit 계정                  [+ 추가] │
├──────────────────────────────────┤
│ ┌─ AccountCard ──────────────┐   │
│ │ [메인]            🟢 enabled │   │
│ │ Access: a3****....          │   │
│ │ 연결: ✓ 1.2s                │   │
│ │ [테스트] [수정] [삭제]      │   │
│ └────────────────────────────┘   │
│ ┌─ AccountCard ──────────────┐   │
│ │ [가족]            ⚪ disabled │   │
│ │ ...                         │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

- **AddAccountDialog**: 라벨/access/secret 입력 → 저장 시 "연결 테스트 중..." 로딩 → 성공 시 카드 추가, 실패 시 에러 토스트 (DB/keyring 모두 롤백됨)
- **EditDialog**: 라벨만 또는 키만 변경 가능. 키 변경 시 running 세션 가드 메시지 표시
- **삭제 확인**: `ConfirmDialog` (기존 컴포넌트 재사용)

#### 4.3.2 라우팅 (`src/App.tsx`)

```tsx
<Route path="/accounts" element={<AccountsPage />} />
```

사이드바에 "Accounts" 메뉴 1줄 (LiveTrading 위 또는 Settings 위).

#### 4.3.3 Settings 페이지

기존 Upbit 키 카드 컴포넌트 제거. 대체 카드:
```tsx
<Card>
  <h3>Upbit 계정</h3>
  <p>계정 관리는 Accounts 페이지로 이동했습니다.</p>
  <Link to="/accounts"><Button>Accounts 페이지로</Button></Link>
</Card>
```

#### 4.3.4 NewSessionDialog (live)

기존 폼에 `<Select>` 1개 추가:

```tsx
<Select label="Upbit 계정" value={accountId} onChange={...}>
  {accounts.map(a => (
    <Option value={a.id} disabled={a.hasRunningSession || !a.enabled}>
      {a.label} {a.hasRunningSession && '(실행 중)'} {!a.enabled && '(비활성)'}
    </Option>
  ))}
</Select>
```

계정 0개일 때: 드롭다운 자리에 안내 + `/accounts` 링크, 시작 버튼 disabled.

#### 4.3.5 LiveTradingPage / SessionCard

세션 행/카드에 계정 라벨 배지:
```tsx
<Badge>{session.account_label}</Badge>
```

`LiveSession` 직렬화 응답에 `account_label` 필드 추가 (백엔드 JOIN).

**ManualOrderCard 재작업**: 현재 `realSessions[0]` 자동 선택 로직(`src/components/live/ManualOrderCard.tsx:25,169`)은 multi-real에서 위험. 변경:
- 카드 상단에 "대상 세션" `<Select>` 추가, 옵션은 real 세션 라벨 (`[메인] ETH-V3` 형식)
- 0개 → 카드 disabled + "real 세션이 없습니다" 안내 (기존과 동일)
- 1개 → 자동 선택 (기존 동작 보존, 클릭 1회 절약)
- 2개+ → 명시적 선택 필수, 미선택 시 주문 버튼 disabled
- `manual_market_order`로 전달되는 `session_id`는 선택된 세션. account_id는 백엔드가 `live_sessions.upbit_account_id`로 조회

#### 4.3.6 `src/lib/api.ts` / `src/lib/live.ts`

신규 커맨드 6개는 **Tauri IPC만** 추가. PWA/HTTP는 본 작업 범위 외 — 라이브 트레이딩 자체가 현재 `src/lib/live.ts:18` 주석대로 Phase 1 desktop-only이고 Axum routes에도 live/account 경로가 존재하지 않는다. 멀티 계정도 동일 정책을 따른다. PWA 노출은 라이브 트레이딩 HTTP 노출 작업과 함께 묶어 후속 단계로 이관.

#### 4.3.7 `src/types/index.ts`

```ts
export type UpbitAccount = {
  id: number;
  user_id: number;
  label: string;
  enabled: boolean;
  created_at: string;
  has_access_key: boolean;  // 키링 존재 여부 (값은 노출 X)
  has_secret_key: boolean;
  has_running_session?: boolean;  // list 응답에 JOIN으로 포함
};
```

`LiveSession` 타입에 `upbit_account_id: number`, `account_label: string` 추가.

### 4.4 알림 (Discord)

`services/session_engine.rs`의 notification 빌더에서 메시지 prefix:

```rust
let prefix = format!("[{}] ", session.account_label);
let msg = format!("{}{}", prefix, original_msg);
```

`session.account_label`은 세션 로드 시 JOIN으로 함께 가져옴 (`live_repo::load_session`에서 `LEFT JOIN upbit_accounts`). cycle당 추가 DB 조회 0.

## 5. 데이터 흐름

### 5.1 계정 추가 시

```
User: Accounts → [+ 추가] → 라벨/키 입력
  ↓
Frontend: invoke('add_upbit_account', {...})
  ↓
Backend:
  1. 입력 검증
  2. BEGIN TRANSACTION
  3. INSERT upbit_accounts → id=2 (예)
  4. keyring write: upbit_access_key_2, upbit_secret_key_2
  5. UpbitClient::new(...).get_all_balances()
  6a. 성공 → COMMIT → 반환
  6b. 실패 → keyring delete + ROLLBACK → 에러
  ↓
Frontend: 카드 목록에 추가, 토스트
```

### 5.2 세션 시작 시

```
User: LiveTrading → [+ 새 세션] → 계정/프리셋 선택
  ↓
Frontend: invoke('start_live_session', { account_id, preset_id, ... })
  ↓
Backend:
  1. 계정 enabled 확인, 키링 존재 확인
  2. 같은 account_id의 running 세션 확인 (UNIQUE 인덱스가 이중 보호)
  3. INSERT live_sessions (upbit_account_id = ...)
  4. session_engine 시작
  ↓
Cycle:
  - session.upbit_account_id → upbit_client_or_err(id) → 주문/조회
```

### 5.3 계정 삭제 시

```
User: Accounts → [삭제] → ConfirmDialog
  ↓
Frontend: invoke('delete_upbit_account', { id })
  ↓
Backend:
  1. running 세션 카운트 → > 0 이면 거부
  2. DELETE FROM upbit_accounts WHERE id = ?
  3. FK `ON DELETE SET NULL`에 의해 과거 stopped 세션의 upbit_account_id 자동 NULL화
  4. keyring delete: upbit_access_key_<id>, upbit_secret_key_<id>
```

**FK 정책**: `ON DELETE SET NULL`. UI에서 NULL일 때 "[삭제됨]"으로 표시. `account_label_snapshot` 칼럼은 추가하지 않음 — 라벨 변경/삭제 이력 추적은 본 작업 범위 밖이고, 손익 수치는 NULL과 무관하게 보존됨. Simple is Best.

## 6. 에러 처리

| 시나리오 | 처리 |
|----------|------|
| `add_upbit_account` 라벨 중복 | UNIQUE 위반 → "이미 같은 이름이 있습니다" |
| `add` 연결 테스트 실패 | keyring/DB 롤백 + Upbit 에러 메시지 노출 |
| `delete` running 세션 존재 | 거부 + "먼저 세션을 중지하세요" |
| `update(key)` running 세션 존재 | 거부 + 동일 메시지 (라벨만 update는 허용) |
| 세션 시작 시 계정 disabled | 거부 + "계정이 비활성화 상태입니다" |
| Cycle 중 키링 읽기 실패 | 세션 자동 중지 + 알림 (기존 메커니즘 재사용) |
| 401 Unauthorized (잘못된 키) | 세션 자동 중지 + 알림 |
| 마이그레이션 중 keyring 쓰기 실패 | 로그 + DB row 롤백, 사용자는 Accounts 페이지에서 수동 재등록 |

## 7. 테스트

### 7.1 백엔드

- **마이그레이션 흡수 통합 테스트** (`tests/account_migration_test.rs` 신규):
  - keyring mock으로 옛 키 주입 → `initialize()` 실행 → `upbit_accounts.id=1` 존재 + 새 네이밍 keyring 존재 + 옛 항목 부재 검증
  - 옛 키 없는 경우 → `upbit_accounts` 빈 상태 유지 검증
- **UNIQUE 인덱스 검증** (`tests/account_session_uniqueness_test.rs`):
  - 같은 account_id로 running 세션 두 개 시도 → 두 번째 INSERT 실패
- **기존 통합 테스트 갱신**: `pending_orders_test.rs`, `closed_bar_policy_test.rs` 등이 `upbit_account_id` 파라미터를 받도록

### 7.2 프론트엔드

- **AccountsPage 렌더링 테스트**: 가벼운 단위 (기존 `duplicateMarkers.test.ts` 패턴)
- **AddAccountDialog 폼 검증**: 빈 입력 거부, 라벨 길이 등
- 풀 e2e는 범위 외

### 7.3 수동 검증 체크리스트

- [ ] 기존 단일 키로 빌드된 DB가 새 버전 첫 부팅 시 자동 흡수
- [ ] 옛 환경변수 `UPBIT_ACCESS_KEY` 만으로 운영 중인 dev가 깨지지 않음 (한 릴리즈 유지)
- [ ] 계정 2개 추가 후 각각 다른 전략 세션 시작 → 양쪽 정상 cycle
- [ ] 한 계정에 running 세션 있는 상태에서 두 번째 세션 시작 시도 → 거부
- [ ] running 세션 있는 계정 삭제 시도 → 거부
- [ ] 세션 카드 / Discord 메시지에 `[label]` prefix 표시

## 8. 마이그레이션 / 롤아웃

1. **v1**: 자동 흡수 + 새 커맨드 + Accounts 페이지. 옛 환경변수 fallback 유지(`UPBIT_ACCESS_KEY`).
2. **v2** (다음 릴리즈): 옛 환경변수 fallback 제거, 옛 단일 키 keyring 항목 정리.
3. 데이터 손실 위험 없음 — 옛 키는 새 위치로 복사 후 삭제, 실패 시 옛 위치 보존.

## 9. 코드 리뷰 반영 결정사항

스펙 v1 리뷰에서 제기된 open question 결정:

| 질문 | 결정 | 근거 |
|------|------|------|
| paper session도 account 필수? | **신규 세션은 필수, 마이그레이션 세션은 NULL 허용** | DB 컬럼은 nullable로 두되 백엔드/UI가 새 세션 생성 시 NOT NULL 강제. 옛 paper-only 사용자 보호. |
| `account_label_snapshot` 칼럼 | **추가 안 함** | 손익은 `trades`에 session_id 기준으로 보존됨. 계정 삭제는 드문 운영이고 UI가 NULL에 "[삭제됨]" 표시 — Simple is Best. 추후 감사 요구가 강해지면 별도 작업. |
| env fallback 정책 | **마이그레이션 1회만 참조, 런타임 무시** | 멀티 계정 환경에서 모든 계정이 같은 env를 읽는 사고 방지. clean install + env dev 환경은 자동 흡수로 1회 보호. |

## 10. 범위 외 (Out of Scope)

- **거래소 추가** (Bithumb 등): 멀티 거래소 추상화는 별도 설계
- **계정별 Discord webhook 분리**: 글로벌 + label prefix로 충분 (질문 3-A 선택)
- **계정별 일일손실한도 / 안전한도 오버라이드**: 현재 세션 단위(`max_daily_loss_pct`) 유지
- **계정 그룹화 / 폴더**: 소수 운영이라 불필요
- **권한 분리** (예: 조회 전용 키 vs 거래 키): Upbit API 권한 모델을 그대로 사용, UI에서 노출 안 함
- **PWA / HTTP 노출**: 라이브 트레이딩 전체가 Phase 1 desktop-only이므로 계정 관리도 동일. 후속 단계에서 라이브 HTTP 노출과 함께 묶음.

## 11. 영향 받는 파일

### 신규
- `src-tauri/migrations/014_upbit_accounts.sql`
- `src-tauri/src/commands/upbit_accounts.rs`
- `src-tauri/src/models/upbit_account.rs`
- `src-tauri/src/db/upbit_accounts_repo.rs`
- `src-tauri/tests/account_migration_test.rs`
- `src-tauri/tests/account_session_uniqueness_test.rs`
- `src/pages/AccountsPage.tsx`
- `src/components/accounts/AccountCard.tsx`
- `src/components/accounts/AddAccountDialog.tsx`
- `src/components/accounts/EditAccountDialog.tsx`

### 수정
- `src-tauri/src/db/schema.rs` — migration 014 등록 + 자동 흡수
- `src-tauri/src/commands/upbit_keys.rs` — 시그니처에 account_id, 옛 커맨드 deprecate
- `src-tauri/src/commands/trading.rs` — account_id 인자 추가
- `src-tauri/src/commands/live_trading.rs` — 세션 생성 검증
- `src-tauri/src/services/session_engine.rs` — client 생성 시 account_id 사용 + label prefix
- `src-tauri/src/services/auto_trader.rs` — account_id 전파
- `src-tauri/src/services/pending_order_tracker.rs` — **재구조화**: pending을 account_id로 그룹핑, 계정별 client로 reconcile (§4.2.5)
- `src-tauri/src/models/live.rs` — `LiveSession.upbit_account_id`, `account_label` 추가
- `src-tauri/src/db/live_repo.rs` — JOIN 추가, `count_real_sessions` 제거, `list_pending_wait_with_account` 신설
- `src-tauri/src/commands/live_trading.rs` — `toggle_session_mode`에서 `count_real_sessions` 검사 제거
- `src-tauri/src/lib.rs` — invoke_handler에 신규 커맨드 6개 등록
- `src/App.tsx` — 라우트 1줄
- `src/pages/SettingsPage.tsx` (또는 Settings 컴포넌트) — Upbit 섹션 제거, 안내로 대체
- `src/pages/LiveTradingPage.tsx` — 계정 배지
- `src/components/live/NewSessionDialog.tsx` — 계정 드롭다운
- `src/components/live/ManualOrderCard.tsx` — `realSessions[0]` 자동선택 제거, 명시적 세션 셀렉터 추가
- `src/lib/api.ts` — 신규 커맨드 6개
- `src/types/index.ts` — UpbitAccount 타입, LiveSession 확장
- `History.md`, `Manual/` (작업 완료 후 업데이트)
