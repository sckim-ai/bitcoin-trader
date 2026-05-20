//! 일회성 진단/보정 도구. live_trades.pnl_pct가 옛 코드(× 100.0)로 저장된
//! row를 식별하고, pnl/price/volume에서 분수형 pnl_pct를 정확히 역산해 보정한다.
//!
//! 단위 정의:
//!   - 분수형(fraction): 0.0194 = 1.94% — 차트가 ×100해서 표시
//!   - 퍼센트형(legacy): 1.94 = 1.94% — 옛 코드의 잘못된 단위
//!
//! 역산식 (단위 무관, pnl/price/volume에서 정확히 추론):
//!   buy = sell - pnl/volume
//!   pnl_pct(분수형) = (sell - buy) / buy = (pnl/volume) / (sell - pnl/volume)
//!
//! 사용법:
//!   cargo run --example pnl_pct_tool --no-default-features -- diagnose
//!   cargo run --example pnl_pct_tool --no-default-features -- fix --apply
//!
//! `fix`는 dry-run 기본. `--apply` 플래그로만 실제 UPDATE.

use rusqlite::Connection;
use std::env;
use std::path::PathBuf;

fn db_path() -> PathBuf {
    let local = env::var("LOCALAPPDATA").expect("LOCALAPPDATA env var");
    PathBuf::from(local).join("bitcoin-trader").join("bitcoin_trader.db")
}

#[derive(Debug)]
struct SellRow {
    id: i64,
    session_id: i64,
    ts: String,
    is_real: bool,
    signal: String,
    sell_price: f64,
    volume: f64,
    pnl: f64,
    stored_pnl_pct: f64,
}

fn load_sells(conn: &Connection) -> rusqlite::Result<Vec<SellRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, ts, is_real, signal, price, volume, pnl, pnl_pct
           FROM live_trades
          WHERE side = 'sell'
            AND pnl IS NOT NULL
            AND pnl_pct IS NOT NULL
            AND volume > 0
            AND price > 0
          ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SellRow {
            id: r.get(0)?,
            session_id: r.get(1)?,
            ts: r.get(2)?,
            is_real: r.get::<_, i64>(3)? != 0,
            signal: r.get(4)?,
            sell_price: r.get(5)?,
            volume: r.get(6)?,
            pnl: r.get(7)?,
            stored_pnl_pct: r.get(8)?,
        })
    })?;
    rows.collect()
}

/// 분수형 pnl_pct를 (pnl, sell_price, volume)에서 역산. divide-by-zero 가드.
fn recompute_fraction(pnl: f64, sell_price: f64, volume: f64) -> Option<f64> {
    if volume <= 0.0 {
        return None;
    }
    let buy = sell_price - pnl / volume;
    if buy.abs() < 1e-9 {
        return None;
    }
    Some((pnl / volume) / buy)
}

/// 저장된 stored vs 역산된 fraction을 비교해 어떤 단위인지 분류.
fn classify(stored: f64, fraction: f64) -> &'static str {
    let abs_frac = fraction.abs();
    if abs_frac < 1e-9 {
        return if stored.abs() < 1e-9 { "ok-zero" } else { "drift" };
    }
    let ratio_to_fraction = (stored / fraction).abs();
    let ratio_to_percent = (stored / (fraction * 100.0)).abs();
    if (ratio_to_fraction - 1.0).abs() < 0.01 {
        "ok-fraction" // 이미 분수형으로 저장 (올바름)
    } else if (ratio_to_percent - 1.0).abs() < 0.01 {
        "legacy-percent" // 옛 단위 (× 100.0 저장 — 차트에서 ×10000 부풀려짐)
    } else {
        "drift" // 어느 쪽도 아님 — buy_price 추론과 다른 케이스
    }
}

