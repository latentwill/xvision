import { describe, expect, it, vi, afterEach } from "vitest";

import { formatGateVerdict, getCycleRun, listLineageNodes, type CycleRunDetail } from "./api";
import * as client from "@/api/client";

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
