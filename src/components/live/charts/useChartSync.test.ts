import { describe, it, expect } from "vitest";
import { shouldPropagate, recordSync, type SyncState } from "./useChartSync";

describe("chart sync state machine", () => {
  it("propagates the very first event when no prior sync recorded", () => {
    const state: SyncState = { lastSync: null };
    expect(shouldPropagate(state, { from: 0, to: 100 })).toBe(true);
  });

  it("skips an echo event matching the last sync exactly", () => {
    const state: SyncState = { lastSync: null };
    recordSync(state, { from: 10, to: 50 });
    expect(shouldPropagate(state, { from: 10, to: 50 })).toBe(false);
  });

  it("propagates when zooming further out (to widens)", () => {
    const state: SyncState = { lastSync: null };
    recordSync(state, { from: 100, to: 168 });
    // wheel-zoom-out: visible logical range expands
    expect(shouldPropagate(state, { from: 50, to: 168 })).toBe(true);
  });

  it("propagates when zooming further in (range narrows)", () => {
    const state: SyncState = { lastSync: null };
    recordSync(state, { from: 0, to: 168 });
    expect(shouldPropagate(state, { from: 60, to: 90 })).toBe(true);
  });

  it("skips floating-point echo within tolerance (e.g. lightweight-charts wobble)", () => {
    const state: SyncState = { lastSync: null };
    recordSync(state, { from: 10.0, to: 50.0 });
    expect(shouldPropagate(state, { from: 10.0000001, to: 50.0000001 })).toBe(false);
  });

  it("does NOT skip when only one side moves (asymmetric pan)", () => {
    const state: SyncState = { lastSync: null };
    recordSync(state, { from: 10, to: 50 });
    // only `to` changed (drag right edge)
    expect(shouldPropagate(state, { from: 10, to: 60 })).toBe(true);
  });

  it("supports continuous zoom-out beyond the previously recorded range", () => {
    // Simulates the user wheel-zooming out repeatedly. Each step should
    // propagate to the partner; a stale record must never block widening.
    const state: SyncState = { lastSync: null };

    const stepsFromUserWheel = [
      { from: 100, to: 168 },
      { from:  80, to: 168 },
      { from:  40, to: 168 },
      { from: -20, to: 168 },   // zoom past the data left edge
      { from: -50, to: 200 },   // also past right edge
    ];
    for (const step of stepsFromUserWheel) {
      expect(shouldPropagate(state, step)).toBe(true);
      recordSync(state, step);
    }
  });

  it("simulates one full propagate cycle: A→B then B's echo back to A is skipped", () => {
    const state: SyncState = { lastSync: null };

    // A handler decides to propagate to B
    const userZoomOnA = { from: 50, to: 100 };
    expect(shouldPropagate(state, userZoomOnA)).toBe(true);
    recordSync(state, userZoomOnA);
    // ...we call setVisibleLogicalRange on B...
    // B then echoes the same range back to A's handler:
    const echoFromB = { from: 50, to: 100 };
    expect(shouldPropagate(state, echoFromB)).toBe(false);
  });
});
