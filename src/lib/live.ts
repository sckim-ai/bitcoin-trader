import type {
  Preset,
  LiveSession,
  LiveTrade,
  CreateSessionArgs,
  SavePresetArgs,
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
