# Live Trading Phase 3 Implementation Plan (2-Panel Charts)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 라이브 트레이딩 페이지에 `lightweight-charts` 기반 캔들 차트(SMA/BB/Volume 오버레이 + 세션별 매수/매도 마커)와 시간축이 동기화된 시그널 레인 차트를 추가하고, Phase 2의 틱 스트림으로 진행 중인 시간봉을 실시간 갱신한다.

**Architecture:** 백엔드는 손대지 않는다 — 기존 `get_market_data("ETH","hour")`로 봉 데이터(지표 포함)를, `list_session_trades(id)`로 세션별 체결 이력을 가져온다. 프론트는 chart 인스턴스 두 개(`CandleChart`, `SignalLaneChart`)를 만들고 `useChartSync` 훅으로 두 chart의 `timeScale`을 양방향 구독해 동기화한다. 진행 중인 시간봉은 `runningCandle` 상태로 별도 관리하며, Phase 2 `market:tick`을 받아 high/low/close를 갱신하고 hour 경계에서 새 봉으로 롤오버한다.

**Tech Stack:** React + TypeScript + Zustand. `lightweight-charts ^4.2` (TradingView, MIT). 백엔드 변경 없음.

**Scope (Phase 3):**
- 신규 컴포넌트: `CandleChart`, `SignalLaneChart`, `useChartSync` 훅
- 헬퍼: `sessionPalette` (Tableau 10), `runningCandle` (틱 → 봉 머지)
- Store 확장: 봉 캐시 + 세션별 trade 일괄 로드
- LiveTradingPage 레이아웃: KPI 바 → 캔들 차트 → 시그널 레인 차트 → 프리셋 → 세션 테이블

**Out-of-scope (Phase 4+):** 실전 승격 게이트, BuyReady/SellReady 등 중간 시그널 표시(현재 DB는 buy/sell만 저장). 로그 패널 세션 필터(현재 LiveTradingPage엔 별도 로그 패널 없음).

---

## 파일 구조 (Phase 3)

**신규:**
- `src/components/live/charts/sessionPalette.ts` — 세션별 색 할당 (Tableau 10 팔레트)
- `src/components/live/charts/runningCandle.ts` — 봉 변환 + 틱 머지 순수 함수
- `src/components/live/charts/useChartSync.ts` — 두 chart의 timeScale 양방향 동기화 훅
- `src/components/live/CandleChart.tsx` — 캔들 + 지표 오버레이 + 세션 마커 + 틱 갱신
- `src/components/live/SignalLaneChart.tsx` — 세션별 시그널 레인

**수정:**
- `package.json` — `lightweight-charts ^4.2` 의존성
- `src/stores/liveTradingStore.ts` — `candles` cache + `loadAllSessionTrades` action
- `src/pages/LiveTradingPage.tsx` — 차트 두 개 배치

---

## Task 1: lightweight-charts 의존성 추가

**Files:**
- Modify: `d:\SW\bitcoin-trader\package.json`

- [ ] **Step 1: 의존성 설치**

From repo root `d:\SW\bitcoin-trader`:

```bash
npm install lightweight-charts@^4.2
```

이 명령이 `package.json`의 `dependencies`에 `"lightweight-charts": "^4.2.x"` 형태로 추가하고 `package-lock.json`을 갱신한다.

- [ ] **Step 2: 빌드 확인**

```bash
npm run vite:build
```

Expected: 성공 (의존성만 추가했으므로 코드 변경 없음). 새 deprecation 경고 없는지 가볍게 확인.

- [ ] **Step 3: 커밋**

```bash
git add package.json package-lock.json
git commit -m "chore(live): add lightweight-charts ^4.2 dependency"
```

---

## Task 2: 세션별 색 팔레트

**Files:**
- Create: `d:\SW\bitcoin-trader\src\components\live\charts\sessionPalette.ts`

- [ ] **Step 1: 색 유틸 작성**

Create `src/components/live/charts/sessionPalette.ts`:

