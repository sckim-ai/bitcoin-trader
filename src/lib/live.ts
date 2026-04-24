import type {
  Preset,
  LiveSession,
  LiveTrade,
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
export const listSessionTrades = (sessionId: number): Promise<LiveTrade[]> =>
  invoke("list_session_trades", { sessionId });

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
