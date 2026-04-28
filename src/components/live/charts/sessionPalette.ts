// 캔들/매매 마커가 이미 점유한 hue를 피한 세션 식별 팔레트:
//   - 초록 (캔들 상승봉)
//   - 파랑 (매도 — 수익)
//   - 빨강 (캔들 하락봉 + 매도 — 손실)
// Tableau 10에서 위 영역과 충돌하는 5색(슬레이트블루/빨강/초록/청록/살구)을
// 빼고, ColorBrewer Dark2의 안전한 5색(진보라/마젠타/진주황/골드/진회색)으로
// 보충. 처음 5색은 채도 높은 hue로 동시 5~6 세션도 구분 가능.
const SESSION_PALETTE = [
  "#F28E2B", // 주황 (Tableau)
  "#EDC948", // 노랑 (Tableau)
  "#B07AA1", // 라일락 (Tableau)
  "#9C755F", // 갈색 (Tableau)
  "#7570B3", // 진보라 (Dark2)
  "#E7298A", // 마젠타 (Dark2)
  "#D95F02", // 진주황 (Dark2)
  "#A6761D", // 골드/올리브 (Dark2)
  "#BAB0AC", // 밝은 회색 (Tableau)
  "#666666", // 진회색 (Dark2)
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
  if (idx < 0) return SESSION_PALETTE[0];
  return SESSION_PALETTE[idx % SESSION_PALETTE.length];
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
