import { useEffect } from "react";
import type { IChartApi, LogicalRange } from "lightweight-charts";

/**
 * Pure state machine for two-way logical-range sync between charts.
 * Extracted so it's testable without React/lightweight-charts.
 *
 * Why logical (bar index) and not time:
 *   `setVisibleRange` rounds to bar boundaries, so the echo callback fires
 *   with a slightly different range than what we set, breaking equality
 *   checks. Logical ranges are the bar indices themselves — exact in,
 *   exact out, no rounding. This relies on both charts having the *same
 *   bars* (same time series); we now ensure that by feeding both the
 *   identical, parent-filtered marketData plus a synced "current hour
 *   bucket" placeholder.
 */
export interface SyncState {
  lastSync: { from: number; to: number } | null;
}

const TOLERANCE = 0.001; // bar-index tolerance, generous for float wobble

/** Decide whether an incoming range should be propagated to the partner. */
export function shouldPropagate(state: SyncState, incoming: { from: number; to: number }): boolean {
  if (!state.lastSync) return true;
  return Math.abs(incoming.from - state.lastSync.from) > TOLERANCE
      || Math.abs(incoming.to - state.lastSync.to) > TOLERANCE;
}

export function recordSync(state: SyncState, range: { from: number; to: number }): void {
  state.lastSync = { from: range.from, to: range.to };
}

export function useChartSync(
  top: IChartApi | null,
  bottom: IChartApi | null,
) {
  useEffect(() => {
    if (!top || !bottom) return;
    const state: SyncState = { lastSync: null };

    const propagate = (range: LogicalRange | null, target: IChartApi) => {
      if (!range) return;
      if (!shouldPropagate(state, range)) return;
      recordSync(state, range);
      target.timeScale().setVisibleLogicalRange(range);
    };

    const onTop = (range: LogicalRange | null) => propagate(range, bottom);
    const onBottom = (range: LogicalRange | null) => propagate(range, top);

    top.timeScale().subscribeVisibleLogicalRangeChange(onTop);
    bottom.timeScale().subscribeVisibleLogicalRangeChange(onBottom);

    return () => {
      top.timeScale().unsubscribeVisibleLogicalRangeChange(onTop);
      bottom.timeScale().unsubscribeVisibleLogicalRangeChange(onBottom);
    };
  }, [top, bottom]);
}
