import { describe, it, expect } from "vitest";
import type { SeriesMarker, Time } from "lightweight-charts";

/**
 * Diagnostic test for the "live chart shows duplicate buy/sell labels when
 * multiple sessions exist" report.
 *
 * This test MIRRORS — line-for-line — the marker construction loop in
 * `CandleChart.tsx:259-313` (the useEffect labelled "── 2b. Trade markers ──").
 * If that loop changes, this mirror should be updated. We mirror rather than
 * import because the production code is inlined inside a React useEffect; a
 * separate extracted helper would be cleaner long-term but is out of scope
 * for this verification.
 */

interface FakeTrade {
  ts: string;
  side: "buy" | "sell";
  pnl_pct: number | null;
  is_real: boolean;
}

interface FakeSession {
  id: number;
  label: string;
}

const PROFIT = "#3b82f6";
const LOSS = "#f43f5e";

/** Mirror of CandleChart.tsx:259-313 — keep in sync. */
function buildMarkers(
  sessions: FakeSession[],
  tradesBySession: Record<number, FakeTrade[]>,
  sessionIds: number[],
  hiddenSessionIds: number[],
  colorFor: (id: number, ids: number[]) => string,
): SeriesMarker<Time>[] {
  const hidden = new Set(hiddenSessionIds);
  const allMarkers: SeriesMarker<Time>[] = [];
  for (const session of sessions) {
    if (hidden.has(session.id)) continue;
    const color = colorFor(session.id, sessionIds);
    const trades = tradesBySession[session.id] ?? [];
    for (const t of trades) {
      const baseLabel = t.is_real ? `${session.label} (R)` : session.label;
      const time = isoToUtcSec(t.ts) as Time;
      const position = t.side === "buy" ? "belowBar" : "aboveBar";
      const shape = t.side === "buy" ? "arrowUp" : "arrowDown";
      const size = t.is_real ? 2 : 1;

      if (t.side === "sell" && t.pnl_pct != null) {
        const pnlColor = t.pnl_pct >= 0 ? PROFIT : LOSS;
        const sign = t.pnl_pct >= 0 ? "+" : "";
        // (1) pnl marker — pushed first → ends up below the name in stack
        allMarkers.push({
          time,
          position,
          shape,
          size,
          color: pnlColor,
          text: `${sign}${(t.pnl_pct * 100).toFixed(2)}%`,
        });
        // (2) name marker — pushed second → ends up above the pnl
        allMarkers.push({
          time,
          position,
          shape: "square",
          size: 0,
          color,
          text: baseLabel,
        });
      } else {
        allMarkers.push({ time, position, shape, size, color, text: baseLabel });
      }
    }
  }
  allMarkers.sort((a, b) => (a.time as number) - (b.time as number));
  return allMarkers;
}

function isoToUtcSec(iso: string): number {
  return Math.floor(new Date(iso).getTime() / 1000);
}

const palette = ["#F28E2B", "#EDC948", "#B07AA1", "#9C755F", "#7570B3"];
function colorFor(id: number, ids: number[]): string {
  const idx = ids.indexOf(id);
  return palette[idx >= 0 ? idx % palette.length : 0];
}

const SELL_TS = "2026-04-21T01:00:00Z";
const SELL_TIME = isoToUtcSec(SELL_TS);

function makeSell(): FakeTrade {
  return { ts: SELL_TS, side: "sell", pnl_pct: 0.0154, is_real: false };
}

