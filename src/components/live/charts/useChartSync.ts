import { useEffect } from "react";
import type { IChartApi, Range, Time } from "lightweight-charts";

/**
 * Two-way bind two charts' visible *time* range so panning/zooming one syncs
 * the other. We sync time (not logical/index) because the two charts may hold
 * very different point counts — a candle chart with hundreds of bars and a
 * signal lane with only a handful of trade points. With logical-index sync,
 * the same index in each chart maps to wildly different times, so a small
 * drag on one chart would jump the other to an unrelated moment.
 *
 * Loop guard: instead of a transient `suppress` flag (which races with async
 * `subscribeVisibleTimeRangeChange` callbacks emitted by lightweight-charts
 * after a `setVisibleRange` call), we cache the last range we propagated and
 * compare incoming events against it. An incoming range that matches the
 * cache is the echo of our own write and is ignored — covering both the
 * synchronous and async event-emission cases.
 */
export function useChartSync(
  top: IChartApi | null,
  bottom: IChartApi | null,
) {
  useEffect(() => {
    if (!top || !bottom) return;

    let lastSync: { from: number; to: number } | null = null;

    const numericFromTime = (t: Time): number => {
      if (typeof t === "number") return t;
      if (typeof t === "string") return Math.floor(new Date(t).getTime() / 1000);
      // BusinessDay { year, month, day } — rare for our intraday charts
      return Math.floor(new Date(`${t.year}-${t.month}-${t.day}`).getTime() / 1000);
    };

    const sameAsLast = (range: Range<Time>): boolean => {
      if (!lastSync) return false;
      return Math.abs(numericFromTime(range.from) - lastSync.from) < 0.5
        && Math.abs(numericFromTime(range.to) - lastSync.to) < 0.5;
    };

    const propagate = (range: Range<Time> | null, target: IChartApi) => {
      if (!range) return;
      if (sameAsLast(range)) return;  // echo of our own write — skip
      lastSync = {
        from: numericFromTime(range.from),
        to: numericFromTime(range.to),
      };
      target.timeScale().setVisibleRange(range);
    };

    const onTop = (range: Range<Time> | null) => propagate(range, bottom);
    const onBottom = (range: Range<Time> | null) => propagate(range, top);

    top.timeScale().subscribeVisibleTimeRangeChange(onTop);
    bottom.timeScale().subscribeVisibleTimeRangeChange(onBottom);

    return () => {
      top.timeScale().unsubscribeVisibleTimeRangeChange(onTop);
      bottom.timeScale().unsubscribeVisibleTimeRangeChange(onBottom);
    };
  }, [top, bottom]);
}
