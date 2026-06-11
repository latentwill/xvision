import { describe, expect, it } from "vitest";
import { narrateEvent, normalizePersisted } from "./narrateEvent";

// Fixtures mirror the REAL wire shapes (progress.rs): flattened fields, "type" tag.
describe("narrateEvent", () => {
  it("narrates mutation_proposed with writer and hash", () => {
    const n = narrateEvent({
      type: "mutation_proposed",
      cycle_id: "c1",
      parent_hash: "ffff0000aa",
      child_hash: "abcd1234ef",
      mutator_model: "gemini-2.5-pro",
    });
    expect(n.sentence).toBe("Writer gemini-2.5-pro proposed an experiment → abcd1234");
    expect(n.tone).toBe("neutral");
    expect(n.hash).toBe("abcd1234ef");
  });

  it("narrates the three gate outcomes with delta", () => {
    const kept = narrateEvent({
      type: "mutation_gated",
      child_hash: "abcd1234ef",
      passed: true,
      outcome: "kept",
      delta_day: 0.21,
    });
    expect(kept.sentence).toBe("Gate passed abcd1234 · ΔSharpe +0.21 — kept");
    expect(kept.tone).toBe("kept");

    const dropped = narrateEvent({
      type: "mutation_gated",
      child_hash: "abcd1234ef",
      passed: false,
      outcome: "dropped",
      delta_day: -0.08,
    });
    expect(dropped.sentence).toBe("Gate failed abcd1234 · ΔSharpe −0.08 — rejected");
    expect(dropped.tone).toBe("rejected");

    const suspect = narrateEvent({
      type: "mutation_gated",
      child_hash: "abcd1234ef",
      passed: false,
      outcome: "suspect",
    });
    expect(suspect.sentence).toBe("Gate flagged abcd1234 — suspect");
    expect(suspect.tone).toBe("suspect");
  });

  it("narrates honesty_check_run with its message", () => {
    const n = narrateEvent({ type: "honesty_check_run", passed: true, message: "sabotage caught" });
    expect(n.sentence).toBe("Honesty check passed — sabotage caught");
    expect(n.tone).toBe("kept");
    expect(narrateEvent({ type: "honesty_check_run", passed: false, message: "" }).tone).toBe(
      "suspect",
    );
  });

  it("narrates the remaining kinds", () => {
    expect(
      narrateEvent({ type: "cycle_started", cycle_id: "cyc-7f3a", parent_count: 3 }).sentence,
    ).toBe("Cycle cyc-7f3a started · 3 parents");

    expect(
      narrateEvent({
        type: "cycle_finished",
        active_count: 2,
        suspect_count: 1,
        rejected_count: 11,
      }).sentence,
    ).toBe("Cycle finished — 2 kept · 1 suspect · 11 rejected");

    expect(
      narrateEvent({ type: "parent_selected", parent_hash: "abcd1234ef" }).sentence,
    ).toBe("Parent selected: abcd1234");

    expect(
      narrateEvent({ type: "no_candidate", parent_hash: "abcd1234ef", reason: "identity diff" })
        .tone,
    ).toBe("warn");

    expect(
      narrateEvent({
        type: "judge_finding",
        child_hash: "abcd1234ef",
        severity: "warn",
        code: "lookahead",
      }).tone,
    ).toBe("warn");

    expect(
      narrateEvent({ type: "phase_started", phase: "eval", detail: "backtesting" }).tone,
    ).toBe("neutral");
  });

  it("falls back gracefully on unknown kinds", () => {
    const n = narrateEvent({ type: "future_event" });
    expect(n.sentence).toBe("future_event");
    expect(n.tone).toBe("neutral");
  });
});

describe("normalizePersisted", () => {
  it("parses payload_json (the full serialized event) and maps 3-way gated kinds", () => {
    const e = normalizePersisted({
      seq: 1,
      session_id: "s",
      cycle_id: "c",
      ts: "t",
      kind: "mutation_gated_passed",
      payload_json:
        '{"type":"mutation_gated","child_hash":"abcd1234ef","passed":true,"outcome":"kept","delta_day":0.21}',
    });
    expect(e.type).toBe("mutation_gated");
    expect(e.child_hash).toBe("abcd1234ef");
    expect(e.ts).toBe("t"); // row ts wins so the feed has times
  });

  it("falls back gracefully when payload_json is invalid JSON", () => {
    const e = normalizePersisted({
      seq: 2,
      session_id: "s",
      cycle_id: "c",
      ts: "t2",
      kind: "cycle_started",
      payload_json: "not-json",
    });
    expect(e.type).toBe("cycle_started");
    expect(e.ts).toBe("t2");
  });

  it("maps mutation_gated_suspect and mutation_gated_dropped kinds", () => {
    const suspect = normalizePersisted({
      seq: 3,
      session_id: "s",
      cycle_id: "c",
      ts: "t3",
      kind: "mutation_gated_suspect",
      payload_json: '{"type":"mutation_gated","child_hash":"abc","passed":false,"outcome":"suspect"}',
    });
    expect(suspect.type).toBe("mutation_gated");

    const dropped = normalizePersisted({
      seq: 4,
      session_id: "s",
      cycle_id: "c",
      ts: "t4",
      kind: "mutation_gated_dropped",
      payload_json: '{"type":"mutation_gated","child_hash":"abc","passed":false,"outcome":"dropped"}',
    });
    expect(dropped.type).toBe("mutation_gated");
  });
});
