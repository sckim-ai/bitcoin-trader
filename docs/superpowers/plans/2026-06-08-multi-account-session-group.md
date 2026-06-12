# 다계정 그룹 fan-out Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 라이브 세션을 real로 승급할 때 여러 Upbit 계정을 한 번에 선택해 계정별 real 세션을 fan-out 생성하고, `group_id`로 묶어 그룹 단위로 stop/demote/delete 한다.

**Architecture:** 기존 "세션 1개 = 계정 1개" 실행 단위를 재사용한다(엔진/스케줄러/체결/미체결/유니크 인덱스 무변경). 승급 시 리드 세션을 첫 계정에 바인딩하고, 나머지 계정마다 리드 행을 `INSERT ... SELECT`로 복제한다. 모든 멤버는 리드 세션 id를 `group_id`로 공유한다. N=1 승급은 `group_id`를 NULL로 두어 기존 동작과 100% 동일.

**Tech Stack:** Rust (rusqlite, tauri command), React 19 + TypeScript, Zustand, SQLite.

**설계 문서:** `docs/superpowers/specs/2026-06-08-multi-account-session-group-design.md`

---

## File Structure

- `src-tauri/migrations/018_session_group.sql` (생성) — `group_id` 컬럼 + 인덱스.
- `src-tauri/src/db/schema.rs` (수정) — 018 마이그레이션 등록.
- `src-tauri/src/models/live.rs` (수정) — `LiveSession.group_id` 필드.
- `src-tauri/src/db/live_repo.rs` (수정) — `group_id` SELECT/매핑, `insert_real_clone_session`, `set_session_group`, `list_group_session_ids`.
- `src-tauri/src/commands/live_trading.rs` (수정) — `toggle_session_mode` 다계정화 + 그룹 커맨드 3종.
- `src-tauri/src/lib.rs` (수정) — 그룹 커맨드 invoke_handler 등록.
- `src/types/index.ts` (수정) — `LiveSession.group_id`.
- `src/lib/live.ts` (수정) — `toggleSessionMode` 시그니처 + 그룹 커맨드 래퍼.
- `src/stores/liveTradingStore.ts` (수정) — 그룹 액션.
- `src/components/live/PromoteRealDialog.tsx` (수정) — 계정 다중 선택.
- `src/components/live/SessionTable.tsx` (수정) — 그룹 표시 + 그룹 버튼.
- `src/pages/LiveTradingPage.tsx` (수정) — 다이얼로그/그룹 핸들러 배선.
- `Manual/auto-trading.md` (수정) — 다계정 그룹 매뉴얼.

---

### Task 1: 마이그레이션 018 — group_id 컬럼

**Files:**
- Create: `src-tauri/migrations/018_session_group.sql`
- Modify: `src-tauri/src/db/schema.rs:112-118` (017 블록 바로 뒤에 018 블록 추가)

- [ ] **Step 1: 마이그레이션 SQL 작성**

`src-tauri/migrations/018_session_group.sql`:

```sql
-- 018_session_group.sql
-- 다계정 승급 그룹: 같은 group_id를 가진 real 세션들을 한 단위로 관리한다.
-- group_id 값은 그룹을 만든 "리드 세션"의 id를 그대로 사용한다.
-- NULL = 단독 세션(기존 동작과 동일).
ALTER TABLE live_sessions ADD COLUMN group_id INTEGER;
CREATE INDEX IF NOT EXISTS idx_live_sessions_group ON live_sessions(group_id);
```

- [ ] **Step 2: schema.rs에 018 등록**

`src-tauri/src/db/schema.rs`의 017 블록(`schema_v17` … 닫는 `}`) 바로 다음, `optimization_runs` backfill UPDATE 앞에 삽입:

```rust
    let schema_v18 = include_str!("../../migrations/018_session_group.sql");
    if let Err(e) = conn.execute_batch(schema_v18) {
        let msg = e.to_string();
        if !msg.contains("duplicate column") && !msg.contains("already exists") {
            return Err(e);
        }
    }
```

- [ ] **Step 3: 빌드 확인**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 컴파일 성공(경고만 가능). 마이그레이션은 다음 부팅/테스트 때 적용됨.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/migrations/018_session_group.sql src-tauri/src/db/schema.rs
git commit -m "feat(live): add group_id column for multi-account session groups"
```

---

### Task 2: 모델 + repo — group_id 매핑 및 그룹 헬퍼

**Files:**
- Modify: `src-tauri/src/models/live.rs` (LiveSession 구조체)
- Modify: `src-tauri/src/db/live_repo.rs` (SELECT 3곳, row_to_session, 신규 함수 3개)
- Test: `src-tauri/src/db/live_repo.rs` (하단 `#[cfg(test)]`)

- [ ] **Step 1: LiveSession 구조체에 group_id 추가**

`src-tauri/src/models/live.rs`의 `pub struct LiveSession { ... }` 닫는 `}` 직전(마지막 필드 뒤)에 추가:

```rust
    /// 다계정 승급 그룹 식별자. NULL = 단독 세션. 같은 값을 가진 세션들은
    /// 한 번의 승급으로 fan-out된 형제 세션이며 그룹 단위로 관리된다.
    pub group_id: Option<i64>,
```