```typescript
// Tableau 10 — chosen because hues are designed for visual separability on
// both light and dark backgrounds. Wraps after 10 sessions.
const TABLEAU_10 = [
  "#4E79A7", "#F28E2B", "#E15759", "#76B7B2", "#59A14F",
  "#EDC948", "#B07AA1", "#FF9DA7", "#9C755F", "#BAB0AC",
] as const;

/**
 * Stable color assignment for a session — uses index modulo palette length so
 * the same session always gets the same color across re-renders, regardless
 * of the order sessions are listed.
 *
 * `sessionIds` should be a sorted array (e.g., by id ASC) so the assignment
 * is deterministic. The caller decides ordering — this function just looks
 * up the index.
 */
export function colorFor(sessionId: number, sessionIds: readonly number[]): string {
  const idx = sessionIds.indexOf(sessionId);
  if (idx < 0) return TABLEAU_10[0];
  return TABLEAU_10[idx % TABLEAU_10.length];
}

/** Hex → rgba with alpha. Used for translucent markers/lines. */
export function withAlpha(hex: string, alpha: number): string {
  const m = hex.match(/^#([0-9a-f]{6})$/i);
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  return `rgba(${r},${g},${b},${alpha})`;
}
```

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공.

- [ ] **Step 3: 커밋**

```bash
git add src/components/live/charts/sessionPalette.ts
git commit -m "feat(live): session color palette (Tableau 10) with alpha helper"
```

---

## Task 3: 진행 중인 봉 헬퍼 (순수 함수)

**Files:**
- Create: `d:\SW\bitcoin-trader\src\components\live\charts\runningCandle.ts`

`★ Why pure functions:` 틱→봉 머지 로직은 시간 버킷 계산 + 비교라서 차트 객체 없이 단독 검증 가능. 차트 라이프사이클과 분리해두면 hour 경계에서 발생하는 미묘한 버그(예: tick.ts_ms가 분초 단위로 들쭉날쭉)를 격리해 다룰 수 있다.

- [ ] **Step 1: 헬퍼 + 타입 작성**

Create `src/components/live/charts/runningCandle.ts`:

```typescript
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
```

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공.

- [ ] **Step 3: 커밋**

```bash
git add src/components/live/charts/runningCandle.ts
git commit -m "feat(live): runningCandle helper — backend→chart + tick merge"
```

---

## Task 4: 시간축 동기화 훅

**Files:**
- Create: `d:\SW\bitcoin-trader\src\components\live\charts\useChartSync.ts`

`★ API design:` Hook은 `IChartApi | null` 두 인스턴스를 직접 받는다 — refs 미러링은 호출 측에서 처리하지 않고, 각 chart 컴포넌트가 `onChartReady` 콜백으로 인스턴스를 부모 state에 올리도록 한다 (Task 6/7). 이 deps 기반 useEffect는 chart가 둘 다 준비되었을 때만 구독을 시작한다.

- [ ] **Step 1: 훅 작성**

Create `src/components/live/charts/useChartSync.ts`:

```typescript
import { useEffect } from "react";
import type { IChartApi, LogicalRange } from "lightweight-charts";

/**
 * Two-way bind two charts' visible logical range so panning/zooming one syncs
 * the other. `suppress` guards against re-entrant emissions on the same tick.
 * No-op while either chart is null.
 */
export function useChartSync(
  top: IChartApi | null,
  bottom: IChartApi | null,
) {
  useEffect(() => {
    if (!top || !bottom) return;
    let suppress = false;

    const onTop = (range: LogicalRange | null) => {
      if (!range || suppress) return;
      suppress = true;
      try { bottom.timeScale().setVisibleLogicalRange(range); }
      finally { suppress = false; }
    };
    const onBottom = (range: LogicalRange | null) => {
      if (!range || suppress) return;
      suppress = true;
      try { top.timeScale().setVisibleLogicalRange(range); }
      finally { suppress = false; }
    };

    top.timeScale().subscribeVisibleLogicalRangeChange(onTop);
    bottom.timeScale().subscribeVisibleLogicalRangeChange(onBottom);

    return () => {
      top.timeScale().unsubscribeVisibleLogicalRangeChange(onTop);
      bottom.timeScale().unsubscribeVisibleLogicalRangeChange(onBottom);
    };
  }, [top, bottom]);
}
```

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공.

- [ ] **Step 3: 커밋**

```bash
git add src/components/live/charts/useChartSync.ts
git commit -m "feat(live): useChartSync — bi-directional timescale sync hook"
```

---

## Task 5: Store 확장 — 봉 캐시 + 세션별 trade 일괄 로드

