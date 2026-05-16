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
  toggleSessionMode as apiToggleMode,
  emergencyStopAllReal as apiEmergencyStop,
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
  /// Sessions whose markers / signals should be hidden from the chart. An
  /// empty array means every session is visible — that's why we track the
  /// negative ("hidden") set: new sessions show up by default, no sync needed.
  hiddenSessionIds: number[];

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
  /// Promote (paper → real) or demote (real → paper). Backend enforces
  /// multi-real=1 and API key presence; errors propagate to the caller.
  toggleSessionMode: (id: number, mode: "paper" | "real", upbitAccountId?: number) => Promise<void>;
  /// Stop all running real sessions. Returns affected ids for the caller
  /// to surface in a toast.
  emergencyStopAllReal: () => Promise<number[]>;

  toggleSessionVisibility: (id: number) => void;
  setAllSessionsVisible: (visible: boolean) => void;

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
  hiddenSessionIds: [],

  loadMarketData: async () => {
    if (get().loadingMarketData) return;
    set({ loadingMarketData: true });
    try {
      // Best-effort: pull the latest hourly bars from Upbit so the chart
      // reflects current market state. Ignore failure (offline / rate-limit)
      // — we still render whatever's already in the local DB.
      try { await autoUpdateAllMarkets(); } catch { /* offline-safe */ }
      const data = await getMarketData("ETH", "hour");
      // No store-side trimming. The page's `rangeStart` is the single source of
      // truth for the visible window — LiveTradingPage filters by `cutoffMs`
      // and CandleChart re-fits timeScale when that window changes. Letting
      // legacy candles through here means the user can extend `From` further
      // back than 90 days and actually see those bars.
      set({ marketData: data });
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
    // Read-only: scheduler is the single writer of signal_log/trades. We
    // just paint whatever it most recently persisted. Fresh sessions show
    // empty until the next hourly cycle — accepted trade-off for removing
    // the dual-cycle race that produced duplicate trade rows.
    try {
      const cached = await getSessionSignalLog(sessionId);
      set((s) => ({
        signalsBySession: { ...s.signalsBySession, [sessionId]: cached },
      }));
    } catch (e) {
      console.warn("loadSessionSignals failed", sessionId, e);
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
    // Drop the deleted session from the hidden set so the array doesn't
    // accumulate stale ids over time.
    set((s) => ({ hiddenSessionIds: s.hiddenSessionIds.filter((x) => x !== id) }));
  },

  toggleSessionVisibility: (id) => {
    set((s) => ({
      hiddenSessionIds: s.hiddenSessionIds.includes(id)
        ? s.hiddenSessionIds.filter((x) => x !== id)
        : [...s.hiddenSessionIds, id],
    }));
  },

  setAllSessionsVisible: (visible) => {
    if (visible) {
      set({ hiddenSessionIds: [] });
    } else {
      set((s) => ({ hiddenSessionIds: s.sessions.map((x) => x.id) }));
    }
  },

  deletePreset: async (id) => {
    await apiDeletePreset(id);
    await get().refreshPresets();
  },

  toggleSessionMode: async (id, mode, upbitAccountId) => {
    await apiToggleMode(id, mode, upbitAccountId);
    await get().refreshSessions();
  },

  emergencyStopAllReal: async () => {
    const ids = await apiEmergencyStop();
    await get().refreshSessions();
    return ids;
  },

  subscribeEvents: async () => {
    const unsubs: Array<() => void> = [];

    // Tauri session events (same as before)
    if ("__TAURI_INTERNALS__" in window) {
      const { listen } = await import("@tauri-apps/api/event");
      const u1 = await listen<{ session_id?: number }>("session:update", (e) => {
        get().refreshSessions();
        // The hourly cycle has rolled in a fresh candle and a new signal_log
        // entry. Refresh marketData too so the strip can paint the new bar —
        // otherwise marketData stays frozen at mount time and signal
        // transitions occurring after mount have no candle slot to colour.
        get().loadMarketData();
        // Pull the freshly persisted trades so candle markers update in the
        // same paint. Scheduler is now the single writer (see loadSessionSignals
        // — refresh_session_cycle was removed to eliminate the dual-cycle
        // race that duplicated paper trade rows).
        get().loadAllSessionTrades();
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
/// baseEquity = "Start 시점 자본 × (1 + live_return)" — 사용자가 실제 라이브로
/// 운용한 결과 자본만 반영. (백엔드의 current_equity는 since~now 통합이라
/// baseline+live가 섞여 있어 화면에 그대로 쓰면 "Live=0%인데 Equity=+800%" 같은
/// 모순이 발생함. live_return 기반으로 통일해 모든 컬럼이 같은 시간축을 가짐.)
/// holding 중에는 baseEquity 전체가 코인으로 변환된 것으로 보고 가격 비율을 적용.
export function deriveSessionPnl(session: LiveSession, tick: TickData | undefined) {
  const baseEquity = session.initial_capital * (1 + (session.live_return ?? 0) / 100);
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
