import { useEffect, useRef, useState } from "react";
import {
  createChart, ColorType,
  type IChartApi, type ISeriesApi, type SeriesMarker, type Time, type UTCTimestamp,
} from "lightweight-charts";
import type { LiveSession, LiveTrade, MarketData, TickData } from "../../types";
import {
  backendToChart, isoToUtcSec, applyTick, initRunning,
  type ChartCandle, type RunningCandleState,
} from "./charts/runningCandle";
import { colorFor } from "./charts/sessionPalette";

interface Props {
  marketData: MarketData[];
  sessions: LiveSession[];
  tradesBySession: Record<number, LiveTrade[]>;
  tick: TickData | undefined;
  /** Called once when the underlying chart is created; null on unmount. */
  onChartReady?: (chart: IChartApi | null) => void;
}

interface OverlayState {
  sma: boolean;
  bb: boolean;
  volume: boolean;
}

/**
 * Two changes are mutating the chart over time:
 *   1. Static: candles + indicators + markers are set once per render of
 *      `marketData` / `sessions` / `tradesBySession`.
 *   2. Live: every tick mutates the running candle via series.update().
 *
 * We deliberately do NOT pass `tick` through the static effect's deps —
 * otherwise every tick would tear down and rebuild every series, killing
 * performance and causing visual jitter.
 */
