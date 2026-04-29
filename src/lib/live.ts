import type {
  Preset,
  LiveSession,
  LiveTrade,
  SignalEvent,
  CreateSessionArgs,
  SavePresetArgs,
  TickData,
} from "../types";

const isTauri = "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke: tInvoke } = await import("@tauri-apps/api/core");
    return tInvoke<T>(cmd, args);
  }
  // Phase 1: desktop-only. PWA HTTP 경로는 Phase 2+에서 Axum 라우트와 함께 추가.
  throw new Error(`${cmd} is desktop-only in Phase 1`);
}

// ─── Presets ───
export const listPresets = (): Promise<Preset[]> => invoke("list_presets");
export const savePreset = (args: SavePresetArgs): Promise<number> =>
  invoke("save_preset", { args });
export const deletePreset = (id: number): Promise<void> =>
  invoke("delete_preset", { id });

// ─── Sessions ───
export const listSessions = (): Promise<LiveSession[]> => invoke("list_sessions");
export const createSession = (args: CreateSessionArgs): Promise<number> =>
  invoke("create_session", { args });
export const startSession = (id: number): Promise<void> =>
  invoke("start_session", { id });
export const stopSession = (id: number): Promise<void> =>
  invoke("stop_session", { id });
export const deleteSession = (id: number): Promise<void> =>
  invoke("delete_session", { id });
/// Promote a paper session to real (or demote back to paper). Backend enforces
/// multi-real=1 and verifies API keys are configured before promoting.
export const toggleSessionMode = (id: number, mode: "paper" | "real"): Promise<void> =>
  invoke("toggle_session_mode", { args: { id, mode } });
/// Stop all running real sessions. Returns the affected session ids.
/// Mode stays 'real' — only status flips to 'stopped'.
export const emergencyStopAllReal = (): Promise<number[]> =>
  invoke("emergency_stop_all_real");
export const listSessionTrades = (sessionId: number): Promise<LiveTrade[]> =>
  invoke("list_session_trades", { sessionId });
/// Persisted per-candle signal_log for the session — written by session_engine
/// each cycle so it always matches the trades in live_trades.
export const getSessionSignalLog = (sessionId: number): Promise<SignalEvent[]> =>
  invoke("get_session_signal_log", { sessionId });
/// On-demand cycle: re-runs run_session_cycle now (persists fresh trades +
/// signal_log + equity) and returns the signal_log. Used on page mount and
/// when the user picks a session in the strip dropdown so the chart always
/// reflects current state without waiting for the hourly scheduler.
export const refreshSessionCycle = (sessionId: number): Promise<SignalEvent[]> =>
  invoke("refresh_session_cycle", { sessionId });

// ─── Market Ticks ───
/// Subscribe to market ticks. Returns an unsubscribe function.
/// Tauri: uses the "market:tick" event emitted by the backend.
/// PWA: opens an EventSource to `/sse/market`.
export async function subscribeTicks(
  onTick: (t: TickData) => void
): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<TickData>("market:tick", (e) => onTick(e.payload));
    return () => {
      unlisten();
    };
  }
  // PWA fallback
  const es = new EventSource("/sse/market");
  es.addEventListener("tick", (ev) => {
    try {
      onTick(JSON.parse((ev as MessageEvent).data) as TickData);
    } catch {}
  });
  return () => {
    es.close();
  };
}
