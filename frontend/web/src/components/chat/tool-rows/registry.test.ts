import { describe, expect, it } from "vitest";

import {
  GenericReadToolRow,
  UnsupportedWriteToolRow,
} from "./renderers";
import {
  KNOWN_TOOLS,
  TOOL_ROW_REGISTRY,
  WAIVED_TOOLS,
  isToolCovered,
  resolveToolRow,
} from "./registry";

describe("tool-row registry completeness", () => {
  it("every known engine tool has an entry or is explicitly waived", () => {
    const missing = KNOWN_TOOLS.filter((name) => !isToolCovered(name));
    expect(
      missing,
      `Unregistered tools (add a registry entry or list in WAIVED_TOOLS): ${missing.join(", ")}`,
    ).toEqual([]);
  });

  it("no waived tool also has a registry entry (waiver would be dead)", () => {
    for (const name of WAIVED_TOOLS) {
      expect(
        name in TOOL_ROW_REGISTRY,
        `${name} is both waived and registered`,
      ).toBe(false);
    }
  });

  it("every registry entry carries a renderer, side-effect class, and label", () => {
    for (const [name, entry] of Object.entries(TOOL_ROW_REGISTRY)) {
      expect(typeof entry.render, `${name}.render`).toBe("function");
      expect(entry.sideEffect, `${name}.sideEffect`).toBeTruthy();
      expect(entry.label, `${name}.label`).toBeTruthy();
    }
  });

  it("read-classified entries declare a read-only side effect", () => {
    // The Rust classifier Read arm — these must never be a write class.
    const readVerbs = [
      "get_strategy",
      "list_strategies",
      "resolve_strategy",
      "read_strategies_file",
    ];
    for (const v of readVerbs) {
      expect(TOOL_ROW_REGISTRY[v]?.sideEffect, v).not.toBe("external_write");
    }
  });
});

describe("resolveToolRow fallback", () => {
  it("returns the registered renderer for a known tool", () => {
    expect(resolveToolRow("create_strategy", "external_write")).toBe(
      TOOL_ROW_REGISTRY.create_strategy.render,
    );
    expect(resolveToolRow("list_strategies", "read_only")).toBe(
      TOOL_ROW_REGISTRY.list_strategies.render,
    );
  });

  it("unknown READ-ONLY tool falls back to the generic read renderer", () => {
    expect(resolveToolRow("some_new_inspect_verb", "read_only")).toBe(
      GenericReadToolRow,
    );
    expect(resolveToolRow("another_reader", "external_read")).toBe(
      GenericReadToolRow,
    );
    expect(resolveToolRow("pure_calc", "pure")).toBe(GenericReadToolRow);
  });

  it("unknown WRITE tool falls back to the unsupported-write renderer", () => {
    expect(resolveToolRow("some_new_write_verb", "external_write")).toBe(
      UnsupportedWriteToolRow,
    );
  });

  it("unknown tool with missing side-effect fails safe to unsupported-write", () => {
    expect(resolveToolRow("mystery_verb", null)).toBe(UnsupportedWriteToolRow);
    expect(resolveToolRow("mystery_verb", undefined)).toBe(
      UnsupportedWriteToolRow,
    );
    expect(resolveToolRow(null, null)).toBe(UnsupportedWriteToolRow);
  });
});
