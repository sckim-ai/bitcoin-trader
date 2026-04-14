import ReactECharts from "echarts-for-react";
import type { ParetoSolution } from "../../types";

interface Props {
  solutions: ParetoSolution[];
}

export default function ParetoChart({ solutions }: Props) {
  const option = {
    backgroundColor: "transparent",
    title: {
      text: "Pareto Front",
      textStyle: { color: "#e5e7eb", fontSize: 14 },
      left: "center",
    },
    tooltip: {
      trigger: "item" as const,
      formatter: (p: { value: number[] }) =>
        `Return: ${p.value[0].toFixed(2)}%<br/>Win Rate: ${p.value[1].toFixed(1)}%`,
    },
    xAxis: {
      name: "Total Return (%)",
      nameTextStyle: { color: "#9ca3af" },
      axisLine: { lineStyle: { color: "#374151" } },
      axisLabel: { color: "#9ca3af" },
      splitLine: { lineStyle: { color: "#1f2937" } },
    },
    yAxis: {
      name: "Win Rate (%)",
      nameTextStyle: { color: "#9ca3af" },
      axisLine: { lineStyle: { color: "#374151" } },
      axisLabel: { color: "#9ca3af" },
      splitLine: { lineStyle: { color: "#1f2937" } },
    },
    series: [
      {
        type: "scatter",
        symbolSize: 10,
        data: solutions.map((s) => s.objectives),
        itemStyle: { color: "#8b5cf6" },
      },
    ],
  };

  return (
    <ReactECharts
      option={option}
      style={{ height: 350 }}
      theme="dark"
      notMerge
    />
  );
}