- [ ] **Step 2: 3개 SELECT에 ls.group_id 추가**

`live_repo.rs`의 `get_session`, `list_sessions`, `list_running_sessions` 각 SQL에서
`ls.notify_discord, ls.notify_account_ids` 다음에 `, ls.group_id`를 추가한다. 예 (`get_session`):

```rust
                ls.notify_discord, ls.notify_account_ids, ls.group_id
         FROM live_sessions ls
```

세 함수 모두 동일하게 `ls.notify_account_ids` → `ls.notify_account_ids, ls.group_id`로 변경.

- [ ] **Step 3: row_to_session에 group_id 매핑 추가**

`row_to_session`의 `notify_account_ids: ... .unwrap_or_default(),` 다음(닫는 `})` 전)에 추가:

```rust
        group_id: row.get(26)?,
```

- [ ] **Step 4: 그룹 repo 헬퍼 3개 추가**

`set_session_mode_real` 함수 바로 아래에 추가:

```rust
/// 세션에 group_id를 기록(다계정 승급 리드 세션용).
pub fn set_session_group(conn: &Connection, id: i64, group_id: i64) -> Result<usize> {
    conn.execute(
        "UPDATE live_sessions SET group_id = ?1 WHERE id = ?2",
        params![group_id, id],
    )
}

/// 그룹의 모든 멤버 세션 id를 반환(생성 순).
pub fn list_group_session_ids(conn: &Connection, group_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM live_sessions WHERE group_id = ?1 ORDER BY id",
    )?;
    let ids = stmt
        .query_map([group_id], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>>>()?;
    Ok(ids)
}

/// 리드 세션 행을 복제해 새 real 세션을 만든다(다계정 승급용). 리드의
/// preset/market/capital/position/limits/notify 설정을 그대로 복사하고
/// mode='real', 새 계정/그룹/타임스탬프만 덮어쓴다. status는 리드를 따른다
/// (running이면 스케줄러가 다음 정시에 자동으로 사이클을 돌린다).
/// 포지션은 시드값이며 첫 사이클의 reconcile_position이 실계정 잔고로 보정한다.
pub fn insert_real_clone_session(
    conn: &Connection,
    lead_id: i64,
    account_id: i64,
    group_id: i64,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO live_sessions
            (user_id, label, preset_id, market, mode, status, initial_capital,
             start_ts, real_started_at, last_cycle_ts, last_signal,
             current_position, current_buy_price, current_buy_volume,
             current_equity, live_return, max_daily_loss_pct, max_daily_trades,
             max_order_krw, created_at, upbit_account_id, notify_discord,
             notify_account_ids, group_id)
         SELECT user_id, label, preset_id, market, 'real', status, initial_capital,
             start_ts, ?2, NULL, last_signal,
             current_position, current_buy_price, current_buy_volume,
             current_equity, 0.0, max_daily_loss_pct, max_daily_trades,
             max_order_krw, ?2, ?3, notify_discord,
             notify_account_ids, ?4
         FROM live_sessions WHERE id = ?1",
        params![lead_id, now, account_id, group_id],
    )?;
    Ok(conn.last_insert_rowid())
}
```

- [ ] **Step 5: 실패 테스트 작성**

`live_repo.rs` 하단 `#[cfg(test)] mod tests` 안에 추가. 기존 테스트가 쓰는 셋업 헬퍼(같은 모듈의 `setup`/`mk_*` 류)를 그대로 따른다. 기존 테스트에서 세션 생성에 쓰는 방식(`insert_session(&conn, 1, "lbl", preset_id, "ETH", "paper", 100000.0, "2026-01-01T00:00:00Z", None)`)을 참고:

```rust
#[test]
fn clone_session_copies_lead_and_sets_group() {
    let conn = test_conn(); // 기존 테스트의 in-memory conn 헬퍼 사용
    let preset_id = insert_test_preset(&conn); // 기존 헬퍼 사용
    let lead = insert_session(
        &conn, 1, "grp", preset_id, "ETH", "paper", 500000.0,
        "2026-01-01T00:00:00Z", None,
    ).unwrap();
    set_session_status(&conn, lead, "running").unwrap();
    set_session_mode_real(&conn, lead, 1).unwrap();
    set_session_group(&conn, lead, lead).unwrap();

    let clone = insert_real_clone_session(&conn, lead, 2, lead).unwrap();

    let c = get_session(&conn, clone).unwrap().unwrap();
    assert_eq!(c.label, "grp");
    assert_eq!(c.mode, "real");
    assert_eq!(c.status, "running");      // 리드 status 상속
    assert_eq!(c.upbit_account_id, Some(2));
    assert_eq!(c.group_id, Some(lead));
    assert_eq!(c.initial_capital, 500000.0);

    let members = list_group_session_ids(&conn, lead).unwrap();
    assert_eq!(members, vec![lead, clone]);
}
```