**Files:**
- Modify: `d:\SW\bitcoin-trader\src\stores\liveTradingStore.ts`

- [ ] **Step 1: store API 확장**

Read the existing store first to confirm structure (after Phase 2 it includes `ticks` + `deriveSessionPnl`). Add only what's described below — do not rewrite untouched portions.

추가할 import (`../lib/api`에서 `getMarketData`):

```typescript
import { getMarketData } from "../lib/api";
import type { MarketData } from "../types";
```

`LiveTradingState` 인터페이스에 다음 필드/메서드 추가:

```typescript
  /// Hourly KRW-ETH market data (candle + indicators). Loaded lazily by the
  /// chart component; cached so multiple chart mounts share one fetch.
  marketData: MarketData[] | null;
  loadingMarketData: boolean;
  loadMarketData: () => Promise<void>;
  /// Bulk-load trades for all sessions in parallel — cheaper than per-card.
  loadAllSessionTrades: () => Promise<void>;
```

초기값 + 구현 추가 (다른 메서드들 사이에 자연스럽게 끼워넣기):

```typescript
  marketData: null,
  loadingMarketData: false,

  loadMarketData: async () => {
    if (get().loadingMarketData) return;
    set({ loadingMarketData: true });
    try {
      const data = await getMarketData("ETH", "hour");
      set({ marketData: data });
    } finally {
      set({ loadingMarketData: false });
    }
  },

  loadAllSessionTrades: async () => {
    const ids = get().sessions.map(s => s.id);
    const results = await Promise.all(
      ids.map(async (id) => [id, await listSessionTrades(id)] as const),
    );
    const map: Record<number, typeof results[number][1]> = {};
    for (const [id, trades] of results) map[id] = trades;
    set({ tradesBySession: map });
  },
```

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공.

- [ ] **Step 3: 커밋**

```bash
git add src/stores/liveTradingStore.ts
git commit -m "feat(live): store — marketData cache + loadAllSessionTrades"
```

---

## Task 6: CandleChart 컴포넌트

**Files:**
- Create: `d:\SW\bitcoin-trader\src\components\live\CandleChart.tsx`

`★ Scope:` 이 태스크가 가장 큰 파일이지만 전부 한 컴포넌트 안에서 응집해야 자연스럽다 — chart 인스턴스, 시리즈 핸들들, useEffect cleanup이 한 라이프사이클로 묶임.

- [ ] **Step 1: 컴포넌트 작성**

