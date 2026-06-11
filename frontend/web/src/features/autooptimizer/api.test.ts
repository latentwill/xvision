import { describe, expect, it, vi, afterEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";

import {
  formatGateVerdict,
  getCycleRun,
  listLineageNodes,
  useCycleEvents,
  useRiver,
  type CycleRunDetail,
} from "./api";
import * as client from "@/api/client";
import { makeClient } from "./test-utils";

// ─── Hook test helpers ────────────────────────────────────────────────────────

function makeWrapper() {
  const queryClient = makeClient();
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

afterEach(() => vi.restoreAllMocks());

describe("autooptimizer api additions", () => {
  it("getCycleRun fetches the per-cycle detail endpoint", async () => {
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({ cycle_id: "cyc-1", nodes: [] } as unknown as CycleRunDetail);
    await getCycleRun("cyc 1");
    expect(spy).toHaveBeenCalledWith(
      "/api/autooptimizer/cycles/cyc%201",
    );
  });

  it("listLineageNodes forwards a cycle_id filter", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    await listLineageNodes({ cycleId: "cyc-1" });
    expect(spy).toHaveBeenCalledWith(
      "/api/autooptimizer/lineage?cycle_id=cyc-1",
    );
  });
});

// Regression: rejected lineage nodes serialize `gate_verdict` as the Rust
// externally-tagged enum object `{ Fail: { reason } }`, not a string. The old
// formatter called `.toLowerCase()` on it and threw, blanking the Genealogy /
// Provenance tabs. The formatter must accept every shape without crashing.
describe("formatGateVerdict", () => {
  it("handles the Pass string and passed DB form", () => {
    expect(formatGateVerdict("Pass")).toBe("Accepted");
    expect(formatGateVerdict("passed")).toBe("Accepted");
  });

  it("handles the Fail object form without crashing", () => {
    expect(formatGateVerdict({ Fail: { reason: "inversion-pair symmetric noise" } })).toBe(
      "Rejected",
    );
    expect(formatGateVerdict({ Pass: null })).toBe("Accepted");
  });

  it("handles the rejected:<reason> DB string prefix", () => {
    expect(formatGateVerdict("rejected:min-improvement not met")).toBe("Rejected");
    expect(formatGateVerdict("fail:overfit")).toBe("Rejected");
  });

  it("maps quarantined to Suspect and null to Pending", () => {
    expect(formatGateVerdict("quarantined")).toBe("Suspect");
    expect(formatGateVerdict(null)).toBe("Pending");
    expect(formatGateVerdict(undefined)).toBe("Pending");
  });

  it("never throws on an unexpected shape", () => {
    expect(() => formatGateVerdict(42 as unknown)).not.toThrow();
    expect(() => formatGateVerdict({ weird: true } as unknown)).not.toThrow();
  });
});

// ─── useCycleEvents ───────────────────────────────────────────────────────────

describe("useCycleEvents", () => {
  it("fetches persisted events for a cycle", async () => {
    const fixture = [
      {
        seq: 1,
        session_id: "s",
        cycle_id: "cyc-1",
        kind: "cycle_started",
        payload_json: "{}",
        ts: "2026-06-11T00:00:00Z",
      },
    ];
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue(fixture);
    const wrapper = makeWrapper();
    const { result } = renderHook(() => useCycleEvents("cyc-1"), { wrapper });
    await waitFor(() => expect(result.current.data).toHaveLength(1));
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining("/api/autooptimizer/cycles/cyc-1/events"),
    );
  });

  it("is disabled (idle) without a cycle id", () => {
    const wrapper = makeWrapper();
    const { result } = renderHook(() => useCycleEvents(null), { wrapper });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

// ─── useRiver ────────────────────────────────────────────────────────────────

describe("useRiver", () => {
  it("fetches river nodes", async () => {
    const fixture = [
      {
        bundle_hash: "h",
        parent_hash: null,
        cycle_id: "c",
        status: "active",
        created_at: "t",
        child_day_score: 1.2,
        delta_day: 0.1,
      },
    ];
    vi.spyOn(client, "apiFetch").mockResolvedValue(fixture);
    const wrapper = makeWrapper();
    const { result } = renderHook(() => useRiver(), { wrapper });
    await waitFor(() => expect(result.current.data?.[0].bundle_hash).toBe("h"));
  });

  it("surfaces isError without retry on older backends (404)", async () => {
    vi.spyOn(client, "apiFetch").mockRejectedValue(new Error("404 Not Found"));
    const wrapper = makeWrapper();
    const { result } = renderHook(() => useRiver(), { wrapper });
    await waitFor(() => expect(result.current.isError).toBe(true));
  });
});