> 참고: `test_conn`/`insert_test_preset` 이름이 기존 테스트와 다르면 기존 모듈의 헬퍼명으로 맞춘다. upbit_accounts FK가 필요하면 기존 계정 테스트 헬퍼로 계정 1·2를 먼저 만든다(또는 014 인덱스가 in-memory에 적용되는지 확인 — 적용 안 되면 account_id FK는 NULL 허용이므로 임의 정수도 통과).

- [ ] **Step 6: 테스트 실패 확인**

Run: `cd src-tauri && cargo test clone_session_copies_lead_and_sets_group`
Expected: 컴파일 실패 또는 assert 실패(아직 group_id 미구현 상태에서 작성했다면 컴파일 에러 → Step 1~4 적용 후 통과).

- [ ] **Step 7: 테스트 통과 확인**

Run: `cd src-tauri && cargo test clone_session_copies_lead_and_sets_group`
Expected: PASS

- [ ] **Step 8: 전체 테스트 + 빌드**

Run: `cd src-tauri && cargo test && cargo build --features tauri-app`
Expected: 전부 PASS / 빌드 성공. (기존 테스트가 `group_id` 컬럼 부재로 깨지지 않는지 확인 — in-memory 스키마에 018이 포함돼야 한다. 누락 시 테스트 셋업의 마이그레이션 목록에 018 추가.)

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/models/live.rs src-tauri/src/db/live_repo.rs
git commit -m "feat(live): group_id model/repo + clone-session helper"
```

---

### Task 3: 승급 커맨드 다계정화

**Files:**
- Modify: `src-tauri/src/commands/live_trading.rs:301-359` (`ToggleSessionModeArgs`, `toggle_session_mode`)

- [ ] **Step 1: Args 다계정화**

`ToggleSessionModeArgs`를 다음으로 교체:

```rust
#[derive(serde::Deserialize)]
pub struct ToggleSessionModeArgs {
    pub id: i64,
    pub mode: String, // "paper" or "real"
    /// real 승급 시 연동할 계정 목록. 비어 있으면 세션에 이미 연결된 계정 1개를 사용.
    /// 2개 이상이면 첫 계정은 리드 세션에 바인딩, 나머지는 계정별 real 세션으로
    /// 복제 생성되며 모두 같은 group_id(리드 id)로 묶인다.
    /// paper로 되돌릴 때는 무시.
    #[serde(default)]
    pub upbit_account_ids: Vec<i64>,
}
```

- [ ] **Step 2: real 승급 경로를 트랜잭션 + fan-out으로 교체**

`toggle_session_mode`의 `if mode == "real" { ... }` 블록 전체를 아래로 교체. (paper 분기 `else { ... }`는 그대로 둔다.) `conn`을 `mut`로 받도록 함수 상단의 `let conn = state.db.lock()...`를 `let mut conn = state.db.lock()...`로 변경:

```rust
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;

    if mode == "real" {
        let session = live_repo::get_session(&conn, args.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("session {} not found", args.id))?;

        // 계정 목록 결정: 명시 목록 우선, 비면 세션에 이미 연결된 계정 1개.
        let account_ids: Vec<i64> = if !args.upbit_account_ids.is_empty() {
            args.upbit_account_ids.clone()
        } else {
            vec![session
                .upbit_account_id
                .ok_or_else(|| "Upbit 계정을 선택하세요.".to_string())?]
        };

        // 중복 제거(같은 계정 두 번 선택 방지).
        let mut seen = std::collections::HashSet::new();
        let account_ids: Vec<i64> = account_ids
            .into_iter()
            .filter(|id| seen.insert(*id))
            .collect();

        // 각 계정 사전 검증: 존재/활성/키. 한 곳이라도 실패면 아무것도 안 만든다.
        for &acc_id in &account_ids {
            let acc = crate::db::upbit_accounts_repo::get_account(&conn, acc_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "계정이 삭제되었거나 존재하지 않습니다.".to_string())?;
            if !acc.enabled {
                return Err(format!("계정 [{}]이(가) 비활성 상태입니다.", acc.label));
            }
            let (ha, hs) = crate::commands::upbit_keys::has_keys_for(acc_id);
            if !ha || !hs {
                return Err(format!("계정 [{}] API 키가 설정되지 않았습니다.", acc.label));
            }
        }

        let lead_account = account_ids[0];
        let extra_accounts = &account_ids[1..];
        let multi = !extra_accounts.is_empty();

        // 친절한 라벨 조회용 맵(유니크 위반 메시지에 사용).
        let label_for = |conn: &rusqlite::Connection, id: i64| -> String {
            crate::db::upbit_accounts_repo::get_account(conn, id)
                .ok()
                .flatten()
                .map(|a| a.label)
                .unwrap_or_else(|| format!("#{id}"))
        };

        let tx = conn.transaction().map_err(|e| e.to_string())?;

        // 리드 승급.
        if let Err(e) = live_repo::set_session_mode_real(&tx, args.id, lead_account) {
            let s = e.to_string().to_lowercase();
            return Err(if s.contains("unique") || s.contains("constraint") {
                format!("계정 [{}]에 이미 실행 중인 real 세션이 있습니다.", label_for(&tx, lead_account))
            } else {
                e.to_string()
            });
        }

        if multi {
            // 그룹 식별자 = 리드 세션 id.
            live_repo::set_session_group(&tx, args.id, args.id).map_err(|e| e.to_string())?;
            for &acc_id in extra_accounts {
                if let Err(e) = live_repo::insert_real_clone_session(&tx, args.id, acc_id, args.id) {
                    let s = e.to_string().to_lowercase();
                    return Err(if s.contains("unique") || s.contains("constraint") {
                        format!("계정 [{}]에 이미 실행 중인 real 세션이 있습니다.", label_for(&tx, acc_id))
                    } else {
                        e.to_string()
                    });
                }
            }
        }

        tx.commit().map_err(|e| e.to_string())?;

        // running 멤버를 paper_session_ids에 반영(기존 start/stop 패턴과 일관).
        if multi && session.status == "running" {
            let members = {
                live_repo::list_group_session_ids(&conn, args.id).map_err(|e| e.to_string())?
            };
            let mut set = state.paper_session_ids.lock().map_err(|e| e.to_string())?;
            for mid in members {
                set.insert(mid, ());
            }
        }
    } else {
        // paper로 되돌리기: 계정 연결은 보존, mode만 변경.
        live_repo::set_session_mode(&conn, args.id, mode).map_err(|e| e.to_string())?;
    }
    Ok(())
