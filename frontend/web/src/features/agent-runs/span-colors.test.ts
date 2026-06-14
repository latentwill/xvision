// frontend/web/src/features/agent-runs/span-colors.test.ts
//
// WS-17 span-taxonomy rename: `decision.model` / `decision.reasoning`
// must map to first-class colour categories so the trace dock reads the
// decision-producing model call (and its chain-of-thought) as what they
// are, not as a generic supervisor catch-all. The retired `model.call` /
// `model.reasoning` wire values stay recognised as legacy aliases.

import { describe, expect, test } from "vitest";
import type { SpanKind } from "@/api/types-agent-runs";
import { CATEGORY_STYLES, categoryOf, spanColor } from "./span-colors";

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
    ] as const) {
      expect(CATEGORY_STYLES[cat]).toBeDefined();
      expect(CATEGORY_STYLES[cat].hex).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });
});