Create `src/components/live/CandleChart.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import {
  createChart, ColorType,
  type IChartApi, type ISeriesApi, type SeriesMarker, type Time, type UTCTimestamp,
} from "lightweight-charts";
import type { LiveSession, LiveTrade, MarketData, TickData } from "../../types";
import {
  backendToChart, isoToUtcSec, applyTick, initRunning,
  type ChartCandle, type RunningCandleState,
} from "./charts/runningCandle";
import { colorFor } from "./charts/sessionPalette";

interface Props {
  marketData: MarketData[];
  sessions: LiveSession[];
  tradesBySession: Record<number, LiveTrade[]>;
  tick: TickData | undefined;
  /** Called once when the underlying chart is created; null on unmount. */
  onChartReady?: (chart: IChartApi | null) => void;
}

interface OverlayState {
  sma: boolean;
  bb: boolean;
  volume: boolean;
}

/**
 * Two changes are mutating the chart over time:
 *   1. Static: candles + indicators + markers are set once per render of
 *      `marketData` / `sessions` / `tradesBySession`.
 *   2. Live: every tick mutates the running candle via series.update().
 *
 * We deliberately do NOT pass `tick` through the static effect's deps —
 * otherwise every tick would tear down and rebuild every series, killing
 * performance and causing visual jitter.
 */
export default function CandleChart({
  marketData, sessions, tradesBySession, tick, onChartReady,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const smaSeriesRef = useRef<ISeriesApi<"Line">[]>([]);
  const bbSeriesRef = useRef<ISeriesApi<"Line">[]>([]);
  const sessionSeriesRef = useRef<Record<number, ISeriesApi<"Line">>>({});
  const runningRef = useRef<RunningCandleState>({ current: null });

  const [overlay, setOverlay] = useState<OverlayState>({ sma: true, bb: false, volume: true });

  // ── 1. Initialise chart instance once ────────────────────────────────
  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "#0c0c0f" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "#1e1e26" },
        horzLines: { color: "#1e1e26" },
      },
      timeScale: { borderColor: "#1e1e26", timeVisible: true, secondsVisible: false },
      rightPriceScale: { borderColor: "#1e1e26" },
      autoSize: true,
    });
    chartRef.current = chart;

    candleSeriesRef.current = chart.addCandlestickSeries({
      upColor: "#10b981", downColor: "#f43f5e",
      borderUpColor: "#10b981", borderDownColor: "#f43f5e",
      wickUpColor: "#10b981", wickDownColor: "#f43f5e",
    });

    volumeSeriesRef.current = chart.addHistogramSeries({
      color: "#3f3f46",
      priceFormat: { type: "volume" },
      priceScaleId: "volume",
    });
    chart.priceScale("volume").applyOptions({
      scaleMargins: { top: 0.85, bottom: 0 },
    });

    onChartReady?.(chart);

    return () => {
      onChartReady?.(null);
      chart.remove();
      chartRef.current = null;
      candleSeriesRef.current = null;
      volumeSeriesRef.current = null;
      smaSeriesRef.current = [];
      bbSeriesRef.current = [];
      sessionSeriesRef.current = {};
      runningRef.current = { current: null };
    };
    // onChartReady deliberately omitted — referencing a fresh callback on
    // every parent render would re-initialise the chart, which we never want.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── 2. Static data: candles, volume, indicators, session markers ─────
  useEffect(() => {
    const chart = chartRef.current;
    const candleSeries = candleSeriesRef.current;
    const volumeSeries = volumeSeriesRef.current;
    if (!chart || !candleSeries || !volumeSeries || marketData.length === 0) return;

    // Candles
    const chartCandles: ChartCandle[] = backendToChart(marketData.map(m => m.candle));
    candleSeries.setData(chartCandles.map(c => ({
      time: c.time as UTCTimestamp, open: c.open, high: c.high, low: c.low, close: c.close,
    })));
    runningRef.current = initRunning(chartCandles);

    // Volume — green when close >= open, red otherwise
    volumeSeries.setData(marketData.map(m => ({
      time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
      value: m.candle.volume,
      color: m.candle.close >= m.candle.open
        ? "rgba(16,185,129,0.4)" : "rgba(244,63,94,0.4)",
    })));

    // SMA overlays — dispose previous, recreate
    smaSeriesRef.current.forEach(s => chart.removeSeries(s));
    smaSeriesRef.current = [];
    const smaConfigs: Array<[keyof MarketData["indicators"], string]> = [
      ["sma_10", "#fbbf24"], ["sma_25", "#60a5fa"], ["sma_60", "#a78bfa"],
    ];
    for (const [field, color] of smaConfigs) {
      const s = chart.addLineSeries({ color, lineWidth: 1, priceLineVisible: false, lastValueVisible: false });
      s.setData(marketData
        .filter(m => Number.isFinite(m.indicators[field] as number) && (m.indicators[field] as number) > 0)
        .map(m => ({
          time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
          value: m.indicators[field] as number,
        })));
      smaSeriesRef.current.push(s);
    }

    // Bollinger Bands
    bbSeriesRef.current.forEach(s => chart.removeSeries(s));
    bbSeriesRef.current = [];
    const bbConfigs: Array<[keyof MarketData["indicators"], string]> = [
      ["bollinger_upper", "rgba(148,163,184,0.6)"],
      ["bollinger_middle", "rgba(148,163,184,0.4)"],
      ["bollinger_lower", "rgba(148,163,184,0.6)"],
    ];
    for (const [field, color] of bbConfigs) {
      const s = chart.addLineSeries({ color, lineWidth: 1, priceLineVisible: false, lastValueVisible: false });
      s.setData(marketData
        .filter(m => Number.isFinite(m.indicators[field] as number) && (m.indicators[field] as number) > 0)
        .map(m => ({
          time: isoToUtcSec(m.candle.timestamp) as UTCTimestamp,
          value: m.indicators[field] as number,
        })));
      bbSeriesRef.current.push(s);
    }

    // Session markers — one dummy LineSeries per session so we can toggle/
    // recolour individually. Markers go on each session's own series.
    Object.values(sessionSeriesRef.current).forEach(s => chart.removeSeries(s));
    sessionSeriesRef.current = {};

    const sessionIds = sessions.map(s => s.id).slice().sort((a, b) => a - b);
    for (const session of sessions) {
      const series = chart.addLineSeries({
        color: "rgba(0,0,0,0)",  // invisible line — markers only
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      sessionSeriesRef.current[session.id] = series;

      const trades = tradesBySession[session.id] ?? [];
      const color = colorFor(session.id, sessionIds);
      const markers: SeriesMarker<Time>[] = trades.map(t => ({
        time: isoToUtcSec(t.ts) as UTCTimestamp,
        position: t.side === "buy" ? "belowBar" : "aboveBar",
        color,
        shape: t.side === "buy" ? "arrowUp" : "arrowDown",
        text: t.is_real ? `${session.label}!` : session.label,
        size: t.is_real ? 2 : 1,
      }));
      // setMarkers requires non-empty data on the series. Plant a single
      // invisible point at the first marker's time so the markers attach.
      if (markers.length > 0) {
        series.setData(markers.map(m => ({ time: m.time, value: 0 })));
        series.setMarkers(markers);
      }
    }
  }, [marketData, sessions, tradesBySession]);

  // ── 3. Overlay toggles ───────────────────────────────────────────────
  useEffect(() => {
    smaSeriesRef.current.forEach(s => s.applyOptions({ visible: overlay.sma }));
    bbSeriesRef.current.forEach(s => s.applyOptions({ visible: overlay.bb }));
    if (volumeSeriesRef.current) {
      volumeSeriesRef.current.applyOptions({ visible: overlay.volume });
    }
  }, [overlay]);

  // ── 4. Live tick — update the running candle in place ────────────────
  useEffect(() => {
    if (!tick || !candleSeriesRef.current) return;
    const { updated } = applyTick(runningRef.current, tick.price, tick.ts_ms);
    candleSeriesRef.current.update({
      time: updated.time as UTCTimestamp,
      open: updated.open, high: updated.high, low: updated.low, close: updated.close,
    });
  }, [tick]);

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-xs">
        <ToggleChip label="SMA" on={overlay.sma} onChange={v => setOverlay(o => ({ ...o, sma: v }))} />
        <ToggleChip label="BB" on={overlay.bb} onChange={v => setOverlay(o => ({ ...o, bb: v }))} />
        <ToggleChip label="Volume" on={overlay.volume} onChange={v => setOverlay(o => ({ ...o, volume: v }))} />
      </div>
      <div ref={containerRef} className="w-full h-[420px] bg-[#0c0c0f] border border-[#1e1e26] rounded-xl" />
    </div>
  );
}

function ToggleChip({ label, on, onChange }: { label: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!on)}
      className={`px-2.5 py-1 rounded-md border text-xs ${
        on ? "bg-zinc-800 border-zinc-700 text-zinc-200" : "bg-transparent border-zinc-800 text-zinc-500"
      }`}
    >
      {label}
    </button>
  );
}
```

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공. 타입 에러가 나면 수정 후 재빌드.

