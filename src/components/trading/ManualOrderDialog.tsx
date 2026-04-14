import { useState } from "react";
import { manualBuy, manualSell } from "../../lib/api";

interface Props {
  side: "buy" | "sell";
  market: string;
  currentPrice: number;
  onClose: () => void;
  onSuccess: (msg: string) => void;
  onError: (msg: string) => void;
}

export default function ManualOrderDialog({
  side,
  market,
  currentPrice,
  onClose,
  onSuccess,
  onError,
}: Props) {
  const [orderType, setOrderType] = useState<"limit">("limit");
  const [amount, setAmount] = useState("");
  const [price, setPrice] = useState(currentPrice.toString());
  const [submitting, setSubmitting] = useState(false);

  const feeRate = 0.0005;
  const numPrice = Number(price) || 0;
  const numAmount = Number(amount) || 0;

  const total = side === "buy" ? numAmount : numAmount * numPrice;
  const fee = total * feeRate;

  const handleSubmit = async () => {
    if (numAmount <= 0 || numPrice <= 0) return;
    setSubmitting(true);
    try {
      const volume =
        side === "buy" ? numAmount / numPrice : numAmount;
      const fn = side === "buy" ? manualBuy : manualSell;
      await fn(market, volume, numPrice);
      onSuccess(
        `${side === "buy" ? "Buy" : "Sell"} order placed: ${volume.toFixed(8)} @ ${numPrice.toLocaleString()}`
      );
      onClose();
    } catch (e) {
      onError(`Order failed: ${e}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-gray-900 rounded-lg p-6 w-96 border border-gray-700">
        <h2
          className={`text-lg font-bold mb-4 ${
            side === "buy" ? "text-green-400" : "text-red-400"
          }`}
        >
          Manual {side === "buy" ? "Buy" : "Sell"}
        </h2>

        {/* Order type */}
        <div className="mb-3">
          <label className="block text-sm text-gray-400 mb-1">Order Type</label>
          <div className="flex gap-2">
            <button
              className={`flex-1 py-1 rounded text-sm ${
                orderType === "limit"
                  ? "bg-violet-600 text-white"
                  : "bg-gray-800 text-gray-400"
              }`}
              onClick={() => setOrderType("limit")}
            >
              Limit
            </button>
          </div>
        </div>

        {/* Amount */}
        <div className="mb-3">
          <label className="block text-sm text-gray-400 mb-1">
            {side === "buy" ? "Amount (KRW)" : "Volume (Coin)"}
          </label>
          <input
            type="number"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
            placeholder={side === "buy" ? "e.g. 100000" : "e.g. 0.001"}
          />
        </div>

        {/* Price */}
        <div className="mb-3">
          <label className="block text-sm text-gray-400 mb-1">Price (KRW)</label>
          <input
            type="number"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-white"
          />
        </div>

        {/* Fee preview */}
        <div className="bg-gray-800 rounded p-3 mb-4 text-sm space-y-1">
          <div className="flex justify-between text-gray-400">
            <span>Total</span>
            <span className="text-white font-mono">{total.toLocaleString()} KRW</span>
          </div>
          <div className="flex justify-between text-gray-400">
            <span>Fee (0.05%)</span>
            <span className="text-white font-mono">{fee.toFixed(0)} KRW</span>
          </div>
        </div>

        {/* Buttons */}
        <div className="flex gap-3">
          <button
            onClick={onClose}
            className="flex-1 py-2 bg-gray-800 text-gray-300 rounded hover:bg-gray-700 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={submitting || numAmount <= 0 || numPrice <= 0}
            className={`flex-1 py-2 rounded font-medium transition-colors disabled:bg-gray-700 disabled:text-gray-500 ${
              side === "buy"
                ? "bg-green-600 hover:bg-green-700 text-white"
                : "bg-red-600 hover:bg-red-700 text-white"
            }`}
          >
            {submitting
              ? "Submitting..."
              : side === "buy"
              ? "Confirm Buy"
              : "Confirm Sell"}
          </button>
        </div>
      </div>
    </div>
  );
}
