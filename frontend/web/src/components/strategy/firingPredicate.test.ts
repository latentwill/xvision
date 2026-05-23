import { describe, expect, it } from "vitest";
import {
  buildPredicate,
  describePredicate,
  findIncomingFilterEdge,
  withAddedEdge,
  withoutEdge,
} from "./firingPredicate";

describe("buildPredicate", () => {
  it("parses numeric values", () => {
    const p = buildPredicate("gte", "confidence.value", "0.8");
    expect(p).toEqual({ gte: { signal_field: "confidence.value", value: 0.8 } });
  });

  it("parses booleans", () => {
    const p = buildPredicate("eq", "is_open", "true");
    expect(p).toEqual({ eq: { signal_field: "is_open", value: true } });
  });

  it("keeps non-numeric values as strings", () => {
    const p = buildPredicate("eq", "regime", "high_vol");
    expect(p).toEqual({ eq: { signal_field: "regime", value: "high_vol" } });
  });
});

describe("describePredicate", () => {
  it("round-trips a scalar eq predicate", () => {
    expect(describePredicate({ eq: { signal_field: "regime", value: "trend" } })).toEqual({
      op: "eq",
      signalField: "regime",
      value: "trend",
    });
  });

  it("returns null for composite predicates the composer can't render", () => {
    expect(
      describePredicate({
        all: [{ eq: { signal_field: "a", value: 1 } }, { eq: { signal_field: "b", value: 2 } }],
      }),
    ).toBeNull();
  });
});

describe("findIncomingFilterEdge", () => {
  it("returns the gating edge and upstream filter ref", () => {
    const refs = [
      { agent_id: "01F", role: "regime_filter", activates: "filter" as const },
      { agent_id: "01T", role: "trader" },
    ];
    const pipeline = {
      kind: "graph" as const,
      edges: [
        {
          from_role: "regime_filter",
          to_role: "trader",
          condition: { eq: { signal_field: "regime", value: "trend" } },
        },
      ],
    };
    const out = findIncomingFilterEdge(refs[1], pipeline, refs);
    expect(out).not.toBeNull();
    expect(out!.upstream.role).toBe("regime_filter");
  });

  it("ignores unconditional edges", () => {
    const refs = [
      { agent_id: "01F", role: "regime_filter" },
      { agent_id: "01T", role: "trader" },
    ];
    const pipeline = {
      kind: "sequential" as const,
      edges: [{ from_role: "regime_filter", to_role: "trader" }],
    };
    expect(findIncomingFilterEdge(refs[1], pipeline, refs)).toBeNull();
  });
});

describe("withAddedEdge / withoutEdge", () => {
  it("adds the new edge and promotes to graph", () => {
    const out = withAddedEdge(
      { kind: "sequential", edges: [] },
      {
        from_role: "regime_filter",
        to_role: "trader",
        condition: { eq: { signal_field: "regime", value: "trend" } },
      },
    );
    expect(out.kind).toBe("graph");
    expect(out.edges).toHaveLength(1);
  });

  it("replaces an existing edge with the same (from, to)", () => {
    const out = withAddedEdge(
      {
        kind: "graph",
        edges: [
          {
            from_role: "regime_filter",
            to_role: "trader",
            condition: { eq: { signal_field: "regime", value: "trend" } },
          },
        ],
      },
      {
        from_role: "regime_filter",
        to_role: "trader",
        condition: { eq: { signal_field: "regime", value: "chop" } },
      },
    );
    expect(out.edges).toHaveLength(1);
    expect(out.edges![0].condition).toEqual({
      eq: { signal_field: "regime", value: "chop" },
    });
  });

  it("removes the requested edge", () => {
    const out = withoutEdge(
      {
        kind: "graph",
        edges: [
          {
            from_role: "regime_filter",
            to_role: "trader",
            condition: { eq: { signal_field: "regime", value: "trend" } },
          },
          { from_role: "x", to_role: "y" },
        ],
      },
      "regime_filter",
      "trader",
    );
    expect(out.edges).toHaveLength(1);
    expect(out.edges![0].from_role).toBe("x");
  });
});
