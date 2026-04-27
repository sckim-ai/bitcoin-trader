import { useEffect, useRef, useState } from "react";
import {
  createChart, ColorType,
  type IChartApi, type ISeriesApi, type SeriesMarker, type Time, type UTCTimestamp,
} from "lightweight-charts";
import type { LiveSession, LiveTrade, MarketData, SignalEvent, TickData } from "../../types";
import {
  backendToChart, isoToUtcSec, applyTick, initRunning, hourBucketSec,
  type ChartCandle, type RunningCandleState,
} from "./charts/runningCandle";
import { colorFor } from "./charts/sessionPalette";

interface Props {
  /** Already filtered by parent to the visible time window. */
  marketData: MarketData[];
  sessions: LiveSession[];
  /** Already filtered by parent to the visible time window. */
  tradesBySession: Record<number, LiveTrade[]>;
  tick: TickData | undefined;
  /** Per-bar signal log for the strip pane (already filtered upstream). */
  signals: SignalEvent[];
  /** Called once when the underlying chart is created; null on unmount. */
  onChartReady?: (chart: IChartApi | null) => void;
}

/** Signal-type → strip colour. Keys match the lowercase strings emitted by
 *  the engine's `determine_signal_type`. */
const STRIP_COLORS: Record<string, string> = {
  ready: "rgba(148, 163, 184, 0.45)",
  "buy ready": "rgba(134, 239, 172, 0.85)",
  buy: "rgba(16, 185, 129, 1.0)",
  hold: "rgba(251, 191, 36, 0.85)",
  "sell ready": "rgba(251, 146, 60, 0.85)",
  sell: "rgba(239, 68, 68, 1.0)",
};

interface OverlayState {
  sma: boolean;
  bb: boolean;
  volume: boolean;
}

/**
 * Pure renderer: parent controls what data lands here. Internal effects only:
 *   - 1: chart bootstrap (once)
 *   - 2: candles + indicators (when marketData changes)
 *   - 2b: session markers — pinned to a hidden price scale so they don't
 *         distort the candle Y axis (when sessions/trades change)
 *   - 3: overlay visibility toggles
 *   - 4: tick → running candle
 *
 * No setVisibleRange logic here. Parent filters the data to the user's chosen
 * window; lightweight-charts auto-fits to whatever it's given.
 */
