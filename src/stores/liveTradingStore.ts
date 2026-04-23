import { create } from "zustand";
import type { LiveSession, LiveTrade, Preset } from "../types";
import {
  listPresets,
  listSessions,
  listSessionTrades,
  createSession as apiCreateSession,
  startSession as apiStart,
  stopSession as apiStop,
  deleteSession as apiDelete,
  createPreset as apiCreatePreset,
  createDefaultPreset as apiCreateDefaultPreset,
} from "../lib/live";

interface LiveTradingState {
  sessions: LiveSession[];
  presets: Preset[];
  tradesBySession: Record<number, LiveTrade[]>;
  loading: boolean;

  refreshAll: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  refreshPresets: () => Promise<void>;
  refreshTrades: (sessionId: number) => Promise<void>;

  createSession: (args: Parameters<typeof apiCreateSession>[0]) => Promise<void>;
  createPreset: (args: Parameters<typeof apiCreatePreset>[0]) => Promise<void>;
  createDefaultPreset: (name: string, strategyKey: string) => Promise<void>;
  startSession: (id: number) => Promise<void>;
  stopSession: (id: number) => Promise<void>;
  deleteSession: (id: number) => Promise<void>;

  subscribeEvents: () => Promise<() => void>;
}

export const useLiveTradingStore = create<LiveTradingState>((set, get) => ({
  sessions: [],
  presets: [],
  tradesBySession: {},
  loading: false,

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

  createPreset: async (args) => {
    await apiCreatePreset(args);
    await get().refreshPresets();
  },

  createDefaultPreset: async (name, strategyKey) => {
    await apiCreateDefaultPreset(name, strategyKey);
    await get().refreshPresets();
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

  subscribeEvents: async () => {
    if (!("__TAURI_INTERNALS__" in window)) return () => {};
    const { listen } = await import("@tauri-apps/api/event");

    const u1 = await listen("session:update", () => {
      get().refreshSessions();
    });
    const u2 = await listen<{ session_id: number }>("session:log", (e) => {
      console.debug("session:log", e.payload);
    });

    return () => { u1(); u2(); };
  },
}));
