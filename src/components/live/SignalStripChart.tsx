import { useEffect, useRef } from "react";
import {
  createChart, ColorType,
  type IChartApi, type ISeriesApi, type UTCTimestamp,
} from "lightweight-charts";
import type { MarketData, SignalEvent } from "../../types";
import { isoToUtcSec } from "./charts/runningCandle";

/**
 * Discrete colour-per-candle strip mirroring the strategy's signal log.
 * Backend returns `signal_log: SignalEvent[]` from run_simulation, with one
 * entry whenever the signal type *changes*. We fill between transitions to
 * paint every candle.
 */
const SIGNAL_COLORS: Record<string, string> = {
  ready: "rgba(148, 163, 184, 0.5)",      // slate
  "buy ready": "rgba(134, 239, 172, 0.85)", // light green
  buy: "rgba(16, 185, 129, 1.0)",          // emerald
  hold: "rgba(251, 191, 36, 0.85)",        // amber
  "sell ready": "rgba(251, 146, 60, 0.85)", // orange
  sell: "rgba(239, 68, 68, 1.0)",          // red
};

interface Props {
  /** Visible candles (already filtered by parent's rangeStart). */
  marketData: MarketData[];
  /** Signal log for the selected session; aligned by timestamp, not index. */
  signals: SignalEvent[];
  onChartReady?: (chart: IChartApi | null) => void;
}

export default function SignalStripChart({ marketData, signals, onChartReady }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "#0c0c0f" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "transparent" },
        horzLines: { color: "transparent" },
      },
      timeScale: { borderColor: "#1e1e26", timeVisible: true, secondsVisible: false },
      rightPriceScale: { visible: false },
      leftPriceScale: { visible: false },
      autoSize: true,
    });
    chartRef.current = chart;
    seriesRef.current = chart.addHistogramSeries({
      priceLineVisible: false,
      lastValueVisible: false,
      priceScaleId: "strip",
    });
    chart.priceScale("strip").applyOptions({ visible: false });
    onChartReady?.(chart);
    return () => {
      onChartReady?.(null);
      chart.remove();
      chartRef.current = null;
      seriesRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const series = seriesRef.current;
    if (!series) return;
    if (marketData.length === 0) {
      series.setData([]);
      return;
    }

    // Index transitions by their UTC-seconds timestamp.
    const byTime = new Map<number, string>();
    for (const e of signals) byTime.set(isoToUtcSec(e.timestamp), e.signal_type);

    let current = "ready";
    const data = marketData.map((m) => {
      const t = isoToUtcSec(m.candle.timestamp);
      const next = byTime.get(t);
      if (next) current = next;
      return {
        time: t as UTCTimestamp,
        value: 1,
        color: SIGNAL_COLORS[current] ?? SIGNAL_COLORS.ready,
      };
    });
    series.setData(data);
  }, [marketData, signals]);

  return (
    <div className="space-y-1">
      <div className="flex items-center gap-3 text-[10px] text-zinc-400 px-1 flex-wrap">
        <Legend label="Buy" color={SIGNAL_COLORS.buy} />
        <Legend label="Buy Ready" color={SIGNAL_COLORS["buy ready"]} />
        <Legend label="Hold" color={SIGNAL_COLORS.hold} />
        <Legend label="Sell Ready" color={SIGNAL_COLORS["sell ready"]} />
        <Legend label="Sell" color={SIGNAL_COLORS.sell} />
        <Legend label="Ready" color={SIGNAL_COLORS.ready} />
      </div>
      <div ref={containerRef} className="w-full h-[80px] bg-[#0c0c0f] border border-[#1e1e26] rounded-xl" />
    </div>
  );
}

function Legend({ label, color }: { label: string; color: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className="inline-block w-3 h-3 rounded-sm" style={{ background: color }} />
      <span>{label}</span>
    </span>
  );
}
