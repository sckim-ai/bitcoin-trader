import type { PositionInfo } from "../../types";
import { Card, CardContent, CardHeader } from "../ui/Card";

interface Props {
  position: PositionInfo | null;
  currentPrice: number;
}

export default function PositionCard({ position, currentPrice }: Props) {
  const isHolding = position?.status === "holding";
  const pnlPct =
    isHolding && position.buy_price > 0
      ? ((currentPrice - position.buy_price) / position.buy_price) * 100
      : 0;

  return (
    <Card>
      <CardHeader className="flex items-center gap-2">
        <div className={`w-2 h-2 rounded-full ${isHolding ? "bg-emerald-400 shadow-lg shadow-emerald-500/30" : "bg-zinc-600"}`} />
        <h3 className="text-sm font-semibold text-zinc-300">Position</h3>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex justify-between items-center">
          <span className="text-xs text-zinc-500">Status</span>
          <span
            className={`text-sm font-semibold ${
              isHolding ? "text-amber-400" : "text-zinc-600"
            }`}
          >
            {isHolding ? "Holding" : "Idle"}
          </span>
        </div>
        {isHolding && (
          <>
            <div className="flex justify-between items-center">
              <span className="text-xs text-zinc-500">Buy Price</span>
              <span className="text-sm text-zinc-200 font-data">
                {position.buy_price.toLocaleString()}
              </span>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-xs text-zinc-500">Volume</span>
              <span className="text-sm text-zinc-200 font-data">
                {position.buy_volume.toFixed(8)}
              </span>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-xs text-zinc-500">Unrealized P/L</span>
              <span
                className={`text-sm font-bold font-data px-2 py-0.5 rounded-md ${
                  pnlPct >= 0
                    ? "text-emerald-400 bg-emerald-500/10"
                    : "text-rose-400 bg-rose-500/10"
                }`}
              >
                {pnlPct >= 0 ? "+" : ""}
                {pnlPct.toFixed(2)}%
              </span>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
