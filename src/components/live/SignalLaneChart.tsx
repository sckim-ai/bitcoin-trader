import { useEffect, useRef } from "react";
import {
  createChart, ColorType,
  type IChartApi, type ISeriesApi, type SeriesMarker, type Time, type UTCTimestamp,
} from "lightweight-charts";
import type { LiveSession, LiveTrade } from "../../types";
import { isoToUtcSec } from "./charts/runningCandle";
import { colorFor } from "./charts/sessionPalette";

interface Props {
  sessions: LiveSession[];
  tradesBySession: Record<number, LiveTrade[]>;
  onChartReady?: (chart: IChartApi | null) => void;
}

export default function SignalLaneChart({ sessions, tradesBySession, onChartReady }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<Record<number, ISeriesApi<"Line">>>({});

  // Initialise chart once
  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "#0c0c0f" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "#1e1e26" },
        horzLines: { color: "transparent" },
      },
      timeScale: { borderColor: "#1e1e26", timeVisible: true, secondsVisible: false },
      rightPriceScale: { borderColor: "#1e1e26", visible: false },
      autoSize: true,
    });
    chartRef.current = chart;
    onChartReady?.(chart);
    return () => {
      onChartReady?.(null);
      chart.remove();
      chartRef.current = null;
      seriesRef.current = {};
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Re-render lanes whenever sessions or trades change
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;

    Object.values(seriesRef.current).forEach(s => chart.removeSeries(s));
    seriesRef.current = {};

    if (sessions.length === 0) return;

    const sessionIds = sessions.map(s => s.id).slice().sort((a, b) => a - b);

    // Each session occupies a y-band. We use lane index = i (1..N), with the
    // line itself invisible — only markers show. The price scale is hidden.
    sessions.forEach((session) => {
      const lane = sessionIds.length - sessionIds.indexOf(session.id);  // top → bottom
      const scaleId = `lane-${session.id}`;
      const series = chart.addLineSeries({
        color: "rgba(0,0,0,0)",
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
        priceScaleId: scaleId,
      });
      // Hidden per-session scale so the integer lane value never paints axis
      // labels nor influences the (already-hidden) right scale's range.
      chart.priceScale(scaleId).applyOptions({ visible: false });
      seriesRef.current[session.id] = series;

      const trades = tradesBySession[session.id] ?? [];
      if (trades.length === 0) return;

      const color = colorFor(session.id, sessionIds);
      const data = trades.map(t => ({
        time: isoToUtcSec(t.ts) as UTCTimestamp,
        value: lane,
      }));
      series.setData(data);

      const markers: SeriesMarker<Time>[] = trades.map(t => ({
        time: isoToUtcSec(t.ts) as UTCTimestamp,
        position: "inBar",
        color,
        shape: t.side === "buy" ? "circle" : "square",
        text: `${session.label}:${t.side}`,
        size: 1,
      }));
      series.setMarkers(markers);
    });
  }, [sessions, tradesBySession]);

  return (
    <div ref={containerRef} className="w-full h-[140px] bg-[#0c0c0f] border border-[#1e1e26] rounded-xl" />
  );
}
