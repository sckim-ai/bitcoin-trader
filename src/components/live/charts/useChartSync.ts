import { useEffect } from "react";
import type { IChartApi, LogicalRange } from "lightweight-charts";

/**
 * Two-way bind two charts' visible logical range so panning/zooming one syncs
 * the other. `suppress` guards against re-entrant emissions on the same tick.
 * No-op while either chart is null.
 */
export function useChartSync(
  top: IChartApi | null,
  bottom: IChartApi | null,
) {
  useEffect(() => {
    if (!top || !bottom) return;
    let suppress = false;

    const onTop = (range: LogicalRange | null) => {
      if (!range || suppress) return;
      suppress = true;
      try { bottom.timeScale().setVisibleLogicalRange(range); }
      finally { suppress = false; }
    };
    const onBottom = (range: LogicalRange | null) => {
      if (!range || suppress) return;
      suppress = true;
      try { top.timeScale().setVisibleLogicalRange(range); }
      finally { suppress = false; }
    };

    top.timeScale().subscribeVisibleLogicalRangeChange(onTop);
    bottom.timeScale().subscribeVisibleLogicalRangeChange(onBottom);

    return () => {
      top.timeScale().unsubscribeVisibleLogicalRangeChange(onTop);
      bottom.timeScale().unsubscribeVisibleLogicalRangeChange(onBottom);
    };
  }, [top, bottom]);
}
