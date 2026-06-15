import { describe, expect, it } from "vitest";
import { deriveActivity, RUNNING_STALE_MS } from "./deriveActivity";

describe("deriveActivity", () => {
  it("idle when nothing signals a run", () => {
    expect(deriveActivity({ streamRunning: false })).toEqual({
      activity: "idle",
      source: "none",
    });
  });

  it("trusts an authoritative session state (running)", () => {
    expect(
      deriveActivity({ sessionState: "running", streamRunning: false }),
    ).toEqual({ activity: "running", source: "status" });
  });

  it("surfaces paused / cancelling from the session state", () => {
    expect(deriveActivity({ sessionState: "paused", streamRunning: false }).activity).toBe(
      "paused",
    );
    expect(
      deriveActivity({ sessionState: "cancelling", streamRunning: false }).activity,
    ).toBe("cancelling");
  });

  it("a terminal session state does not count as active", () => {
    for (const s of ["finished", "failed", "cancelled", "queued", ""]) {
      expect(deriveActivity({ sessionState: s, streamRunning: false }).activity).toBe(
        "idle",
      );
    }
  });

  it("falls back to the live SSE buffer when the session row is absent", () => {
    expect(deriveActivity({ streamRunning: true })).toEqual({
      activity: "running",
      source: "stream",
    });
  });

  it("status wins over a stale stream signal", () => {
    expect(
      deriveActivity({ sessionState: "paused", streamRunning: true }).activity,
    ).toBe("paused");
  });

  it("detects an in-flight latest cycle from persisted events (no session, no SSE)", () => {
    // The exact case the operator hit: a CLI run with no IPC bridge and a tab
    // that joined after cycle_started — only the DB event log proves it's live.
    expect(
      deriveActivity({
        streamRunning: false,
        latestHasStarted: true,
        latestPhase: "eval",
        latestAgeMs: 30_000,
      }),
    ).toEqual({ activity: "running", source: "events" });
  });

  it("a finished latest cycle (phase=done) is idle, not running", () => {
    expect(
      deriveActivity({
        streamRunning: false,
        latestHasStarted: true,
        latestPhase: "done",
        latestAgeMs: 30_000,
      }).activity,
    ).toBe("idle");
  });

  it("an unfinished cycle with stale telemetry is treated as stalled (idle)", () => {
    expect(
      deriveActivity({
        streamRunning: false,
        latestHasStarted: true,
        latestPhase: "eval",
        latestAgeMs: RUNNING_STALE_MS + 1,
      }).activity,
    ).toBe("idle");
  });

  it("does not claim running from events when telemetry age is unknown", () => {
    // Conservative: without a freshness signal we never fabricate a running state.
    expect(
      deriveActivity({
        streamRunning: false,
        latestHasStarted: true,
        latestPhase: "eval",
        latestAgeMs: null,
      }).activity,
    ).toBe("idle");
  });

  it("an idle-phase latest cycle (no cycle_started yet) is idle", () => {
    expect(
      deriveActivity({
        streamRunning: false,
        latestHasStarted: false,
        latestPhase: "idle",
        latestAgeMs: 1_000,
      }).activity,
    ).toBe("idle");
  });
});
