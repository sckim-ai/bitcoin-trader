import type { PositionInfo } from "../../types";

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
    <div className="bg-gray-900 rounded-lg p-4">
      <h3 className="text-sm text-gray-400 mb-3">Position</h3>
      <div className="space-y-2">
        <div className="flex justify-between">
          <span className="text-gray-400">Status</span>
          <span
            className={`font-medium ${
              isHolding ? "text-yellow-400" : "text-gray-500"
            }`}
          >
            {isHolding ? "Holding" : "Idle"}
          </span>
        </div>
        {isHolding && (
          <>
            <div className="flex justify-between">
              <span className="text-gray-400">Buy Price</span>
              <span className="text-white font-mono">
                {position.buy_price.toLocaleString()}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-400">Volume</span>
              <span className="text-white font-mono">
                {position.buy_volume.toFixed(8)}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-400">Unrealized P/L</span>
              <span
                className={`font-bold font-mono ${
                  pnlPct >= 0 ? "text-green-400" : "text-red-400"
                }`}
              >
                {pnlPct >= 0 ? "+" : ""}
                {pnlPct.toFixed(2)}%
              </span>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
