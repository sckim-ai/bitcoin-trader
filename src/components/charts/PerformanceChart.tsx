import ReactECharts from "echarts-for-react";
import type { TradeRecord } from "../../types";

interface Props {
  trades: TradeRecord[];
}

export default function PerformanceChart({ trades }: Props) {
  // Build equity curve from trades
  let equity = 1.0;
  const equityCurve = [{ index: 0, value: 1.0 }];
  for (const trade of trades) {
    equity *= 1 + trade.pnl_pct;
    equityCurve.push({ index: trade.sell_index, value: equity });
  }

  const option = {
    backgroundColor: "transparent",
    title: {
      text: "Equity Curve",
      textStyle: { color: "#e5e7eb", fontSize: 14 },
      left: "center",
    },
    tooltip: {
      trigger: "axis" as const,
      formatter: (p: { value: number[] }[]) =>
        `Bar: ${p[0].value[0]}<br/>Equity: ${p[0].value[1].toFixed(4)}`,
    },
    xAxis: {
      name: "Bar Index",
      nameTextStyle: { color: "#9ca3af" },
      axisLine: { lineStyle: { color: "#374151" } },
      axisLabel: { color: "#9ca3af" },
      splitLine: { show: false },
    },
    yAxis: {
      name: "Equity",
      nameTextStyle: { color: "#9ca3af" },
      axisLine: { lineStyle: { color: "#374151" } },
      axisLabel: { color: "#9ca3af" },
      splitLine: { lineStyle: { color: "#1f2937" } },
    },
    series: [
      {
        type: "line",
        smooth: true,
        showSymbol: false,
        data: equityCurve.map((p) => [p.index, p.value]),
        lineStyle: { color: "#22c55e", width: 2 },
        areaStyle: {
          color: {
            type: "linear" as const,
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: "rgba(34,197,94,0.3)" },
              { offset: 1, color: "rgba(34,197,94,0.0)" },
            ],
          },
        },
      },
    ],
  };

  return (
    <ReactECharts
      option={option}
      style={{ height: 300 }}
      theme="dark"
      notMerge
    />
  );
}
