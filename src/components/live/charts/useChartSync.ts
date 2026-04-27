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
 * `suppress` guards against re-entrant callbacks within the same tick.
 */
export function useChartSync(
  top: IChartApi | null,
  bottom: IChartApi | null,
) {
  useEffect(() => {
    if (!top || !bottom) return;
    let suppress = false;

    const onTop = (range: Range<Time> | null) => {
      if (!range || suppress) return;
      suppress = true;
      try { bottom.timeScale().setVisibleRange(range); }
      finally { suppress = false; }
    };
    const onBottom = (range: Range<Time> | null) => {
      if (!range || suppress) return;
      suppress = true;
      try { top.timeScale().setVisibleRange(range); }
      finally { suppress = false; }
    };

    top.timeScale().subscribeVisibleTimeRangeChange(onTop);
    bottom.timeScale().subscribeVisibleTimeRangeChange(onBottom);

    return () => {
      top.timeScale().unsubscribeVisibleTimeRangeChange(onTop);
      bottom.timeScale().unsubscribeVisibleTimeRangeChange(onBottom);
    };
  }, [top, bottom]);
}
