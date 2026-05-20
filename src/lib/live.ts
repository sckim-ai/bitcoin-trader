import type {
  Preset,
  LiveSession,
  LiveTrade,
  SignalEvent,
  CreateSessionArgs,
  SavePresetArgs,
  TickData,
  UpbitAccount,
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
/// Update or clear a session's BUY cap (KRW). Pass null to clear (full balance).
export const setSessionOrderCap = (id: number, max_order_krw: number | null): Promise<void> =>
  invoke("set_session_order_cap", { id, maxOrderKrw: max_order_krw });
export const startSession = (id: number): Promise<void> =>
  invoke("start_session", { id });
export const stopSession = (id: number): Promise<void> =>
  invoke("stop_session", { id });
export const deleteSession = (id: number): Promise<void> =>
  invoke("delete_session", { id });
/// Promote a paper session to real (or demote back to paper).
///   - real 승급: `upbitAccountId` 인자 또는 세션에 이미 연결된 계정 중 하나가 필수.
///     백엔드가 partial unique index로 계정당 1 running real을 강제.
///   - paper 복귀: 계정 인자 무시되며, 세션의 기존 계정 연결은 보존된다.
export const toggleSessionMode = (
  id: number,
  mode: "paper" | "real",
  upbitAccountId?: number,
): Promise<void> =>
  invoke("toggle_session_mode", {
    args: { id, mode, upbit_account_id: upbitAccountId ?? null },
  });
/// Stop all running real sessions. Returns the affected session ids.
/// Mode stays 'real' — only status flips to 'stopped'.
export const emergencyStopAllReal = (): Promise<number[]> =>
  invoke("emergency_stop_all_real");

export interface PendingOrderRow {
  uuid: string;
  session_id: number;
  side: "bid" | "ask";
  market: string;
  ord_type: string;
  target_price: number | null;
  requested: number;
  placed_at: string;
  last_checked: string | null;
}

/// Wait-state pending orders across all sessions. Empty in steady state.
export const listPendingOrders = (): Promise<PendingOrderRow[]> =>
  invoke("list_pending_orders");
export const listSessionTrades = (sessionId: number): Promise<LiveTrade[]> =>
  invoke("list_session_trades", { sessionId });
/// Persisted per-candle signal_log for the session — written by session_engine
/// each cycle so it always matches the trades in live_trades.
export const getSessionSignalLog = (sessionId: number): Promise<SignalEvent[]> =>
  invoke("get_session_signal_log", { sessionId });

// ─── Upbit Accounts ───
export const listUpbitAccounts = (): Promise<UpbitAccount[]> =>
  invoke("list_upbit_accounts");

export interface AddAccountArgs {
  label: string;
  access_key: string;
  secret_key: string;
  /** 비워두면 Settings의 글로벌 Discord webhook이 fallback. */
  discord_webhook_url?: string;
}
export const addUpbitAccount = (args: AddAccountArgs): Promise<UpbitAccount> =>
  invoke("add_upbit_account", { args });

export interface UpdateAccountArgs {
  id: number;
  label?: string;
  access_key?: string;
  secret_key?: string;
  /** undefined=변경 없음, 빈 문자열="" = 글로벌 fallback으로 복귀, 문자열=교체. */
  discord_webhook_url?: string;
}
export const updateUpbitAccount = (args: UpdateAccountArgs): Promise<void> =>
  invoke("update_upbit_account", { args });

export const deleteUpbitAccount = (id: number): Promise<void> =>
  invoke("delete_upbit_account", { id });

export const setUpbitAccountEnabled = (id: number, enabled: boolean): Promise<void> =>
  invoke("set_upbit_account_enabled", { id, enabled });

/** Returns the number of currencies the account holds — proves keys work. */
export const testUpbitAccountConnection = (id: number): Promise<number> =>
  invoke("test_upbit_account_connection", { id });

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