```

> 주의(Mutex+async): `toggle_session_mode`는 동기 `pub fn`이라 `.await`가 없다. MutexGuard를 그대로 써도 안전하다. `tx`는 commit/return 시 drop되어 guard보다 먼저 해제된다.
> `Transaction`은 `Deref<Target=Connection>`이므로 `&tx`가 `&Connection` 인자에 자동 강제변환된다.

- [ ] **Step 3: 빌드 확인**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 성공. (에러 시: `get_account`/`has_keys_for` 경로, `transaction()`에 `mut conn` 확인.)

- [ ] **Step 4: 승급 fan-out 테스트(통합)**

`src-tauri/tests/` 의 기존 통합 테스트 스타일을 따르거나, repo 레벨 단위테스트로 대체 가능하면 Task 2의 테스트로 충분하다. 커맨드 레벨 테스트가 어려우면(State 목 필요) 이 단계는 수동 검증으로 남기고 Step 5의 빌드로 갈음한다. 수동 검증 시나리오를 plan에 명시:
- 계정 2개 선택 승급 → `list_group_session_ids(lead)`가 2개, 둘 다 mode=real, 서로 다른 account_id.
- 이미 real 실행 중 계정 포함 → 에러 + 아무 세션도 안 생김(리드 mode도 paper 유지).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/live_trading.rs
git commit -m "feat(live): multi-account real promotion with transactional fan-out"
```

---

### Task 4: 그룹 관리 커맨드 3종

**Files:**
- Modify: `src-tauri/src/commands/live_trading.rs` (`stop_session`/`delete_session` 인근에 추가)
- Modify: `src-tauri/src/lib.rs:169-175` (invoke_handler 등록)

- [ ] **Step 1: 그룹 커맨드 작성**

`live_trading.rs`의 `delete_session` 함수 다음에 추가:

```rust
// ─── Group operations (다계정 승급 그룹) ───

/// 그룹의 모든 멤버 세션을 stopped로. paper_session_ids에서도 제거.
#[tauri::command]
pub fn stop_session_group(group_id: i64, state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let ids = live_repo::list_group_session_ids(&conn, group_id).map_err(|e| e.to_string())?;
    for id in &ids {
        live_repo::set_session_status(&conn, *id, "stopped").map_err(|e| e.to_string())?;
    }
    drop(conn);
    let mut set = state.paper_session_ids.lock().map_err(|e| e.to_string())?;
    for id in &ids {
        set.remove(id);
    }
    Ok(ids)
}

/// 그룹의 모든 멤버를 paper로 되돌림(계정 바인딩 보존).
#[tauri::command]
pub fn demote_session_group(group_id: i64, state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let ids = live_repo::list_group_session_ids(&conn, group_id).map_err(|e| e.to_string())?;
    for id in &ids {
        live_repo::set_session_mode(&conn, *id, "paper").map_err(|e| e.to_string())?;
    }
    Ok(ids)
}

/// 그룹의 모든 멤버 세션을 삭제.
#[tauri::command]
pub fn delete_session_group(group_id: i64, state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let ids = live_repo::list_group_session_ids(&conn, group_id).map_err(|e| e.to_string())?;
    for id in &ids {
        live_repo::delete_session(&conn, *id).map_err(|e| e.to_string())?;
    }
    drop(conn);
    let mut set = state.paper_session_ids.lock().map_err(|e| e.to_string())?;
    for id in &ids {
        set.remove(id);
    }
    Ok(ids)
}
```

- [ ] **Step 2: invoke_handler 등록**

`src-tauri/src/lib.rs`의 `live_trading::delete_session,` 다음 줄들에 추가:

```rust
                live_trading::stop_session_group,
                live_trading::demote_session_group,
                live_trading::delete_session_group,
```

- [ ] **Step 3: 빌드 확인**

