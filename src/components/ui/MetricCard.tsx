import { TrendingUp, TrendingDown } from "lucide-react";

interface MetricCardProps {
  label: string;
  value: string;
  trend?: "up" | "down";
  color?: "green" | "red" | "amber" | "neutral";
  className?: string;
}

const barColors = {
  green: "bg-emerald-500",
  red: "bg-rose-500",
  amber: "bg-amber-500",
  neutral: "bg-zinc-700",
};

const valueColors = {
  green: "text-emerald-400",
  red: "text-rose-400",
  amber: "text-amber-400",
  neutral: "text-zinc-100",
};

export function MetricCard({ label, value, trend, color = "neutral", className }: MetricCardProps) {
  return (
    <div
      className={`bg-[#0c0c0f] border border-[#1e1e26] rounded-xl overflow-hidden transition-all duration-200 hover:border-[#2a2a35] group ${className || ""}`}
      style={{ boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03)" }}
    >
      <div className="px-4 py-3.5">
        <p className="text-xs font-medium text-zinc-500 mb-1.5">{label}</p>
        <div className="flex items-center gap-2">
          <p className={`text-xl font-semibold font-data ${valueColors[color]}`}>{value}</p>
          {trend === "up" && <TrendingUp size={16} className="text-emerald-400" />}
          {trend === "down" && <TrendingDown size={16} className="text-rose-400" />}
        </div>
      </div>
      <div className={`h-0.5 ${barColors[color]} opacity-60 group-hover:opacity-100 transition-opacity`} />
    </div>
  );
}
