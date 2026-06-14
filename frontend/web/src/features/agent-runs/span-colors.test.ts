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
  categoryOfSpan,
  isKnownSpanKind,
  optiGateColor,
  spanColor,
  spanColorForSpan,
} from "./span-colors";

function span(over: Partial<RunSpan> = {}): RunSpan {
  return {
    span_id: "s_1",
    parent_span_id: null,
    name: "x",
    kind: "agent.run",
    started_at: "2026-06-14T10:00:00.000Z",
    finished_at: "2026-06-14T10:00:01.000Z",
    status: "ok",
    attributes: {},
    ...over,
  };
}

// Every SpanKind in the union must resolve to a non-fallback category +
// color. WS-8 render parity: nothing in the taxonomy may silently fall into
// the unknown bucket.
const ALL_SPAN_KINDS: SpanKind[] = [
  "agent.run",
  "agent.plan",
  "agent.decision",
  "decision.model",
  "decision.reasoning",
  "model.call",
  "model.reasoning",
  "tool.call",
  "tool.validate_input",
  "tool.validate_output",
  "approval.request",
  "approval.response",
  "sandbox.exec",
  "supervisor.review",
  "financial.eval",
  "artifact.write",
  "ipc.notification",
  "skill.invoke",
  "broker.call",
  "recovery.attempt",
  "state.transition",
  "opti.cycle",
  "opti.parent",
  "opti.experiment",
  "opti.gate",
  "opti.honesty",
  "opti.judge",
  "opti.flywheel",
  "engine.event",
];

describe("span taxonomy completeness — WS-8 render parity", () => {
  test("every known SpanKind resolves to a non-unknown category", () => {
    for (const kind of ALL_SPAN_KINDS) {
      // engine.event resolves per-span (its family lives in attributes), so
      // it's allowed to read as `unknown` on a bare kind lookup; assert it's
      // recognised by the predicate instead.
      expect(isKnownSpanKind(kind), `SpanKind "${kind}" is not recognised`).toBe(true);
      if (kind !== "engine.event") {
        expect(
          categoryOf(kind),
          `SpanKind "${kind}" fell into the unknown category`,
        ).not.toBe("unknown");
      }
    }
  });

  test("every known SpanKind has a defined color + label", () => {
    for (const kind of ALL_SPAN_KINDS) {
      const style = spanColor(kind);
      expect(style.hex, `SpanKind "${kind}" has no hex`).toMatch(/^#[0-9a-fA-F]{6}$/);
      expect(style.label.length, `SpanKind "${kind}" has empty label`).toBeGreaterThan(0);
    }
  });

  test("an unknown SpanKind resolves to the typed unknown fallback, not a mislabel", () => {
    const cat = categoryOf("future.unforeseen.kind" as SpanKind);
    expect(cat).toBe("unknown");
    expect(isKnownSpanKind("future.unforeseen.kind" as SpanKind)).toBe(false);
    const style = spanColor("future.unforeseen.kind" as SpanKind);
    expect(style.hex).toMatch(/^#[0-9a-fA-F]{6}$/);
    expect(style.label.length).toBeGreaterThan(0);
  });

  test("engine.event span derives its color/label from the engine-event family", () => {
    const riskRow = span({
      kind: "engine.event",
      attributes: { engine_event_kind: "risk_veto" },
    });
    // Risk family is distinct from a model/decision span.
    expect(categoryOfSpan(riskRow)).not.toBe("unknown");
    expect(spanColorForSpan(riskRow).label).toBe("RISK");

    const orderRow = span({
      kind: "engine.event",
      attributes: { engine_event_kind: "order_signed" },
    });
    expect(spanColorForSpan(orderRow).label).toBe("ORDER");
  });

  test("engine.event span with an unknown engine kind still renders (typed fallback)", () => {
    const row = span({
      kind: "engine.event",
      attributes: { engine_event_kind: "brand_new_engine_signal" },
    });
    const style = spanColorForSpan(row);
    expect(style.hex).toMatch(/^#[0-9a-fA-F]{6}$/);
    expect(style.label.length).toBeGreaterThan(0);
  });

  test("a span with a genuinely unknown kind renders the unknown fallback color", () => {
    const row = span({ kind: "totally.unknown" as SpanKind });
    const style = spanColorForSpan(row);
    expect(categoryOfSpan(row)).toBe("unknown");
    expect(style.hex).toMatch(/^#[0-9a-fA-F]{6}$/);
    expect(style.label.length).toBeGreaterThan(0);
  });
});

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
      "unknown",
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