Run: `cd src-tauri && cargo build --features tauri-app`
Expected: 성공.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/live_trading.rs src-tauri/src/lib.rs
git commit -m "feat(live): group stop/demote/delete commands"
```

---

### Task 5: 프론트 타입 + API 래퍼 + store

**Files:**
- Modify: `src/types/index.ts:244-245` (LiveSession에 group_id)
- Modify: `src/lib/live.ts:56-63` (toggleSessionMode) + 그룹 래퍼 추가
- Modify: `src/stores/liveTradingStore.ts` (그룹 액션 + toggleSessionMode 시그니처)

- [ ] **Step 1: 타입 추가**

`src/types/index.ts`의 `LiveSession`에서 `notify_account_ids?: number[];` 다음에 추가:

```ts
  /** 다계정 승급 그룹 id. null = 단독 세션. */
  group_id: number | null;
```

- [ ] **Step 2: live.ts — toggleSessionMode 다계정화 + 그룹 래퍼**

`toggleSessionMode`를 교체:

```ts
/// Promote a paper session to real (or demote back to paper).
///   - real 승급: accountIds 1개 이상. 비우면 세션에 이미 연결된 계정 사용.
///     2개 이상이면 계정별 real 세션이 fan-out 생성되어 group_id로 묶인다.
///   - paper 복귀: 계정 인자 무시, 기존 연결 보존.
export const toggleSessionMode = (
  id: number,
  mode: "paper" | "real",
  accountIds?: number[],
): Promise<void> =>
  invoke("toggle_session_mode", {
    args: { id, mode, upbit_account_ids: accountIds ?? [] },
  });

/// 그룹 단위 정지/강등/삭제. 영향받은 세션 id 목록 반환.
export const stopSessionGroup = (groupId: number): Promise<number[]> =>
  invoke("stop_session_group", { groupId });
export const demoteSessionGroup = (groupId: number): Promise<number[]> =>
  invoke("demote_session_group", { groupId });
export const deleteSessionGroup = (groupId: number): Promise<number[]> =>
  invoke("delete_session_group", { groupId });
```

- [ ] **Step 3: store — 그룹 액션 추가 + toggleSessionMode 시그니처**

`liveTradingStore.ts` 상단 import에 추가(기존 `toggleSessionMode as apiToggleMode` 인근):

```ts
  stopSessionGroup as apiStopGroup,
  demoteSessionGroup as apiDemoteGroup,
  deleteSessionGroup as apiDeleteGroup,
```

인터페이스 정의에서 `toggleSessionMode` 시그니처를 변경하고 그룹 액션 3개를 추가:

```ts
  toggleSessionMode: (id: number, mode: "paper" | "real", accountIds?: number[]) => Promise<void>;
  stopSessionGroup: (groupId: number) => Promise<void>;
  demoteSessionGroup: (groupId: number) => Promise<void>;
  deleteSessionGroup: (groupId: number) => Promise<void>;
```

구현부에서 `toggleSessionMode`를 교체하고 그룹 액션 추가:

```ts
  toggleSessionMode: async (id, mode, accountIds) => {
    await apiToggleMode(id, mode, accountIds);
    await get().refreshSessions();
  },

  stopSessionGroup: async (groupId) => {
    await apiStopGroup(groupId);
    await get().refreshSessions();
  },
  demoteSessionGroup: async (groupId) => {
    await apiDemoteGroup(groupId);
    await get().refreshSessions();
  },
  deleteSessionGroup: async (groupId) => {
    await apiDeleteGroup(groupId);
    await get().refreshSessions();
  },
```

- [ ] **Step 4: 빌드 확인**

Run: `npm run vite:build`
Expected: 타입 에러 없이 성공. (`toggleSessionMode` 호출부 LiveTradingPage가 아직 단일 인자라 타입 에러가 날 수 있음 → Task 7에서 교체. 여기서 에러 나면 Step은 통과로 보고 Task 7 직후 재확인.)

- [ ] **Step 5: Commit**

```bash
git add src/types/index.ts src/lib/live.ts src/stores/liveTradingStore.ts
git commit -m "feat(live): frontend types/api/store for account groups"
```

---

### Task 6: PromoteRealDialog 다중 선택

**Files:**
- Modify: `src/components/live/PromoteRealDialog.tsx`

- [ ] **Step 1: onConfirm 시그니처 + 다중 선택 상태로 교체**

`Props.onConfirm`을 `(accountIds: number[]) => Promise<void>`로 변경하고, 단일 `accountId` 상태를 `Set<number>`로 교체. select를 체크박스 목록으로 교체. 전체 컴포넌트를 다음으로 교체:

```tsx
import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { AlertTriangle } from "lucide-react";
import type { LiveSession, UpbitAccount } from "../../types";
import { listUpbitAccounts } from "../../lib/live";

interface Props {
  session: LiveSession;
  onClose: () => void;
  /** 선택된 계정 id 목록을 전달. 호출자가 toggleSessionMode(id, 'real', ids) 실행. */
  onConfirm: (accountIds: number[]) => Promise<void>;
}

