import { useEffect, useRef } from "react";
import {
  createChart,
  type IChartApi,
  type ISeriesApi,
  type Time,
  CandlestickSeries,
  HistogramSeries,
  ColorType,
} from "lightweight-charts";
import type { Candle } from "../../types";

interface Props {
  candles: Candle[];
  timeframe?: string;
  livePrice?: number | null;
}

type UTC = import("lightweight-charts").UTCTimestamp;

function barTime(c: Candle): UTC {
  return (new Date(c.timestamp).getTime() / 1000) as UTC;
}

function pad(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

function formatTick(time: Time, timeframe: string): string {
  const ts = (typeof time === "number" ? time : 0) * 1000;
  const d = new Date(ts);
  const mm = pad(d.getMonth() + 1);
  const dd = pad(d.getDate());
  if (timeframe === "hour") {
    return `${mm}-${dd} ${pad(d.getHours())}:00`;
  }
  return `${d.getFullYear()}-${mm}-${dd}`;
}

function formatCrosshair(time: Time, timeframe: string): string {
  const ts = (typeof time === "number" ? time : 0) * 1000;
  const d = new Date(ts);
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  if (timeframe === "hour") {
    return `${date} ${pad(d.getHours())}:00`;
  }
  return date;
}

export default function CandlestickChart({ candles, timeframe = "hour", livePrice }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const lastBarRef = useRef<{ time: UTC; open: number; high: number; low: number } | null>(null);
  const fittedRef = useRef(false);

  useEffect(() => {
    if (!containerRef.current) return;

    const isHour = timeframe === "hour";
    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "#030712" },
        textColor: "#9ca3af",
      },
      grid: {
        vertLines: { color: "#1f2937" },
        horzLines: { color: "#1f2937" },
      },
      width: containerRef.current.clientWidth,
      height: 400,
      crosshair: {
        mode: 0,
      },
      timeScale: {
        borderColor: "#374151",
        timeVisible: isHour,
        secondsVisible: false,
        tickMarkFormatter: (time: Time) => formatTick(time, timeframe),
      },
      localization: {
        timeFormatter: (time: Time) => formatCrosshair(time, timeframe),
      },
      rightPriceScale: {
        borderColor: "#374151",
      },
    });

    const candleSeries = chart.addSeries(CandlestickSeries, {
      upColor: "#22c55e",
      downColor: "#ef4444",
      borderDownColor: "#ef4444",
      borderUpColor: "#22c55e",
      wickDownColor: "#ef4444",
      wickUpColor: "#22c55e",
    });

    const volumeSeries = chart.addSeries(HistogramSeries, {
      priceFormat: { type: "volume" },
      priceScaleId: "volume",
    });

    chart.priceScale("volume").applyOptions({
      scaleMargins: { top: 0.8, bottom: 0 },
    });

    chartRef.current = chart;
    candleSeriesRef.current = candleSeries;
    volumeSeriesRef.current = volumeSeries;

    const handleResize = () => {
      if (containerRef.current) {
        chart.applyOptions({ width: containerRef.current.clientWidth });
      }
    };
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      chart.remove();
    };
  }, [timeframe]);

  // setData on full-array changes; fitContent only on first load to avoid scroll jitter on background updates.
  useEffect(() => {
    if (!candleSeriesRef.current || !volumeSeriesRef.current || candles.length === 0) return;

    const candleData = candles.map((c) => ({
      time: barTime(c),
      open: c.open,
      high: c.high,
      low: c.low,
      close: c.close,
    }));
    const volumeData = candles.map((c) => ({
      time: barTime(c),
      value: c.volume,
      color: c.close >= c.open ? "rgba(34,197,94,0.3)" : "rgba(239,68,68,0.3)",
    }));

    candleSeriesRef.current.setData(candleData);
    volumeSeriesRef.current.setData(volumeData);

    const last = candles[candles.length - 1];
    lastBarRef.current = {
      time: barTime(last),
      open: last.open,
      high: last.high,
      low: last.low,
    };

    if (!fittedRef.current) {
      chartRef.current?.timeScale().fitContent();
      fittedRef.current = true;
    }
  }, [candles]);

  // Reset fit flag when timeframe/market changes (component remounts on timeframe change anyway).
  useEffect(() => {
    fittedRef.current = false;
  }, [timeframe]);

  // Live price → update last bar's close (and stretch high/low if needed).
  useEffect(() => {
    if (!candleSeriesRef.current || livePrice == null || !lastBarRef.current) return;
    const bar = lastBarRef.current;
    bar.high = Math.max(bar.high, livePrice);
    bar.low = Math.min(bar.low, livePrice);
    candleSeriesRef.current.update({
      time: bar.time,
      open: bar.open,
      high: bar.high,
      low: bar.low,
      close: livePrice,
    });
  }, [livePrice]);

  return <div ref={containerRef} className="w-full rounded-lg overflow-hidden" />;
}