export default function CandleChart({
  marketData, sessions, tradesBySession, tick, signals, onChartReady,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const stripSeriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const smaSeriesRef = useRef<ISeriesApi<"Line">[]>([]);
  const bbSeriesRef = useRef<ISeriesApi<"Line">[]>([]);
  const sessionSeriesRef = useRef<Record<number, ISeriesApi<"Line">>>({});
  const runningRef = useRef<RunningCandleState>({ current: null });

  const [overlay, setOverlay] = useState<OverlayState>({ sma: true, bb: false, volume: true });

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
      timeScale: {
        borderColor: "#1e1e26", timeVisible: true, secondsVisible: false,
        // Pin a small floor so wheel-zoom-out has the same hard limit as the
        // signal strip chart (both charts identical ⇒ sync stays in step).
        minBarSpacing: 0.5,
      },
      rightPriceScale: { borderColor: "#1e1e26" },
      // Drag the chart body to pan only; no axis-drag scaling. Wheel and
      // pinch still zoom for power users.
      handleScale: { axisPressedMouseMove: false, axisDoubleClickReset: false, mouseWheel: true, pinch: true },
      autoSize: true,
    });
    chartRef.current = chart;

    // Pane stack inside ONE chart instance. Right-scale is shared by candles
    // and indicators; volume + strip get their own hidden scales pinned to
    // discrete vertical bands at the bottom — no separate chart instance, no
    // sync to maintain.
    candleSeriesRef.current = chart.addCandlestickSeries({
      upColor: "#10b981", downColor: "#f43f5e",
      borderUpColor: "#10b981", borderDownColor: "#f43f5e",
      wickUpColor: "#10b981", wickDownColor: "#f43f5e",
    });
    chart.priceScale("right").applyOptions({
      scaleMargins: { top: 0.05, bottom: 0.25 }, // candles in top 70%
    });

    volumeSeriesRef.current = chart.addHistogramSeries({
      color: "#3f3f46",
      priceFormat: { type: "volume" },
      priceScaleId: "volume",
    });
    chart.priceScale("volume").applyOptions({
      scaleMargins: { top: 0.75, bottom: 0.12 }, // volume band 75-88%
      visible: false,
    });

    stripSeriesRef.current = chart.addHistogramSeries({
      priceFormat: { type: "volume" },
      priceScaleId: "strip",
      priceLineVisible: false,
      lastValueVisible: false,
    });
    chart.priceScale("strip").applyOptions({
      scaleMargins: { top: 0.9, bottom: 0 }, // signal strip band bottom 10%
      visible: false,
    });

    onChartReady?.(chart);

    return () => {
      onChartReady?.(null);
      chart.remove();
      chartRef.current = null;
      candleSeriesRef.current = null;
      volumeSeriesRef.current = null;
      stripSeriesRef.current = null;
      smaSeriesRef.current = [];
      bbSeriesRef.current = [];
      sessionSeriesRef.current = {};
      runningRef.current = { current: null };
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── 2. Candles + volume + indicators ─────────────────────────────────
  useEffect(() => {
    const chart = chartRef.current;
    const candleSeries = candleSeriesRef.current;
    const volumeSeries = volumeSeriesRef.current;
    if (!chart || !candleSeries || !volumeSeries || marketData.length === 0) return;

    const chartCandles: ChartCandle[] = backendToChart(marketData.map(m => m.candle));
    // Always include a "current hour bucket" placeholder so the candle
    // chart's right edge tracks the same time as SignalStripChart even
    // before the first tick arrives. Subsequent ticks update this same
    // bucket via applyTick().
    const nowBucket = hourBucketSec(Date.now());
    const lastBackend = chartCandles[chartCandles.length - 1];
    if (lastBackend && nowBucket > lastBackend.time) {
      chartCandles.push({
        time: nowBucket,
        open: lastBackend.close,
        high: lastBackend.close,
        low: lastBackend.close,
        close: lastBackend.close,
      });
    }
    candleSeries.setData(chartCandles.map(c => ({
      time: c.time as UTCTimestamp, open: c.open, high: c.high, low: c.low, close: c.close,
    })));
    runningRef.current = initRunning(chartCandles);

    volumeSeries.setData(marketData.map(m => ({
      time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
      value: m.candle.volume,
      color: m.candle.close >= m.candle.open
        ? "rgba(16,185,129,0.4)" : "rgba(244,63,94,0.4)",
    })));

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
  }, [marketData]);

  // ── 2b. Session markers ──────────────────────────────────────────────
  // Markers attach via a transparent line series. Each session series rides
  // on its own hidden price scale (`priceScaleId: 'markers-<id>'`) so the
  // marker line value (which we set to the trade price for natural placement)
  // does NOT participate in the main candle Y-axis auto-fit. Without this,
  // an early-session value of 0 forced the chart to include 0..5M, squashing
  // recent action into a sliver.
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;

    Object.values(sessionSeriesRef.current).forEach(s => chart.removeSeries(s));
    sessionSeriesRef.current = {};

    const sessionIds = sessions.map(s => s.id).slice().sort((a, b) => a - b);
    for (const session of sessions) {
      const scaleId = `markers-${session.id}`;
      const series = chart.addLineSeries({
        color: "rgba(0,0,0,0)",
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
        priceScaleId: scaleId,
      });
      // Hide this overlay scale so it doesn't render axis labels.
      chart.priceScale(scaleId).applyOptions({ visible: false });
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
      if (markers.length > 0) {
        // Anchor data uses the trade price so the marker's vertical
        // position (belowBar / aboveBar) is computed near the candle.
        series.setData(trades.map(t => ({
          time: isoToUtcSec(t.ts) as UTCTimestamp,
          value: t.price,
        })));
        series.setMarkers(markers);
      }
    }
  }, [sessions, tradesBySession]);

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

  // ── 5. Signal strip — colour every candle by current signal type ─────
  // Lives in the same chart instance so the time axis is shared with the
  // candle pane by construction. No external sync hook required.
  useEffect(() => {
    const series = stripSeriesRef.current;
    if (!series || marketData.length === 0) {
      series?.setData([]);
      return;
    }

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
        color: STRIP_COLORS[current] ?? STRIP_COLORS.ready,
      };
    });

    // Match the candles' running-bucket extension so the strip's right edge
    // tracks the live candle pixel-perfectly.
    const nowBucket = hourBucketSec(Date.now());
    const lastTime = data.length > 0 ? (data[data.length - 1].time as number) : null;
    if (lastTime != null && nowBucket > lastTime) {
      data.push({
        time: nowBucket as UTCTimestamp,
        value: 1,
        color: STRIP_COLORS[current] ?? STRIP_COLORS.ready,
      });
    }

    series.setData(data);
  }, [marketData, signals]);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="flex items-center gap-2 text-xs">
          <ToggleChip label="SMA" on={overlay.sma} onChange={v => setOverlay(o => ({ ...o, sma: v }))} />
          <ToggleChip label="BB" on={overlay.bb} onChange={v => setOverlay(o => ({ ...o, bb: v }))} />
          <ToggleChip label="Volume" on={overlay.volume} onChange={v => setOverlay(o => ({ ...o, volume: v }))} />
        </div>
        <div className="flex items-center gap-3 text-[10px] text-zinc-400 flex-wrap">
          <Legend label="Buy" color={STRIP_COLORS.buy} />
          <Legend label="Buy Ready" color={STRIP_COLORS["buy ready"]} />
          <Legend label="Hold" color={STRIP_COLORS.hold} />
          <Legend label="Sell Ready" color={STRIP_COLORS["sell ready"]} />
          <Legend label="Sell" color={STRIP_COLORS.sell} />
          <Legend label="Ready" color={STRIP_COLORS.ready} />
        </div>
      </div>
      <div ref={containerRef} className="w-full h-[480px] bg-[#0c0c0f] border border-[#1e1e26] rounded-xl" />
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
