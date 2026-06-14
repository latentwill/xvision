// frontend/web/src/features/agent-runs/engine-event-kinds.test.ts
//
// WS-8 taxonomy completeness: every known `EngineEvent.kind` must resolve to
// a non-fallback family + a human label; an unknown kind must resolve to the
// typed `unknown` fallback (carrying the raw kind) rather than being silently
// swallowed.

import { describe, expect, test } from "vitest";
import {
  KNOWN_ENGINE_EVENT_KINDS,
  engineEventFamilyOf,
  engineEventLabelOf,
  ENGINE_EVENT_FAMILY_STYLES,
  type EngineEventFamily,
} from "./engine-event-kinds";

describe("engine-event-kinds — taxonomy completeness", () => {
  test("every known kind resolves to a non-unknown family", () => {
    for (const kind of KNOWN_ENGINE_EVENT_KINDS) {
      const family = engineEventFamilyOf(kind);
      expect(
        family,
        `engine-event kind "${kind}" fell through to the unknown family`,
      ).not.toBe("unknown");
    }
  });

  test("every known kind has a non-empty human label", () => {
    for (const kind of KNOWN_ENGINE_EVENT_KINDS) {
      const label = engineEventLabelOf(kind);
      expect(label.length, `engine-event kind "${kind}" has an empty label`).toBeGreaterThan(0);
      // The label must be a friendlier form than the raw snake_case kind.
      expect(label).not.toContain("_");
    }
  });

  test("every family referenced by a known kind has a defined style", () => {
    for (const kind of KNOWN_ENGINE_EVENT_KINDS) {
      const family = engineEventFamilyOf(kind);
      expect(ENGINE_EVENT_FAMILY_STYLES[family]).toBeDefined();
    }
  });

  test("an unknown kind resolves to the typed unknown fallback, not dropped", () => {
    const family: EngineEventFamily = engineEventFamilyOf("some_future_engine_kind");
    expect(family).toBe("unknown");
    // The label still surfaces SOMETHING derived from the raw kind so the row
    // is never blank — the operator sees the kind even for kinds we don't know.
    const label = engineEventLabelOf("some_future_engine_kind");
    expect(label.length).toBeGreaterThan(0);
    expect(label.toLowerCase()).toContain("some future engine kind");
  });

  test("every family style has a hex color + short uppercase label", () => {
    for (const family of Object.keys(
      ENGINE_EVENT_FAMILY_STYLES,
    ) as EngineEventFamily[]) {
      const style = ENGINE_EVENT_FAMILY_STYLES[family];
      expect(style.hex).toMatch(/^#[0-9a-fA-F]{6}$/);
      expect(style.label.length).toBeGreaterThan(0);
      expect(style.label).toBe(style.label.toUpperCase());
    }
  });
});