- [ ] **Step 3: 커밋**

```bash
git add src/components/live/CandleChart.tsx
git commit -m "feat(live): CandleChart with SMA/BB/Volume + session markers + live ticks"
```

---

## Task 7: SignalLaneChart

**Files:**
- Create: `d:\SW\bitcoin-trader\src\components\live\SignalLaneChart.tsx`

`★ Layout:` 세션마다 가로 레인 하나씩. y축은 세션 인덱스(0,1,2,…). 각 trade는 그 레인 위 점 마커 (buy=원, sell=사각). 시간축은 CandleChart와 동기화.

- [ ] **Step 1: 컴포넌트 작성**

Create `src/components/live/SignalLaneChart.tsx`:

```tsx
import { useEffect, useRef } from "react";
import {
  createChart, ColorType,
  type IChartApi, type ISeriesApi, type SeriesMarker, type Time, type UTCTimestamp,
} from "lightweight-charts";
import type { LiveSession, LiveTrade } from "../../types";
import { isoToUtcSec } from "./charts/runningCandle";
import { colorFor } from "./charts/sessionPalette";

interface Props {
  sessions: LiveSession[];
  tradesBySession: Record<number, LiveTrade[]>;
  onChartReady?: (chart: IChartApi | null) => void;
}

export default function SignalLaneChart({ sessions, tradesBySession, onChartReady }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<Record<number, ISeriesApi<"Line">>>({});

  // Initialise chart once
  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "#0c0c0f" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "#1e1e26" },
        horzLines: { color: "transparent" },
      },
      timeScale: { borderColor: "#1e1e26", timeVisible: true, secondsVisible: false },
      rightPriceScale: { borderColor: "#1e1e26", visible: false },
      autoSize: true,
    });
    chartRef.current = chart;
    onChartReady?.(chart);
    return () => {
      onChartReady?.(null);
      chart.remove();
      chartRef.current = null;
      seriesRef.current = {};
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Re-render lanes whenever sessions or trades change
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;

    Object.values(seriesRef.current).forEach(s => chart.removeSeries(s));
    seriesRef.current = {};

    if (sessions.length === 0) return;

    const sessionIds = sessions.map(s => s.id).slice().sort((a, b) => a - b);

    // Each session occupies a y-band. We use lane index = i (1..N), with the
    // line itself invisible — only markers show. The price scale is hidden.
    sessions.forEach((session, idx) => {
      const lane = sessionIds.length - sessionIds.indexOf(session.id);  // top → bottom
      const series = chart.addLineSeries({
        color: "rgba(0,0,0,0)",
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      seriesRef.current[session.id] = series;

      const trades = tradesBySession[session.id] ?? [];
      if (trades.length === 0) return;

      const color = colorFor(session.id, sessionIds);
      const data = trades.map(t => ({
        time: isoToUtcSec(t.ts) as UTCTimestamp,
        value: lane,
      }));
      series.setData(data);

      const markers: SeriesMarker<Time>[] = trades.map(t => ({
        time: isoToUtcSec(t.ts) as UTCTimestamp,
        position: "inBar",
        color,
        shape: t.side === "buy" ? "circle" : "square",
        text: `${session.label}:${t.side}`,
        size: 1,
      }));
      series.setMarkers(markers);
    });
  }, [sessions, tradesBySession]);

  return (
    <div ref={containerRef} className="w-full h-[140px] bg-[#0c0c0f] border border-[#1e1e26] rounded-xl" />
  );
}

// `forEach` callback param `idx` is unused — kept above for clarity in case
// per-lane styling lands later. Strip if it triggers a lint warning.
```

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공.