export default function CandleChart({
  marketData, sessions, tradesBySession, tick, onChartReady,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const smaSeriesRef = useRef<ISeriesApi<"Line">[]>([]);
  const bbSeriesRef = useRef<ISeriesApi<"Line">[]>([]);
  const sessionSeriesRef = useRef<Record<number, ISeriesApi<"Line">>>({});
  const runningRef = useRef<RunningCandleState>({ current: null });

  const [overlay, setOverlay] = useState<OverlayState>({ sma: true, bb: false, volume: true });

  // User picks the start date; the right edge always tracks "now" so the
  // running candle stays in view. Default = 7 days ago.
  const [rangeStart, setRangeStart] = useState<string>(() => {
    const d = new Date();
    d.setDate(d.getDate() - 7);
    return d.toISOString().slice(0, 10);
  });

  // ── 1. Initialise chart instance once ────────────────────────────────
  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "#0c0c0f" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "#1e1e26" },
        horzLines: { color: "#1e1e26" },
      },
      timeScale: { borderColor: "#1e1e26", timeVisible: true, secondsVisible: false },
      rightPriceScale: { borderColor: "#1e1e26" },
      autoSize: true,
    });
    chartRef.current = chart;

    candleSeriesRef.current = chart.addCandlestickSeries({
      upColor: "#10b981", downColor: "#f43f5e",
      borderUpColor: "#10b981", borderDownColor: "#f43f5e",
      wickUpColor: "#10b981", wickDownColor: "#f43f5e",
    });

    volumeSeriesRef.current = chart.addHistogramSeries({
      color: "#3f3f46",
      priceFormat: { type: "volume" },
      priceScaleId: "volume",
    });
    chart.priceScale("volume").applyOptions({
      scaleMargins: { top: 0.85, bottom: 0 },
    });

    onChartReady?.(chart);

    return () => {
      onChartReady?.(null);
      chart.remove();
      chartRef.current = null;
      candleSeriesRef.current = null;
      volumeSeriesRef.current = null;
      smaSeriesRef.current = [];
      bbSeriesRef.current = [];
      sessionSeriesRef.current = {};
      runningRef.current = { current: null };
    };
    // onChartReady deliberately omitted — referencing a fresh callback on
    // every parent render would re-initialise the chart, which we never want.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── 2. Static data: candles, volume, indicators, session markers ─────
  useEffect(() => {
    const chart = chartRef.current;
    const candleSeries = candleSeriesRef.current;
    const volumeSeries = volumeSeriesRef.current;
    if (!chart || !candleSeries || !volumeSeries || marketData.length === 0) return;

    // Candles
    const chartCandles: ChartCandle[] = backendToChart(marketData.map(m => m.candle));
    candleSeries.setData(chartCandles.map(c => ({
      time: c.time as UTCTimestamp, open: c.open, high: c.high, low: c.low, close: c.close,
    })));
    runningRef.current = initRunning(chartCandles);

    // Volume — green when close >= open, red otherwise
    volumeSeries.setData(marketData.map(m => ({
      time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
      value: m.candle.volume,
      color: m.candle.close >= m.candle.open
        ? "rgba(16,185,129,0.4)" : "rgba(244,63,94,0.4)",
    })));

    // SMA overlays — dispose previous, recreate
    smaSeriesRef.current.forEach(s => chart.removeSeries(s));
    smaSeriesRef.current = [];
    const smaConfigs: Array<[keyof MarketData["indicators"], string]> = [
      ["sma_10", "#fbbf24"], ["sma_25", "#60a5fa"], ["sma_60", "#a78bfa"],
    ];
    for (const [field, color] of smaConfigs) {
      const s = chart.addLineSeries({ color, lineWidth: 1, priceLineVisible: false, lastValueVisible: false });
      s.setData(marketData
        .filter(m => Number.isFinite(m.indicators[field] as number) && (m.indicators[field] as number) > 0)
        .map(m => ({
          time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
          value: m.indicators[field] as number,
        })));
      smaSeriesRef.current.push(s);
    }

    // Bollinger Bands
    bbSeriesRef.current.forEach(s => chart.removeSeries(s));
    bbSeriesRef.current = [];
    const bbConfigs: Array<[keyof MarketData["indicators"], string]> = [
      ["bollinger_upper", "rgba(148,163,184,0.6)"],
      ["bollinger_middle", "rgba(148,163,184,0.4)"],
      ["bollinger_lower", "rgba(148,163,184,0.6)"],
    ];
    for (const [field, color] of bbConfigs) {
      const s = chart.addLineSeries({ color, lineWidth: 1, priceLineVisible: false, lastValueVisible: false });
      s.setData(marketData
        .filter(m => Number.isFinite(m.indicators[field] as number) && (m.indicators[field] as number) > 0)
        .map(m => ({
          time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
          value: m.indicators[field] as number,
        })));
      bbSeriesRef.current.push(s);
    }

    // Session markers — one dummy LineSeries per session so we can toggle/
    // recolour individually. Markers go on each session's own series.
    Object.values(sessionSeriesRef.current).forEach(s => chart.removeSeries(s));
    sessionSeriesRef.current = {};

    const sessionIds = sessions.map(s => s.id).slice().sort((a, b) => a - b);
    for (const session of sessions) {
      const series = chart.addLineSeries({
        color: "rgba(0,0,0,0)",  // invisible line — markers only
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      sessionSeriesRef.current[session.id] = series;

      const trades = tradesBySession[session.id] ?? [];
      const color = colorFor(session.id, sessionIds);
      const markers: SeriesMarker<Time>[] = trades.map(t => ({
        time: isoToUtcSec(t.ts) as UTCTimestamp,
        position: t.side === "buy" ? "belowBar" : "aboveBar",
        color,
        shape: t.side === "buy" ? "arrowUp" : "arrowDown",
        text: t.is_real ? `${session.label}!` : session.label,
        size: t.is_real ? 2 : 1,
      }));
      // setMarkers requires non-empty data on the series. Plant a single
      // invisible point at the first marker's time so the markers attach.
      if (markers.length > 0) {
        series.setData(markers.map(m => ({ time: m.time, value: 0 })));
        series.setMarkers(markers);
      }
    }
  }, [marketData, sessions, tradesBySession]);

  // ── 3. Overlay toggles ───────────────────────────────────────────────
  useEffect(() => {
    smaSeriesRef.current.forEach(s => s.applyOptions({ visible: overlay.sma }));
    bbSeriesRef.current.forEach(s => s.applyOptions({ visible: overlay.bb }));
    if (volumeSeriesRef.current) {
      volumeSeriesRef.current.applyOptions({ visible: overlay.volume });
    }
  }, [overlay]);

  // ── 4. Live tick — update the running candle in place ────────────────
  useEffect(() => {
    if (!tick || !candleSeriesRef.current) return;
    const { updated } = applyTick(runningRef.current, tick.price, tick.ts_ms);
    candleSeriesRef.current.update({
      time: updated.time as UTCTimestamp,
      open: updated.open, high: updated.high, low: updated.low, close: updated.close,
    });
  }, [tick]);

  // ── 5. Apply user-selected visible window ────────────────────────────
  // Right edge always tracks "now" so the running candle stays in view.
  // Sister chart (SignalLaneChart) follows via useChartSync subscription.
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart || marketData.length === 0) return;
    const from = Math.floor(new Date(rangeStart + "T00:00:00Z").getTime() / 1000);
    const to = Math.floor(Date.now() / 1000);
    if (Number.isFinite(from) && from < to) {
      chart.timeScale().setVisibleRange({ from: from as Time, to: to as Time });
    }
  }, [rangeStart, marketData]);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2 text-xs">
          <ToggleChip label="SMA" on={overlay.sma} onChange={v => setOverlay(o => ({ ...o, sma: v }))} />
          <ToggleChip label="BB" on={overlay.bb} onChange={v => setOverlay(o => ({ ...o, bb: v }))} />
          <ToggleChip label="Volume" on={overlay.volume} onChange={v => setOverlay(o => ({ ...o, volume: v }))} />
        </div>
        <div className="flex items-center gap-2 text-xs text-zinc-400">
          <span>From</span>
          <input
            type="date"
            value={rangeStart}
            onChange={(e) => setRangeStart(e.target.value)}
            className="bg-zinc-800 border border-zinc-700 rounded-md px-2 py-1 text-zinc-200"
          />
          <span className="text-zinc-600">→ now</span>
        </div>
      </div>
      <div ref={containerRef} className="w-full h-[420px] bg-[#0c0c0f] border border-[#1e1e26] rounded-xl" />
    </div>
  );
}

function ToggleChip({ label, on, onChange }: { label: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!on)}
      className={`px-2.5 py-1 rounded-md border text-xs ${
        on ? "bg-zinc-800 border-zinc-700 text-zinc-200" : "bg-transparent border-zinc-800 text-zinc-500"
      }`}
    >
      {label}
    </button>
  );
}
