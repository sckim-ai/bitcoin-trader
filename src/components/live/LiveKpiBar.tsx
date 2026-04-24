import { Card, CardContent } from "../ui/Card";
import type { TickData } from "../../types";

interface Props {
  tick: TickData | undefined;
  market?: string;
}

export default function LiveKpiBar({ tick, market = "KRW-ETH" }: Props) {
  const price = tick?.price;
  const change = tick?.change_pct ?? 0;
  const changeColor = change > 0 ? "bg-emerald-500/15 text-emerald-400"
    : change < 0 ? "bg-rose-500/15 text-rose-400" : "bg-zinc-700/30 text-zinc-400";
  const lagMs = tick ? Date.now() - tick.ts_ms : Infinity;
  const stale = lagMs > 10_000;

  return (
    <Card>
      <CardContent className="flex items-center justify-between">
        <div>
          <p className="text-xs font-medium text-zinc-500 mb-1">{market}</p>
          <p className="text-[32px] font-bold text-zinc-100 font-data leading-tight">
            {price != null ? price.toLocaleString() : "--"}
            <span className="text-sm ml-2 text-zinc-600 font-sans font-normal">KRW</span>
          </p>
        </div>
        <div className="flex items-center gap-2">
          {stale && price != null && (
            <span className="text-[10px] text-zinc-600">stale</span>
          )}
          {price != null && (
            <span className={`inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-semibold font-data ${changeColor}`}>
              {change >= 0 ? "+" : ""}{change.toFixed(2)}%
            </span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
