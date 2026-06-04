import { describe, expect, it } from "vitest";

import { formatGateVerdict } from "./api";

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