/**
 * paper → real 승급 하드 컨펌 다이얼로그.
 * 계정을 1개 이상 선택한다. 2개 이상이면 계정별 real 세션이 그룹으로 생성된다.
 * 이미 다른 real이 실행 중인 계정은 비활성(부분 유니크 인덱스로 차단됨).
 * 이전에 연결된 계정은 기본 체크된다.
 */
export default function PromoteRealDialog({ session, onClose, onConfirm }: Props) {
  const [accounts, setAccounts] = useState<UpbitAccount[]>([]);
  const [selected, setSelected] = useState<Set<number>>(
    session.upbit_account_id != null ? new Set([session.upbit_account_id]) : new Set(),
  );
  const [loadingAccounts, setLoadingAccounts] = useState(true);
  const [acknowledged, setAcknowledged] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const previousAccountId = session.upbit_account_id;

  useEffect(() => {
    listUpbitAccounts()
      .then((list) => {
        setAccounts(list);
        // 연결된 계정이 없고 선택 가능한 계정이 1개뿐이면 자동 선택.
        if (previousAccountId == null) {
          const selectable = list.filter((a) => a.enabled && !a.has_running_session);
          if (selectable.length === 1) setSelected(new Set([selectable[0].id]));
        }
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoadingAccounts(false));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggle = (id: number) =>
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const handleConfirm = async () => {
    if (!acknowledged || selected.size === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm([...selected]);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="bg-zinc-900 border border-rose-900/60 rounded-xl p-6 w-[480px] space-y-4">
        <div className="flex items-center gap-2">
          <AlertTriangle size={20} className="text-rose-500" />
          <h3 className="text-lg font-semibold text-zinc-100">실거래 모드로 전환</h3>
        </div>

        <div className="text-sm text-zinc-300 space-y-2">
          <p>
            세션 <span className="font-medium text-amber-400">{session.label}</span>을(를)
            <span className="font-medium text-rose-400"> 실거래(real)</span> 모드로 전환합니다.
          </p>
          <p className="text-zinc-400 text-xs leading-relaxed">
            계정을 2개 이상 선택하면 계정마다 동일 전략의 real 세션이 생성되어 하나의 그룹으로 묶입니다.
            각 계정의 ETH/KRW 잔고가 직접 영향을 받습니다.
          </p>
        </div>

        <div>
          <label className="text-xs text-zinc-500 block mb-1">
            Upbit Accounts <span className="text-rose-400">*</span>
          </label>
          {loadingAccounts ? (
            <p className="text-xs text-zinc-500">계정 로딩 중...</p>
          ) : accounts.length === 0 ? (
            <p className="text-xs text-amber-400">
              등록된 계정이 없습니다. Accounts 페이지에서 먼저 등록하세요.
            </p>
          ) : (
            <div className="space-y-1 max-h-48 overflow-y-auto border border-zinc-700 rounded-lg p-2">
              {accounts.map((a) => {
                const selfOnAccount = a.id === previousAccountId;
                const blockedByOther = a.has_running_session && !selfOnAccount;
                const unavailable = !a.enabled || blockedByOther;
                return (
                  <label
                    key={a.id}
                    className={`flex items-center gap-2 text-sm px-1 py-0.5 rounded ${
                      unavailable ? "opacity-40 cursor-not-allowed" : "cursor-pointer hover:bg-zinc-800"
                    }`}
                  >
                    <input
                      type="checkbox"
                      disabled={unavailable}
                      checked={selected.has(a.id)}
                      onChange={() => toggle(a.id)}
                      className="accent-rose-500"
                    />
                    <span className="text-zinc-200">{a.label}</span>
                    <span className="text-[10px] text-zinc-500">
                      {!a.enabled ? "(비활성)" : blockedByOther ? "(다른 real 실행 중)" : ""}
                    </span>
                  </label>
                );
              })}
            </div>
          )}
          {previousAccountId != null && (
            <p className="text-[10px] text-zinc-500 mt-1">이전에 연결된 계정이 기본 선택됩니다.</p>
          )}
        </div>

        <div className="bg-zinc-800/50 border border-zinc-700 rounded-lg p-3 text-xs text-zinc-400 space-y-1">
          <div><span className="text-zinc-500">Market:</span> <span className="text-zinc-200">{session.market}</span></div>
          <div><span className="text-zinc-500">Initial capital:</span> <span className="text-zinc-200">{session.initial_capital.toLocaleString()} KRW</span></div>
          <div><span className="text-zinc-500">선택 계정 수:</span> <span className="text-zinc-200">{selected.size}</span></div>
        </div>

        <label className="flex items-start gap-2 text-xs text-zinc-300 cursor-pointer">
          <input
            type="checkbox"
            checked={acknowledged}
            onChange={(e) => setAcknowledged(e.target.checked)}
            className="mt-0.5 accent-rose-500 cursor-pointer"
          />
          <span>실제 자금이 움직이며, 손실 가능성을 이해합니다.</span>
        </label>

        {error && <p className="text-xs text-rose-400 break-all">{error}</p>}

        <div className="flex justify-end gap-2 pt-1">
          <Button variant="secondary" onClick={onClose} disabled={submitting}>Cancel</Button>
          <Button
            variant="danger"
            disabled={!acknowledged || submitting || selected.size === 0}
            onClick={handleConfirm}
          >
            {submitting ? "Promoting..." : `Promote to Real (${selected.size})`}
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: 빌드 확인**

Run: `npm run vite:build`
Expected: 성공(LiveTradingPage의 onConfirm 호출부가 Task 7에서 맞춰지기 전이면 타입 에러 가능 → Task 7과 함께 통과 확인).

- [ ] **Step 3: Commit**

```bash
git add src/components/live/PromoteRealDialog.tsx
git commit -m "feat(live): multi-account selection in promote dialog"
```

---

### Task 7: SessionTable 그룹 표시 + LiveTradingPage 배선

**Files:**
- Modify: `src/pages/LiveTradingPage.tsx:427-429` (onConfirm), 그룹 핸들러 추가
- Modify: `src/components/live/SessionTable.tsx` (그룹 헤더 + 그룹 버튼)

- [ ] **Step 1: LiveTradingPage onConfirm 다계정화**

`PromoteRealDialog`의 onConfirm을 교체:

```tsx
          onConfirm={async (accountIds) => {
            await toggleSessionMode(promoteTarget.id, "real", accountIds);
          }}
```

- [ ] **Step 2: LiveTradingPage 그룹 핸들러 추가 + store 연결**

store에서 그룹 액션을 구조분해(파일 상단에서 `toggleSessionMode` 가져오는 곳):

```tsx
    toggleSessionMode, emergencyStopAllReal,
    stopSessionGroup, demoteSessionGroup, deleteSessionGroup,
```

데모트 핸들러(`handleDemote` 등) 인근에 그룹 핸들러 추가:

```tsx
  const handleStopGroup = async (groupId: number) => {
    try { await stopSessionGroup(groupId); }
    catch (e) { window.alert(`Group stop failed: ${e instanceof Error ? e.message : String(e)}`); }
  };
  const handleDemoteGroup = async (groupId: number) => {
    const ok = window.confirm("이 그룹의 모든 세션을 paper로 되돌립니다. 계속할까요?");
    if (!ok) return;
    try { await demoteSessionGroup(groupId); }
    catch (e) { window.alert(`Group demote failed: ${e instanceof Error ? e.message : String(e)}`); }
  };
  const handleDeleteGroup = async (groupId: number) => {
    const ok = window.confirm("이 그룹의 모든 세션을 삭제합니다. 되돌릴 수 없습니다. 계속할까요?");
    if (!ok) return;
    try { await deleteSessionGroup(groupId); }
    catch (e) { window.alert(`Group delete failed: ${e instanceof Error ? e.message : String(e)}`); }
  };
```

`<SessionTable ... />` 호출에 prop 전달:

```tsx
          onStopGroup={handleStopGroup}
          onDemoteGroup={handleDemoteGroup}
          onDeleteGroup={handleDeleteGroup}
```

- [ ] **Step 3: SessionTable — 그룹 prop + 그룹 헤더 행 렌더**

`Props`에 추가:

```tsx
  onStopGroup: (groupId: number) => void;
  onDemoteGroup: (groupId: number) => void;
  onDeleteGroup: (groupId: number) => void;
```

함수 시그니처 구조분해에 `onStopGroup, onDemoteGroup, onDeleteGroup` 추가. 본문에서 행 렌더 직전, 그룹 멤버를 묶기 위해 렌더링되는 `sessions`를 group_id 기준으로 정렬하고 그룹 헤더를 삽입한다. 기존 `{sessions.map((s) => { ... return <tr> ... })}`를 다음 구조로 교체:

```tsx
        {(() => {
          // group_id가 있는 세션은 그룹으로 모으고, 각 그룹의 첫 멤버 위에
          // 그룹 헤더 행(그룹 일괄 버튼)을 끼워 넣는다. 단독 세션은 그대로.
          const renderedGroups = new Set<number>();
          const colSpan = 13; // 현재 컬럼 수
          const rows: React.ReactNode[] = [];
          for (const s of sessions) {
            if (s.group_id != null && !renderedGroups.has(s.group_id)) {
              renderedGroups.add(s.group_id);
              const memberCount = sessions.filter((x) => x.group_id === s.group_id).length;
              rows.push(
                <tr key={`grp-${s.group_id}`} className="bg-zinc-900/60 border-b border-zinc-800">
                  <td colSpan={colSpan} className="py-1.5">
                    <div className="flex items-center gap-2 text-xs">
                      <Badge variant="amber">GROUP</Badge>
                      <span className="text-zinc-300 font-medium">{s.label}</span>
                      <span className="text-zinc-500">· {memberCount} accounts</span>
                      <div className="ml-auto flex gap-1">
                        <Button size="sm" variant="secondary" onClick={() => onStopGroup(s.group_id!)}>Stop group</Button>
                        <Button size="sm" variant="secondary" onClick={() => onDemoteGroup(s.group_id!)}>→ Paper group</Button>
                        <Button size="sm" variant="danger" onClick={() => onDeleteGroup(s.group_id!)}>Del group</Button>
                      </div>
                    </div>
                  </td>
                </tr>,
              );
            }
            rows.push(renderSessionRow(s));
          }
          return rows;
        })()}
```

기존 `tr`(행) JSX를 `renderSessionRow(s: LiveSession)` 헬퍼 함수로 추출한다(컴포넌트 함수 본문 내부에 정의, 기존 `tick`/`derived`/`color` 계산 포함). 즉:

```tsx
  const renderSessionRow = (s: LiveSession) => {
    const tick = ticks[s.market];
    const derived = deriveSessionPnl(s, tick);
    const preset = presetById.get(s.preset_id);
    const baseline = preset?.baseline_return;
    const baselineTrades = preset?.baseline_trades;
    const visible = !hiddenSessionIds.includes(s.id);
    const color = colorFor(s.id, sortedSessionIds);
    return (
      /* ── 기존 <tr key={s.id} ...> ... </tr> 본문 그대로 ── */
    );
  };
```

> `React` 네임스페이스(`React.ReactNode`)를 쓰므로 파일 상단에 `import type React from "react";`가 없으면 추가하거나 `ReactNode`를 `react`에서 named import 한다.

- [ ] **Step 4: 빌드 확인**

Run: `npm run vite:build`
Expected: 타입 에러 없이 성공.

- [ ] **Step 5: 린트**

Run: `npm run lint`
Expected: 신규 에러 없음.

- [ ] **Step 6: Commit**

```bash
git add src/pages/LiveTradingPage.tsx src/components/live/SessionTable.tsx
git commit -m "feat(live): grouped session rows + group action wiring"
```

---

### Task 8: 매뉴얼 + 최종 검증

**Files:**
- Modify: `Manual/auto-trading.md`

- [ ] **Step 1: 매뉴얼 섹션 추가**

`Manual/auto-trading.md`에 "다계정 그룹 승급" 섹션 추가(실제 동작 기준):

```markdown
## 다계정 그룹 승급 (Multi-account group)

세션을 real로 승급할 때 여러 Upbit 계정을 한 번에 선택할 수 있다.

- 승급 다이얼로그에서 계정을 2개 이상 체크 → 계정마다 동일 전략(프리셋·마켓·자본)의
  real 세션이 생성되고, 모두 하나의 그룹(`group_id` = 리드 세션 id)으로 묶인다.
- 이미 다른 real 세션이 실행 중인 계정은 선택 불가(계정당 running real 1개 제약).
- 세션 목록에서 그룹은 헤더 행으로 묶여 표시되며, 그룹 단위로
  Stop / → Paper / Del을 한 번에 적용할 수 있다. 개별 세션 조작도 그대로 가능.
- 계정 1개만 선택하면 그룹 없이 기존 단일 승급과 동일하게 동작한다.
- 각 계정의 포지션·잔고는 독립이며, 첫 사이클에서 실계정 잔고로 reconcile된다.
```

- [ ] **Step 2: 전체 검증**

Run:
```bash
cd src-tauri && cargo build --features tauri-app && cargo test
cd .. && npm run vite:build && npm run lint
```
Expected: 백엔드 빌드/테스트 PASS, 프론트 빌드 PASS, 린트 신규 에러 없음.

- [ ] **Step 3: 수동 검증 안내(사용자에게)**

서버 재시작 후 확인할 시나리오를 사용자에게 안내(Claude는 서버를 직접 시작하지 않음):
1. paper 세션 생성 → Start → → Real 클릭 → 계정 2개 선택 → 승급.
2. 세션 목록에 GROUP 헤더 + 계정별 2행 표시 확인.
3. Stop group / → Paper group / Del group 동작 확인.
4. 계정 1개만 선택 승급 시 그룹 없이 단독 동작 확인.

- [ ] **Step 4: Commit**

```bash
git add Manual/auto-trading.md
git commit -m "docs(live): multi-account group promotion manual"
```

---

## Self-Review (작성자 점검 완료)

- **Spec coverage:** 마이그레이션(Task1)·모델/repo(Task2)·다계정 승급+트랜잭션(Task3)·그룹 커맨드(Task4)·프론트 타입/API/store(Task5)·다이얼로그(Task6)·테이블 그룹+배선(Task7)·매뉴얼/검증(Task8) — 스펙 전 섹션 매핑됨.
- **Placeholder scan:** 모든 코드 단계에 실제 코드 포함. Task2 Step5/Task3 Step4의 테스트 헬퍼명은 기존 테스트 모듈명에 맞추라는 단서 명시(목 불가 시 수동 검증으로 명확히 대체).
- **Type consistency:** Rust `group_id: Option<i64>` ↔ TS `group_id: number | null`. 커맨드 인자 `upbit_account_ids`(snake) ↔ Args 필드 일치. 그룹 커맨드 invoke 키(`groupId`) ↔ Tauri 자동 camel 변환 일치. `toggleSessionMode(id, mode, accountIds)` 시그니처가 lib/store/page 전반 통일.
- **위험:** 라이브 실주문 경로(session_engine/order_executor/pending_tracker) 무변경 — 의도된 핵심 안전장치.
