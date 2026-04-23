//! Phase 1 integration: verify the DB-layer orchestration (presets, sessions,
//! trades, equity) composes correctly. Upbit API / strategy calls are covered
//! in unit tests; this file focuses on persistence boundary.

use bitcoin_trader_lib::db::live_repo;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    conn.execute_batch(include_str!("../migrations/001_initial.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/002_users.sql")).unwrap();
    conn.execute_batch(include_str!("../migrations/006_live_trading.sql")).unwrap();
    conn
}

#[test]
fn full_session_lifecycle() {
    let conn = setup_db();

    // 1) preset 생성
    let pid = live_repo::insert_preset(&conn, 1, "V3-OptA", "V3", "{}", "manual", None).unwrap();

    // 2) 세션 3개 생성
    let s1 = live_repo::insert_session(&conn, 1, "S1", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();
    let s2 = live_repo::insert_session(&conn, 1, "S2", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();
    let s3 = live_repo::insert_session(&conn, 1, "S3", pid, "KRW-ETH", "paper", 1_000_000.0, "2026-04-24T00:00:00Z").unwrap();

    // 3) 각각 running으로 전환
    for sid in [s1, s2, s3] {
        live_repo::set_session_status(&conn, sid, "running").unwrap();
    }
    assert_eq!(live_repo::list_running_sessions(&conn).unwrap().len(), 3);

    // 4) 사이클 시뮬레이션 — 세션마다 다른 trade 삽입
    live_repo::insert_trade(&conn, s1, "2026-04-24T01:00:00Z", "buy",  3e6,   0.3, 450.0, "buy",  None, None, false).unwrap();
    live_repo::insert_trade(&conn, s1, "2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0), Some(3.0), false).unwrap();

    live_repo::insert_trade(&conn, s2, "2026-04-24T03:00:00Z", "buy",  3.2e6, 0.3, 480.0, "buy",  None, None, false).unwrap();
    live_repo::insert_trade(&conn, s2, "2026-04-24T04:00:00Z", "sell", 3.25e6,0.3, 487.5, "sell", Some(1.5), Some(1.5), false).unwrap();

    // 5) 각 세션의 equity 스냅샷
    live_repo::upsert_equity(&conn, s1, "2026-04-24T02:00:00Z", 1_030_000.0, "idle").unwrap();
    live_repo::upsert_equity(&conn, s2, "2026-04-24T04:00:00Z", 1_015_000.0, "idle").unwrap();
    live_repo::upsert_equity(&conn, s3, "2026-04-24T01:00:00Z", 1_000_000.0, "idle").unwrap();

    // 6) 세션별 완료 수 검증
    assert_eq!(live_repo::count_completed_trades(&conn, s1).unwrap(), 1);
    assert_eq!(live_repo::count_completed_trades(&conn, s2).unwrap(), 1);
    assert_eq!(live_repo::count_completed_trades(&conn, s3).unwrap(), 0);

    // 7) 세션 1개 정지 → running 2개
    live_repo::set_session_status(&conn, s3, "stopped").unwrap();
    assert_eq!(live_repo::list_running_sessions(&conn).unwrap().len(), 2);

    // 8) 세션 1 삭제 → trades / equity CASCADE
    live_repo::delete_session(&conn, s1).unwrap();
    assert_eq!(live_repo::list_trades(&conn, s1).unwrap().len(), 0);
    assert_eq!(live_repo::list_equity(&conn, s1).unwrap().len(), 0);
    // s2는 유지
    assert_eq!(live_repo::list_trades(&conn, s2).unwrap().len(), 2);
}

#[test]
fn diff_insert_idempotency() {
    // 동일한 trade 시퀀스를 재삽입하려 할 때, count_completed_trades를 활용해
    // "이미 처리된 것까지는 건너뛰기"를 호출자가 보장해야 한다는 계약을 확인.
    let conn = setup_db();
    let pid = live_repo::insert_preset(&conn, 1, "p", "V3", "{}", "manual", None).unwrap();
    let sid = live_repo::insert_session(&conn, 1, "S", pid, "KRW-ETH", "paper", 1e6, "2026-04-24T00:00:00Z").unwrap();

    // 1차 사이클: trade 1쌍 완료
    let existing_before = live_repo::count_completed_trades(&conn, sid).unwrap();
    assert_eq!(existing_before, 0);

    live_repo::insert_trade(&conn, sid, "2026-04-24T01:00:00Z", "buy",  3e6,   0.3, 450.0, "buy",  None, None, false).unwrap();
    live_repo::insert_trade(&conn, sid, "2026-04-24T02:00:00Z", "sell", 3.1e6, 0.3, 465.0, "sell", Some(3.0), Some(3.0), false).unwrap();

    assert_eq!(live_repo::count_completed_trades(&conn, sid).unwrap(), 1);

    // 2차 사이클: 시뮬레이션이 같은 trade 1개를 재생성한다고 가정
    let simulated_completed: usize = 1;
    let existing = live_repo::count_completed_trades(&conn, sid).unwrap();
    let new_to_insert = simulated_completed.saturating_sub(existing);
    assert_eq!(new_to_insert, 0, "재실행 시 신규 없음");
}