- [ ] **Step 3: 커밋**

```bash
git add src/components/live/SignalLaneChart.tsx
git commit -m "feat(live): SignalLaneChart — per-session lanes with buy/sell markers"
```

---

## Task 8: LiveTradingPage 통합

**Files:**
- Modify: `d:\SW\bitcoin-trader\src\pages\LiveTradingPage.tsx`

`★ Goal:` 페이지 상단에 KPI 바, 그 아래 캔들차트 + 시그널 레인 차트(시간축 동기화), 그 아래 기존 프리셋 + 세션 테이블. 차트는 세션이 0개일 때도 KRW-ETH 시세를 그리지만, 세션 마커는 비어있다.

- [ ] **Step 1: 페이지 통합**

Read the existing `src/pages/LiveTradingPage.tsx` first to confirm the current structure (Phase 2 already added LiveKpiBar + ticks). 추가/변경 사항:

1. Import 추가:
```typescript
import CandleChart from "../components/live/CandleChart";
import SignalLaneChart from "../components/live/SignalLaneChart";
import { useChartSync } from "../components/live/charts/useChartSync";
import type { IChartApi } from "lightweight-charts";
```

2. 컴포넌트 본문에 chart instance state + 추가 store 필드 destructure:
```typescript
  const {
    sessions, presets, ticks,
    marketData, tradesBySession,
    refreshAll, createSession,
    startSession, stopSession, deleteSession, deletePreset,
    subscribeEvents,
    loadMarketData, loadAllSessionTrades,
  } = useLiveTradingStore();

  const [candleChart, setCandleChart] = useState<IChartApi | null>(null);
  const [laneChart, setLaneChart] = useState<IChartApi | null>(null);
  useChartSync(candleChart, laneChart);
```

