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