fn diagnose(conn: &Connection) -> rusqlite::Result<()> {
    let rows = load_sells(conn)?;
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    let mut counts_real = std::collections::HashMap::<&str, usize>::new();
    let mut sample: Vec<(SellRow, f64, &'static str)> = Vec::new();

    for r in &rows {
        let frac = recompute_fraction(r.pnl, r.sell_price, r.volume).unwrap_or(0.0);
        let cls = classify(r.stored_pnl_pct, frac);
        *counts.entry(cls).or_insert(0) += 1;
        if r.is_real {
            *counts_real.entry(cls).or_insert(0) += 1;
        }
        if sample.iter().filter(|(_, _, c)| *c == cls).count() < 3 {
            sample.push((
                SellRow {
                    id: r.id,
                    session_id: r.session_id,
                    ts: r.ts.clone(),
                    is_real: r.is_real,
                    signal: r.signal.clone(),
                    sell_price: r.sell_price,
                    volume: r.volume,
                    pnl: r.pnl,
                    stored_pnl_pct: r.stored_pnl_pct,
                },
                frac,
                cls,
            ));
        }
    }

    println!("=== Total sell rows analyzed: {} ===", rows.len());
    println!("\n[Classification — all sell rows]");
    for (k, v) in &counts {
        println!("  {:>16}  {} rows", k, v);
    }
    println!("\n[Classification — is_real=1 only (R 마커 source)]");
    for (k, v) in &counts_real {
        println!("  {:>16}  {} rows", k, v);
    }

    println!("\n[Samples (up to 3 per class)]");
    for (r, frac, cls) in &sample {
        let real_tag = if r.is_real { "R" } else { "p" };
        println!(
            "  [{}] id={} sid={} {} {} signal={} sell={:.0} vol={:.6} pnl={:.0} stored_pct={:.6} recomputed_fraction={:.6} → ×100={:.4}%",
            cls, r.id, r.session_id, real_tag, r.ts, r.signal,
            r.sell_price, r.volume, r.pnl, r.stored_pnl_pct, frac, frac * 100.0,
        );
    }

    println!("\n[Interpretation]");
    println!("  ok-fraction    : 이미 분수형(올바름). 차트가 ×100해서 정상 표시.");
    println!("  legacy-percent : 옛 코드(×100.0 저장). 차트 ×100 → 100배 부풀림. 보정 필요.");
    println!("  drift          : pnl/price/volume이 일관되지 않음 (수동 조작 또는 reconcile drift).");
    println!("  ok-zero        : pnl_pct=0 — buy=sell이거나 dust 매도.");

    Ok(())
}

fn fix(conn: &mut Connection, apply: bool) -> rusqlite::Result<()> {
    let rows = load_sells(conn)?;
    let mut to_fix: Vec<(i64, f64, f64, &'static str)> = Vec::new();

    for r in &rows {
        let frac = match recompute_fraction(r.pnl, r.sell_price, r.volume) {
            Some(f) => f,
            None => continue,
        };
        let cls = classify(r.stored_pnl_pct, frac);
        if cls == "legacy-percent" || cls == "drift" {
            to_fix.push((r.id, r.stored_pnl_pct, frac, cls));
        }
    }

    println!("Rows needing correction: {}", to_fix.len());
    println!("(legacy-percent: 옛 단위, drift: pnl/price/volume 불일치)");
    if to_fix.is_empty() {
        println!("Nothing to do.");
        return Ok(());
    }

    println!("\n[Preview first 10]");
    for (id, stored, new, cls) in to_fix.iter().take(10) {
        println!("  id={} [{}] stored={:.6} → {:.6} (display: {:.4}% → {:.4}%)",
            id, cls, stored, new, stored * 100.0, new * 100.0);
    }

    if !apply {
        println!("\nDRY RUN — pass `--apply` to actually update.");
        return Ok(());
    }

    let tx = conn.transaction()?;
    for (id, _, new, _) in &to_fix {
        tx.execute(
            "UPDATE live_trades SET pnl_pct = ?1 WHERE id = ?2",
            rusqlite::params![new, id],
        )?;
    }
    tx.commit()?;
    println!("\n✓ Updated {} rows.", to_fix.len());
    Ok(())
}

/// 큰 수익률 sell row dump — buy_price가 정말 그만큼 낮은 게 맞는지 raw로 확인.
/// `min_pct`: 절대값 임계 (분수형 — 0.3 = 30%).
fn dump_high(conn: &Connection, min_pct: f64) -> rusqlite::Result<()> {
    let rows = load_sells(conn)?;
    println!("=== sells with |pnl_pct| >= {} ===", min_pct);
    println!("count={}", rows.iter().filter(|r| r.stored_pnl_pct.abs() >= min_pct).count());
    println!();

    // Session label 함께 보여주기 위해 join.
    for r in &rows {
        if r.stored_pnl_pct.abs() < min_pct {
            continue;
        }
        let label: String = conn.query_row(
            "SELECT label FROM live_sessions WHERE id = ?1",
            [r.session_id],
            |row| row.get(0),
        ).unwrap_or_else(|_| "(deleted)".into());
        let frac = recompute_fraction(r.pnl, r.sell_price, r.volume).unwrap_or(0.0);
        let buy_implied = r.sell_price - r.pnl / r.volume;
        let real_tag = if r.is_real { "R" } else { "p" };
        println!(
            "id={} sid={}({:.30}) {} {} signal={}",
            r.id, r.session_id, label, real_tag, r.ts, r.signal,
        );
        println!(
            "    sell={:.0} vol={:.6} pnl={:.0} → buy_implied={:.0}",
            r.sell_price, r.volume, r.pnl, buy_implied,
        );
        println!(
            "    stored_pnl_pct={:.6} (chart shows {:.4}%) | recomputed={:.6} ({:.4}%)",
            r.stored_pnl_pct, r.stored_pnl_pct * 100.0, frac, frac * 100.0,
        );
        println!();
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: pnl_pct_tool <diagnose|fix [--apply]|dump [min_pct]|delete-phantom-sells [--apply]|restore-aggregated-sells [--apply]|real>");
        std::process::exit(2);
    }
    let cmd = args[1].as_str();
    let apply = args.iter().any(|a| a == "--apply");

    let path = db_path();
    println!("DB: {}", path.display());
    if !path.exists() {
        eprintln!("DB file not found.");
        std::process::exit(1);
    }

    match cmd {
        "diagnose" => {
            let conn = Connection::open(&path).expect("open db");
            diagnose(&conn).expect("diagnose");
        }
        "fix" => {
            let mut conn = Connection::open(&path).expect("open db");
            fix(&mut conn, apply).expect("fix");
        }
        "dump" => {
            let min_pct: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.3);
            let conn = Connection::open(&path).expect("open db");
            dump_high(&conn, min_pct).expect("dump");
        }
        "delete-phantom-sells" => {
            // 같은 (session_id, ts, side) 그룹에 row가 2개 이상이고 그 중 pnl=0
            // (혹은 NULL)인 row는 분할이 아니라 phantom 중복으로 추정해 삭제.
            // 적어도 1건의 pnl≠0 row가 그룹에 남아 있어야 안전.
            let mut conn = Connection::open(&path).expect("open db");
            let candidates: Vec<(i64, i64, String, f64)> = {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, ts, COALESCE(pnl, 0) FROM live_trades
                      WHERE side = 'sell'
                        AND (session_id, ts, side) IN (
                          SELECT session_id, ts, side FROM live_trades
                           WHERE side = 'sell'
                           GROUP BY session_id, ts, side
                          HAVING COUNT(*) > 1
                            AND SUM(CASE WHEN COALESCE(pnl, 0) <> 0 THEN 1 ELSE 0 END) >= 1
                        )"
                ).expect("prep");
                let rows = stmt.query_map([], |r| Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                ))).expect("q").collect::<rusqlite::Result<Vec<_>>>().expect("rows");
                rows
            };
            // Group by (sid, ts), keep one pnl≠0 row, delete pnl=0 rows.
            use std::collections::HashMap;
            let mut groups: HashMap<(i64, String), Vec<(i64, f64)>> = HashMap::new();
            for (id, sid, ts, pnl) in candidates {
                groups.entry((sid, ts)).or_default().push((id, pnl));
            }
            let mut to_delete: Vec<i64> = Vec::new();
            for (_, group) in &groups {
                let kept = group.iter().find(|(_, pnl)| pnl.abs() > 1e-9);
                if kept.is_none() { continue; }
                let kept_id = kept.unwrap().0;
                for (id, pnl) in group {
                    if *id != kept_id && pnl.abs() < 1e-9 {
                        to_delete.push(*id);
                    }
                }
            }
            println!("Phantom rows to delete: {}", to_delete.len());
            for id in &to_delete {
                println!("  delete id={}", id);
            }
            if !apply {
                println!("DRY RUN — pass `--apply` to actually delete.");
            } else if !to_delete.is_empty() {
                let tx = conn.transaction().expect("tx");
                for id in &to_delete {
                    tx.execute("DELETE FROM live_trades WHERE id = ?1", [id]).expect("del");
                }
                tx.commit().expect("commit");
                println!("✓ Deleted {} phantom rows.", to_delete.len());
            }
        }
        "restore-aggregated-sells" => {
            // 분할 매도(보통 3 청크)의 phantom 0-pnl 행이 delete-phantom-sells로
            // 삭제된 후, 남은 1 행이 1 청크의 volume·pnl·fee 만 가지고 있는 상태를
            // 보정. 같은 세션에서 직전 sell 이후 매수된 누적 volume(= 매도 시점의
            // 오픈 포지션 크기)을 추정해, 매도 행의 volume이 그보다 현저히 작으면
            // (잔존 청크) 다음과 같이 보정:
            //   new_volume = open_position
            //   new_fee    = sell.price * new_volume * 0.0005
            //   new_pnl    = (sell.price - cost_basis_avg) * new_volume
            //   new_pnl_pct = (sell.price - cost_basis_avg) / cost_basis_avg
            // cost_basis_avg = 직전 sell 이후 매수 행들의 가중평균.
            let mut conn = Connection::open(&path).expect("open db");
            // 세션별로 buy/sell 시계열 일괄 로드.
            #[derive(Debug, Clone)]
            struct Trade {
                id: i64, side: String, price: f64, volume: f64, fee: f64,
            }
            let trades: Vec<(i64, Trade)> = {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, side, price, volume, fee
                       FROM live_trades
                      WHERE is_real = 1
                      ORDER BY session_id ASC, ts ASC, id ASC"
                ).expect("prep");
                let rows = stmt.query_map([], |r| Ok((
                    r.get::<_, i64>(1)?,
                    Trade {
                        id: r.get(0)?, side: r.get(2)?,
                        price: r.get(3)?, volume: r.get(4)?, fee: r.get(5)?,
                    },
                ))).expect("q").collect::<rusqlite::Result<Vec<_>>>().expect("rows");
                rows
            };
            // 세션별 그룹.
            use std::collections::BTreeMap;
            let mut by_session: BTreeMap<i64, Vec<Trade>> = BTreeMap::new();
            for (sid, t) in trades { by_session.entry(sid).or_default().push(t); }

            let fee_rate = 0.0005;
            let mut updates: Vec<(i64, f64, f64, f64, f64, f64, f64, f64)> = Vec::new();
            // (id, old_volume, new_volume, old_fee, new_fee, new_pnl, new_pnl_pct, cost_basis)

            for (sid, ts_list) in &by_session {
                // sells 직전까지의 buys 누적을 cost basis로 사용.
                let mut open_volume = 0.0_f64;
                let mut open_cost = 0.0_f64; // sum(price * volume) 가중합
                for t in ts_list {
                    if t.side == "buy" {
                        open_volume += t.volume;
                        open_cost += t.price * t.volume;
                    } else if t.side == "sell" {
                        if open_volume <= 0.0 { continue; }
                        let cost_basis = open_cost / open_volume;
                        // sell.volume이 open_volume의 50% 미만이면 분할 잔존 행으로 판정.
                        // (정상 매도는 ≈100%, 분할 후 cleanup 잔존은 ≈33%)
                        let ratio = t.volume / open_volume;
                        if ratio < 0.6 {
                            let new_volume = open_volume;
                            let new_fee = t.price * new_volume * fee_rate;
                            let new_pnl = (t.price - cost_basis) * new_volume;
                            let new_pnl_pct = (t.price - cost_basis) / cost_basis;
                            updates.push((
                                t.id, t.volume, new_volume, t.fee, new_fee,
                                new_pnl, new_pnl_pct, cost_basis,
                            ));
                            // 매도 후 포지션 비움 (전체를 처분한 것으로 가정).
                            open_volume = 0.0;
                            open_cost = 0.0;
                        } else {
                            // 정상 매도: 전체 포지션 처분으로 간주.
                            open_volume = 0.0;
                            open_cost = 0.0;
                        }
                    }
                    let _ = sid; // suppress unused warning when no updates
                }
            }

            println!("=== Sells to aggregate-restore: {} ===", updates.len());
            for (id, ov, nv, of, nf, np, npp, cb) in &updates {
                println!(
                    "  id={} volume {:.8} → {:.8}  fee {:.0} → {:.0}  pnl_new={:.0}  pnl_pct_new={:.6} ({:.4}%)  cost_basis={:.0}",
                    id, ov, nv, of, nf, np, npp, npp * 100.0, cb,
                );
            }

            if !apply {
                println!("\nDRY RUN — pass `--apply` to actually update.");
            } else if !updates.is_empty() {
                let tx = conn.transaction().expect("tx");
                for (id, _, nv, _, nf, np, npp, _) in &updates {
                    tx.execute(
                        "UPDATE live_trades
                            SET volume = ?1, fee = ?2, pnl = ?3, pnl_pct = ?4
                          WHERE id = ?5",
                        rusqlite::params![nv, nf, np, npp, id],
                    ).expect("update");
                }
                tx.commit().expect("commit");
                println!("\n✓ Updated {} sell rows.", updates.len());
            } else {
                println!("\nNothing to update.");
            }
        }
        "real" => {
            // is_real=1 sell 모두 dump (보정 결과 검증 + 사용자 차트 라벨 추적용).
            let conn = Connection::open(&path).expect("open db");
            let rows = load_sells(&conn).expect("load");
            println!("=== ALL is_real=1 sell rows ===");
            let mut count = 0;
            for r in &rows {
                if !r.is_real { continue; }
                count += 1;
                let label: String = conn.query_row(
                    "SELECT label FROM live_sessions WHERE id = ?1",
                    [r.session_id], |row| row.get(0),
                ).unwrap_or_else(|_| "(deleted)".into());
                let frac = recompute_fraction(r.pnl, r.sell_price, r.volume).unwrap_or(0.0);
                let buy_implied = r.sell_price - r.pnl / r.volume;
                println!(
                    "id={} sid={} ({}) {} signal={}",
                    r.id, r.session_id, label, r.ts, r.signal,
                );
                println!(
                    "    sell={:.0} vol={:.6} pnl={:.0} → buy_implied={:.0}",
                    r.sell_price, r.volume, r.pnl, buy_implied,
                );
                println!(
                    "    stored_pnl_pct={:.6} (chart {:.4}%) | recomputed={:.6} ({:.4}%)",
                    r.stored_pnl_pct, r.stored_pnl_pct * 100.0, frac, frac * 100.0,
                );
                println!();
            }
            println!("Total R sell rows: {}", count);
        }
        other => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    }
}