`useEffect`에 차트 데이터 로드 추가 (기존 `refreshAll()` 호출 옆):
```typescript
  useEffect(() => {
    refreshAll().then(() => {
      loadMarketData();
      loadAllSessionTrades();
    });
    let unlisten: (() => void) | null = null;
    subscribeEvents().then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Refresh per-session trades whenever session list changes (id-set proxy).
  const sessionIdsKey = sessions.map(s => s.id).sort((a, b) => a - b).join(",");
  useEffect(() => {
    if (sessions.length > 0) loadAllSessionTrades();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionIdsKey]);
```

3. JSX 변경 — `LiveKpiBar` 다음에 chart 두 개 삽입:
```tsx
      <LiveKpiBar tick={ticks["KRW-ETH"]} />

      {marketData && marketData.length > 0 && (
        <Card>
          <CardHeader>
            <h3 className="text-sm font-semibold text-zinc-300">KRW-ETH (1h)</h3>
          </CardHeader>
          <CardContent>
            <CandleChart
              marketData={marketData}
              sessions={sessions}
              tradesBySession={tradesBySession}
              tick={ticks["KRW-ETH"]}
              onChartReady={setCandleChart}
            />
            <div className="mt-2">
              <SignalLaneChart
                sessions={sessions}
                tradesBySession={tradesBySession}
                onChartReady={setLaneChart}
              />
            </div>
          </CardContent>
        </Card>
      )}
```

(나머지 — 프리셋 카드, 세션 테이블, 다이얼로그 — 그대로 유지.)

- [ ] **Step 2: 빌드**

```bash
npm run vite:build
```

Expected: 성공.

- [ ] **Step 3: 커밋**

```bash
git add src/pages/LiveTradingPage.tsx
git commit -m "feat(live): integrate CandleChart + SignalLaneChart in LiveTradingPage"
```

---

## Task 9: 수동 smoke test

**Files:** (변경 없음 — 사용자 검증)

- [ ] **Step 1: 회귀 빌드/테스트**

From repo root `d:\SW\bitcoin-trader`:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
npm run vite:build
```

Expected: 모두 통과.

- [ ] **Step 2: 사용자에게 앱 실행 요청 (Claude는 직접 실행 안 함)**

```bash
npm run dev
```

- [ ] **Step 3: 체크리스트**

Live 페이지에서 확인:

- [ ] 페이지 마운트 시 캔들 차트가 KRW-ETH 봉으로 그려짐 (수천 개여야 정상)
- [ ] SMA/BB/Volume 토글 버튼 클릭 시 해당 시리즈가 즉시 보이고/숨겨짐
- [ ] 세션이 있으면 차트에 매수/매도 화살표 마커가 세션 색으로 표시됨
- [ ] 마지막 캔들이 틱마다 high/low/close가 갱신되는지 (close 라인이 미세하게 움직임)
- [ ] 매 정시(XX:00)가 지나면 새 봉이 추가되는지 (정확히 검증하려면 정시 무렵에 관찰)
- [ ] 시그널 레인 차트가 캔들 차트와 함께 가로 스크롤·줌됨 (한쪽만 끌어도 다른 쪽 따라옴)
- [ ] 세션 추가/삭제 시 마커가 즉시 갱신됨
- [ ] 세션 0개여도 차트는 그려지고, 에러 없이 동작

- [ ] **Step 4: 문제 발견 시 해당 커밋 수정. 정상이면 종료.**

---

## Definition of Done (Phase 3)

- [ ] `cargo test --no-default-features` 전체 통과 (Phase 1+2 기존 테스트)
- [ ] `npm run vite:build` 통과
- [ ] Smoke test 체크리스트(Task 9) 전부 OK
- [ ] 사용자가 KRW-ETH 캔들 차트 위에서 세션별 매수/매도 시점을 색으로 구분해 비교할 수 있음
- [ ] 시그널 레인 차트가 동일 시간축에서 세션별 거래 시점을 별도 레인으로 보여줌
- [ ] 진행 중인 시간봉이 틱 스트림으로 실시간 갱신됨

## Phase 4 Preview (본 플랜 범위 밖)

다음 플랜에서:
- 실전 승격 다이얼로그 + `real_started_at` 게이트
- 실전 라이프사이클 포지션 추적 (`is_real=1` 기반)
- 보안 follow-up: `127.0.0.1` 바인딩, CORS allowlist
- 로그 패널 (세션 필터 포함)
