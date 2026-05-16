import type { LiveTrade } from "../../../types";

/**
 * 분할 fill 합치기 — 같은 매수/매도 결정에서 발생한 여러 row를 1개 마커용으로 합친다.
 *
 * 문제 배경: REAL 매수/매도는 `execute_split_buy/sell`로 N chunk로 발사되어
 * (a) 일부는 같은 cycle에 done → `real_buy/real_sell` row,
 * (b) 일부는 wait → 다음 cycle에 `pending_order_tracker`가 `real_*_late` row 추가.
 * 두 row 모두 같은 transition 봉(같은 ts)에 정렬되도록 backend에서 맞춰져 있으므로
 * 차트 상에서는 한 결정인데 두 마커가 같은 봉에 stack된다 — 시각적 중복.
 *
 * 합치는 방식 (mathematically consistent):
 *   - price : volume 가중평균 = Σ(p·v)/Σv
 *   - volume: 합계
 *   - fee   : 합계
 *   - pnl   : 합계
 *   - pnl_pct: volume 가중평균. 같은 buy_price 기준 fill이면 (avg_sell - buy)/buy 와 일치.
 *
 * 키: `(ts, side, is_real)` — paper와 real, buy와 sell, 다른 봉은 분리됨.
 * 다른 session_id는 호출자가 이미 분리해서 넘기므로(per-session loop) 키에 포함 X.
 */
export function mergeSplitFills(trades: LiveTrade[]): LiveTrade[] {
  if (trades.length < 2) return trades.slice();
  const groups = new Map<string, LiveTrade[]>();
  for (const t of trades) {
    const key = `${t.ts}|${t.side}|${t.is_real ? 1 : 0}`;
    const g = groups.get(key);
    if (g) g.push(t); else groups.set(key, [t]);
  }
  const merged: LiveTrade[] = [];
  for (const g of groups.values()) {
    if (g.length === 1) {
      merged.push(g[0]);
      continue;
    }
    const totalVol = g.reduce((s, t) => s + t.volume, 0);
    const avgPrice = totalVol > 0
      ? g.reduce((s, t) => s + t.price * t.volume, 0) / totalVol
      : g[0].price;
    const totalFee = g.reduce((s, t) => s + t.fee, 0);
    const anyPnl = g.some(t => t.pnl != null);
    const totalPnl = anyPnl
      ? g.reduce((s, t) => s + (t.pnl ?? 0), 0)
      : null;
    const anyPnlPct = g.some(t => t.pnl_pct != null);
    const avgPnlPct = anyPnlPct && totalVol > 0
      ? g.reduce((s, t) => s + (t.pnl_pct ?? 0) * t.volume, 0) / totalVol
      : null;
    merged.push({
      ...g[0],
      price: avgPrice,
      volume: totalVol,
      fee: totalFee,
      pnl: totalPnl,
      pnl_pct: avgPnlPct,
    });
  }
  return merged;
}