describe("CandleChart trade-marker construction (CandleChart.tsx:259-313)", () => {
  it("control — single session with 1 sell yields 2 stacked markers (label + pnl)", () => {
    const sessions: FakeSession[] = [{ id: 1, label: "Short_156" }];
    const trades: Record<number, FakeTrade[]> = { 1: [makeSell()] };

    const markers = buildMarkers(sessions, trades, [1], [], colorFor);

    expect(markers).toHaveLength(2);
    const atSell = markers.filter((m) => m.time === SELL_TIME && m.position === "aboveBar");
    expect(atSell).toHaveLength(2);
    const texts = atSell.map((m) => m.text).sort();
    expect(texts).toEqual(["+1.54%", "Short_156"]);
  });

  it("repro — TWO sessions sharing the same preset/label yield 4 stacked markers at the same (time, position)", () => {
    // This is the exact configuration the user reported: multiple sessions
    // created from one preset. NewSessionDialog defaults the label to
    // preset.name so both sessions end up labelled "Short_156". The engine
    // is deterministic on the same preset, so both produce a sell at the
    // same timestamp with the same pnl_pct.
    const sessions: FakeSession[] = [
      { id: 1, label: "Short_156" },
      { id: 2, label: "Short_156" },
    ];
    const trades: Record<number, FakeTrade[]> = {
      1: [makeSell()],
      2: [makeSell()],
    };

    const markers = buildMarkers(sessions, trades, [1, 2], [], colorFor);

    // 2 sessions × 1 sell × 2 markers (label + pnl) = 4 markers.
    expect(markers).toHaveLength(4);

    // All 4 land on the SAME (time, position) — that's the visual stack.
    const atSell = markers.filter((m) => m.time === SELL_TIME && m.position === "aboveBar");
    expect(atSell).toHaveLength(4);

    // The text content reproduces the screenshot exactly:
    //   "Short_156" appears twice, "+1.54%" appears twice.
    const texts = atSell.map((m) => m.text).sort();
    expect(texts).toEqual(["+1.54%", "+1.54%", "Short_156", "Short_156"]);

    // And — important — the 4 markers ARE in the stacking order needed to
    // reproduce the screenshot bottom-to-top:
    //   pnl(s1), name(s1), pnl(s2), name(s2)
    // because the loop iterates sessions in order and pushes [pnl, name] per
    // sell. Sort-by-time is stable on equal keys (V8/SpiderMonkey both use
    // TimSort), so push order is preserved.
    expect(atSell.map((m) => m.text)).toEqual([
      "+1.54%",
      "Short_156",
      "+1.54%",
      "Short_156",
    ]);
  });

  it("with N sessions, marker count grows linearly — N sessions × 2 markers per sell", () => {
    const N = 4;
    const sessions: FakeSession[] = Array.from({ length: N }, (_, i) => ({
      id: i + 1,
      label: "Short_156",
    }));
    const trades: Record<number, FakeTrade[]> = {};
    const ids: number[] = [];
    for (const s of sessions) {
      trades[s.id] = [makeSell()];
      ids.push(s.id);
    }

    const markers = buildMarkers(sessions, trades, ids, [], colorFor);

    expect(markers).toHaveLength(2 * N);
    const atSell = markers.filter((m) => m.time === SELL_TIME);
    expect(atSell).toHaveLength(2 * N);
    expect(atSell.filter((m) => m.text === "Short_156")).toHaveLength(N);
    expect(atSell.filter((m) => m.text === "+1.54%")).toHaveLength(N);
  });

  it("hiding sessions via hiddenSessionIds removes their marker contribution", () => {
    const sessions: FakeSession[] = [
      { id: 1, label: "Short_156" },
      { id: 2, label: "Short_156" },
    ];
    const trades: Record<number, FakeTrade[]> = {
      1: [makeSell()],
      2: [makeSell()],
    };

    // Hide session 2 — should drop back to the 2-marker (single-session) shape.
    const markers = buildMarkers(sessions, trades, [1, 2], [2], colorFor);
    expect(markers).toHaveLength(2);
    const texts = markers.map((m) => m.text).sort();
    expect(texts).toEqual(["+1.54%", "Short_156"]);
  });

  it("two sessions with DIFFERENT labels still stack — duplication is the count, not the text", () => {
    // Important nuance: the bug isn't conditional on identical labels. Even
    // with distinct labels, 2 sessions still produce 4 markers at the same
    // coord. Identical labels just make the visual collapse look like a
    // copy-paste; distinct labels make it look like 2 different reports
    // stacked. The user's screenshot shows the identical-label case because
    // NewSessionDialog defaults label = preset.name.
    const sessions: FakeSession[] = [
      { id: 1, label: "Short_156" },
      { id: 2, label: "Short_156_v2" },
    ];
    const trades: Record<number, FakeTrade[]> = {
      1: [makeSell()],
      2: [makeSell()],
    };

    const markers = buildMarkers(sessions, trades, [1, 2], [], colorFor);
    expect(markers).toHaveLength(4);
    const atSell = markers.filter((m) => m.time === SELL_TIME);
    const texts = atSell.map((m) => m.text).sort();
    expect(texts).toEqual(["+1.54%", "+1.54%", "Short_156", "Short_156_v2"]);
  });

  it("real-mode session adds (R) suffix — same stacking, different label text", () => {
    const sessions: FakeSession[] = [
      { id: 1, label: "Short_156" },
      { id: 2, label: "Short_156" },
    ];
    const trades: Record<number, FakeTrade[]> = {
      1: [makeSell()],
      2: [{ ...makeSell(), is_real: true }],
    };

    const markers = buildMarkers(sessions, trades, [1, 2], [], colorFor);
    expect(markers).toHaveLength(4);
    const atSell = markers.filter((m) => m.time === SELL_TIME);
    const texts = atSell.map((m) => m.text).sort();
    expect(texts).toEqual(["+1.54%", "+1.54%", "Short_156", "Short_156 (R)"]);
  });
});
