import { create } from "zustand";
import type { LiveSession, LiveTrade, MarketData, Preset, SignalEvent, TickData } from "../types";
import {
  listPresets,
  listSessions,
  listSessionTrades,
  getSessionSignalLog,
  createSession as apiCreateSession,
  startSession as apiStart,
  stopSession as apiStop,
  deleteSession as apiDelete,
  deletePreset as apiDeletePreset,
  subscribeTicks,
} from "../lib/live";
import { getMarketData, autoUpdateAllMarkets } from "../lib/api";

interface LiveTradingState {
  sessions: LiveSession[];
  presets: Preset[];
  tradesBySession: Record<number, LiveTrade[]>;
  /// Market-keyed most-recent tick snapshot. Updated by the tick subscription.
  ticks: Record<string, TickData>;
  loading: boolean;
  marketData: MarketData[] | null;
  loadingMarketData: boolean;
  /// Per-candle signal log derived by replaying a session's preset on the
  /// visible window. Keyed by session.id.
  signalsBySession: Record<number, SignalEvent[]>;

  refreshAll: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  refreshPresets: () => Promise<void>;
  refreshTrades: (sessionId: number) => Promise<void>;
  loadMarketData: () => Promise<void>;
  loadAllSessionTrades: () => Promise<void>;
  loadSessionSignals: (sessionId: number, since: string) => Promise<void>;

  createSession: (args: Parameters<typeof apiCreateSession>[0]) => Promise<void>;
  startSession: (id: number) => Promise<void>;
  stopSession: (id: number) => Promise<void>;
  deleteSession: (id: number) => Promise<void>;
  deletePreset: (id: number) => Promise<void>;

  subscribeEvents: () => Promise<() => void>;
}

export const useLiveTradingStore = create<LiveTradingState>((set, get) => ({
  sessions: [],
  presets: [],
  tradesBySession: {},
  ticks: {},
  loading: false,
  marketData: null,
  loadingMarketData: false,
  signalsBySession: {},

  loadMarketData: async () => {
    if (get().loadingMarketData) return;
    set({ loadingMarketData: true });
    try {
      // Best-effort: pull the latest hourly bars from Upbit so the chart
      // reflects current market state. Ignore failure (offline / rate-limit)
      // — we still render whatever's already in the local DB.
      try { await autoUpdateAllMarkets(); } catch { /* offline-safe */ }
      const data = await getMarketData("ETH", "hour");
      // Trim to the last 90 days. The DB may carry legacy history (e.g.
      // CSV-imported 2019–2020 candles) that's irrelevant for the live chart
      // and would otherwise stretch the auto-fit range away from "now".
      const cutoffMs = Date.now() - 90 * 24 * 60 * 60 * 1000;
      const recent = data.filter((m) => new Date(m.candle.timestamp).getTime() >= cutoffMs);
      set({ marketData: recent });
    } finally {
      set({ loadingMarketData: false });
    }
  },

  loadAllSessionTrades: async () => {
    const ids = get().sessions.map((s) => s.id);
    const results = await Promise.all(
      ids.map(async (id) => [id, await listSessionTrades(id)] as const),
    );
    const map: Record<number, LiveTrade[]> = {};
    for (const [id, trades] of results) map[id] = trades;
    set({ tradesBySession: map });
  },

  loadSessionSignals: async (sessionId, _since) => {
    // Read the signal_log persisted by session_engine on its last cycle.
    // Same simulation that produced the trades in live_trades — guaranteed
    // alignment, no frontend-side re-simulation, no data-window mismatch.
    try {
      const signals = await getSessionSignalLog(sessionId);
      set((s) => ({
        signalsBySession: { ...s.signalsBySession, [sessionId]: signals },
      }));
    } catch (e) {
      console.error("loadSessionSignals failed", sessionId, e);
    }
  },

  refreshAll: async () => {
    set({ loading: true });
    try {
      await Promise.all([get().refreshSessions(), get().refreshPresets()]);
    } finally {
      set({ loading: false });
    }
  },

  refreshSessions: async () => {
    const sessions = await listSessions();
    set({ sessions });
  },

  refreshPresets: async () => {
    const presets = await listPresets();
    set({ presets });
  },

  refreshTrades: async (sessionId) => {
    const trades = await listSessionTrades(sessionId);
    set((s) => ({ tradesBySession: { ...s.tradesBySession, [sessionId]: trades } }));
  },

  createSession: async (args) => {
    await apiCreateSession(args);
    await get().refreshSessions();
  },

  startSession: async (id) => {
    await apiStart(id);
    await get().refreshSessions();
  },

  stopSession: async (id) => {
    await apiStop(id);
    await get().refreshSessions();
  },

  deleteSession: async (id) => {
    await apiDelete(id);
    await get().refreshSessions();
  },

  deletePreset: async (id) => {
    await apiDeletePreset(id);
    await get().refreshPresets();
  },

  subscribeEvents: async () => {
    const unsubs: Array<() => void> = [];

    // Tauri session events (same as before)
    if ("__TAURI_INTERNALS__" in window) {
      const { listen } = await import("@tauri-apps/api/event");
      const u1 = await listen<{ session_id?: number }>("session:update", (e) => {
        get().refreshSessions();
        // After the engine cycle persists a fresh signal_log, refresh any
        // session whose strip is currently being viewed.
        const sid = e.payload?.session_id;
        if (sid != null) get().loadSessionSignals(sid, "");
      });
      const u2 = await listen<{ session_id: number }>("session:log", (e) => {
        console.debug("session:log", e.payload);
      });
      unsubs.push(u1, u2);
    }

    // Market ticks (Tauri event OR SSE fallback)
    const unsubTicks = await subscribeTicks((tick) => {
      set((s) => ({ ticks: { ...s.ticks, [tick.market]: tick } }));
    });
    unsubs.push(unsubTicks);

    return () => { unsubs.forEach((fn) => fn()); };
  },
}));

/// Derive current equity and unrealized P/L for a session using the latest tick.
/// holding 중에는 baseEquity 전체가 코인으로 변환된 것으로 보고 가격 비율을 적용한다.
/// (이전 구현은 buy_volume을 곱해 미실현을 계산했지만, 전략 내부 set_volume이
///  비현실적인 값을 만들 때 화면에서 폭주하는 통로가 됐음 — 비율 기반은 그 통로를 차단.)
export function deriveSessionPnl(session: LiveSession, tick: TickData | undefined) {
  const baseEquity = session.current_equity ?? session.initial_capital;
  if (session.current_position !== "holding" || tick == null
      || session.current_buy_price == null || session.current_buy_price <= 0) {
    return {
      currentEquity: baseEquity,
      unrealizedPnl: 0,
      unrealizedPnlPct: 0,
      pnlPctSinceStart: baseEquity / session.initial_capital * 100 - 100,
    };
  }
  const ratio = tick.price / session.current_buy_price;
  const currentEquity = baseEquity * ratio;
  return {
    currentEquity,
    unrealizedPnl: baseEquity * (ratio - 1),
    unrealizedPnlPct: (ratio - 1) * 100,
    pnlPctSinceStart: currentEquity / session.initial_capital * 100 - 100,
  };
}
