// frontend/web/src/features/agent-runs/span-colors.test.ts
//
// WS-17 span-taxonomy rename: `decision.model` / `decision.reasoning`
// must map to first-class colour categories so the trace dock reads the
// decision-producing model call (and its chain-of-thought) as what they
// are, not as a generic supervisor catch-all. The retired `model.call` /
// `model.reasoning` wire values stay recognised as legacy aliases.

import { describe, expect, test } from "vitest";
import type { RunSpan, SpanKind } from "@/api/types-agent-runs";
import {
  CATEGORY_STYLES,
  categoryOf,
  spanColor,
  optiGateColor,
  spanColorForSpan,
} from "./span-colors";

describe("categoryOf — WS-17 decision span taxonomy", () => {
  test("decision.model maps to the model category (blue MODEL swatch)", () => {
    expect(categoryOf("decision.model")).toBe("model");
    expect(spanColor("decision.model").label).toBe("MODEL");
  });

  test("decision.reasoning maps to its own reasoning category", () => {
    expect(categoryOf("decision.reasoning")).toBe("reasoning");
    expect(spanColor("decision.reasoning").label).toBe("REASN");
  });

  test("legacy model.call / model.reasoning still resolve (back-compat)", () => {
    // Historical exports / older recorded rows pre-date the rename.
    expect(categoryOf("model.call" as SpanKind)).toBe("model");
    expect(categoryOf("model.reasoning" as SpanKind)).toBe("reasoning");
  });

  test("agent.decision keeps its first-class decision swatch", () => {
    expect(categoryOf("agent.decision")).toBe("decision");
    expect(spanColor("agent.decision").label).toBe("DECDE");
  });

  test("every category has a defined style", () => {
    for (const cat of [
      "agent",
      "decision",
      "model",
      "reasoning",
      "tool",
      "broker",
      "supervisor",
      "artifact",
      "opti_cycle",
      "opti_phase",
      "opti_kept",
      "opti_suspect",
      "opti_rejected",
      "opti_eval_run",
    ] as const) {
      expect(CATEGORY_STYLES[cat]).toBeDefined();
      expect(CATEGORY_STYLES[cat].hex).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });
});

describe("categoryOf — WS-11a OPTI span taxonomy", () => {
  test("opti.cycle maps to the cycle category", () => {
    expect(categoryOf("opti.cycle")).toBe("opti_cycle");
  });

  test("opti.parent / opti.experiment / opti.honesty / opti.flywheel map to the phase category", () => {
    expect(categoryOf("opti.parent")).toBe("opti_phase");
    expect(categoryOf("opti.experiment")).toBe("opti_phase");
    expect(categoryOf("opti.honesty")).toBe("opti_phase");
    expect(categoryOf("opti.flywheel")).toBe("opti_phase");
  });

  test("opti.eval-run (WS-11b) maps to its own teal eval-run category", () => {
    expect(categoryOf("opti.eval-run")).toBe("opti_eval_run");
    // Distinct swatch from the generic phase tint so the drill-link node stands out.
    expect(spanColor("opti.eval-run").hex).not.toBe(spanColor("opti.experiment").hex);
  });

  test("opti.gate resolves a three-way tone by outcome (Active/Suspect/Rejected)", () => {
    // kept = positive, suspect = warn, rejected = muted. The gate kind alone
    // is ambiguous, so the outcome attribute drives the swatch.
    // 5-char operator swatch labels (Active / Suspect / Rejected, abbreviated).
    expect(optiGateColor("kept").label).toMatch(/ACTIV/i);
    expect(optiGateColor("suspect").label).toMatch(/SUSP/i);
    expect(optiGateColor("rejected").label).toMatch(/REJ/i);
    // tones differ across the three outcomes
    expect(optiGateColor("kept").hex).not.toBe(optiGateColor("rejected").hex);
    expect(optiGateColor("suspect").hex).not.toBe(optiGateColor("kept").hex);
  });

  test("spanColorForSpan tints an opti.gate row off its outcome attribute", () => {
    const keptGate = {
      kind: "opti.gate",
      attributes: { outcome: "kept" },
    } as unknown as RunSpan;
    const rejGate = {
      kind: "opti.gate",
      attributes: { outcome: "rejected" },
    } as unknown as RunSpan;
    expect(spanColorForSpan(keptGate).hex).toBe(optiGateColor("kept").hex);
    expect(spanColorForSpan(rejGate).hex).toBe(optiGateColor("rejected").hex);
  });

  test("spanColorForSpan falls back to kind-based color for non-gate spans", () => {
    const exp = {
      kind: "opti.experiment",
      attributes: {},
    } as unknown as RunSpan;
    expect(spanColorForSpan(exp).hex).toBe(spanColor("opti.experiment").hex);
  });
});
