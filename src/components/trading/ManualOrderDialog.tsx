import { useState } from "react";
import { manualBuy, manualSell } from "../../lib/api";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";

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
      const volume = side === "buy" ? numAmount / numPrice : numAmount;
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
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50">
      <div
        className="bg-[#0c0c0f] rounded-2xl p-6 w-96 border border-[#1e1e26] animate-fade-in"
        style={{ boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03), 0 25px 50px -12px rgba(0,0,0,0.5)" }}
      >
        <h2
          className={`text-lg font-bold mb-5 ${
            side === "buy" ? "text-emerald-400" : "text-rose-400"
          }`}
        >
          Manual {side === "buy" ? "Buy" : "Sell"}
        </h2>

        {/* Order type */}
        <div className="mb-4">
          <label className="block text-xs font-medium text-zinc-500 mb-1.5">Order Type</label>
          <div className="flex bg-[#141419] border border-[#1e1e26] rounded-lg p-0.5">
            <button className="flex-1 py-1.5 rounded-md text-xs font-semibold bg-amber-500 text-black">
              Limit
            </button>
          </div>
        </div>

        <div className="space-y-3 mb-4">
          <Input
            label={side === "buy" ? "Amount (KRW)" : "Volume (Coin)"}
            type="number"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            placeholder={side === "buy" ? "e.g. 100000" : "e.g. 0.001"}
          />
          <Input
            label="Price (KRW)"
            type="number"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
          />
        </div>

        {/* Fee preview */}
        <div className="bg-[#141419] border border-[#1e1e26] rounded-xl p-3 mb-5 text-sm space-y-1.5">
          <div className="flex justify-between">
            <span className="text-zinc-500 text-xs">Total</span>
            <span className="text-zinc-200 font-data text-xs">{total.toLocaleString()} KRW</span>
          </div>
          <div className="flex justify-between">
            <span className="text-zinc-500 text-xs">Fee (0.05%)</span>
            <span className="text-zinc-200 font-data text-xs">{fee.toFixed(0)} KRW</span>
          </div>
        </div>

        {/* Buttons */}
        <div className="flex gap-3">
          <Button onClick={onClose} variant="secondary" className="flex-1" size="lg">
            Cancel
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={submitting || numAmount <= 0 || numPrice <= 0}
            variant={side === "buy" ? "success" : "danger"}
            className="flex-1"
            size="lg"
          >
            {submitting
              ? "Submitting..."
              : side === "buy"
              ? "Confirm Buy"
              : "Confirm Sell"}
          </Button>
        </div>
      </div>
    </div>
  );
}
