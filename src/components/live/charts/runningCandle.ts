import type { Candle } from "../../../types";

/** Chart-friendly candle. lightweight-charts uses `time` as UTC epoch seconds. */
export interface ChartCandle {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
}

const HOUR_MS = 3_600_000;
const HOUR_S = 3_600;

/** Start-of-hour bucket as UTC epoch seconds for a given epoch ms. */
export function hourBucketSec(tsMs: number): number {
  return Math.floor(tsMs / HOUR_MS) * HOUR_S;
}

/** Convert ISO/RFC3339 timestamp to UTC epoch seconds. */
export function isoToUtcSec(iso: string): number {
  return Math.floor(new Date(iso).getTime() / 1000);
}

/** Convert backend Candle[] to chart array, sorted ascending by time. */
export function backendToChart(candles: Candle[]): ChartCandle[] {
  return candles
    .map(c => ({
      time: isoToUtcSec(c.timestamp),
      open: c.open,
      high: c.high,
      low: c.low,
      close: c.close,
    }))
    .sort((a, b) => a.time - b.time);
}

export interface RunningCandleState {
  /** The candle currently being mutated by ticks. `null` until first tick. */
  current: ChartCandle | null;
}

/** Apply a single tick to the running candle. Returns:
 *   - `finalized`: the previous bar that just rolled to history (null if still same hour)
 *   - `updated`: the bar to push to series.update(...)
 *
 * Mutates `state.current` in place.
 */
export function applyTick(
  state: RunningCandleState,
  tickPrice: number,
  tickTsMs: number,
): { finalized: ChartCandle | null; updated: ChartCandle } {
  const time = hourBucketSec(tickTsMs);
  if (state.current == null) {
    state.current = { time, open: tickPrice, high: tickPrice, low: tickPrice, close: tickPrice };
    return { finalized: null, updated: state.current };
  }
  if (time > state.current.time) {
    const finalized = state.current;
    state.current = { time, open: tickPrice, high: tickPrice, low: tickPrice, close: tickPrice };
    return { finalized, updated: state.current };
  }
  // same hour — extend high/low, advance close
  if (tickPrice > state.current.high) state.current.high = tickPrice;
  if (tickPrice < state.current.low) state.current.low = tickPrice;
  state.current.close = tickPrice;
  return { finalized: null, updated: state.current };
}

/** Seed running state from the most recent loaded bar. Returns a fresh copy
 *  so subsequent mutations don't bleed into the input array. */
export function initRunning(loaded: ChartCandle[]): RunningCandleState {
  if (loaded.length === 0) return { current: null };
  return { current: { ...loaded[loaded.length - 1] } };
}
