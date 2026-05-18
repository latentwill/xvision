// frontend/web/src/features/agent-runs/FlameGraph.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { RunSpan } from "@/api/types-agent-runs";
import { FlameGraph } from "./FlameGraph";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

function mkSpan(p: Partial<RunSpan> & Pick<RunSpan, "span_id" | "name">): RunSpan {
  return {
    span_id: p.span_id,
    parent_span_id: p.parent_span_id ?? null,
    name: p.name,
    kind: p.kind ?? "agent.run",
    started_at: p.started_at ?? "2026-05-18T00:00:00.000Z",
    finished_at: p.finished_at ?? "2026-05-18T00:00:01.000Z",
    status: p.status ?? "ok",
    attributes: p.attributes ?? {},
  };
}

describe("FlameGraph", () => {
  test("renders one bar per span", () => {
    render(
      <FlameGraph
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId(/^flame-bar-/)).toHaveLength(MOCK_RUN_COMPLETED.spans.length);
  });

  test("bar widths reflect duration relative to total", () => {
    render(
      <FlameGraph
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    // Root span ("agent.run", id "s1") has the longest duration — should
    // get the widest bar.
    const root = screen.getByTestId("flame-bar-s1");
    const width = parseFloat(root.style.width);
    expect(width).toBeGreaterThanOrEqual(95);
  });

  test("sibling top-level spans render on distinct rows", () => {
    // Three parentless spans in the filtered set must not collapse onto
    // row 0 — they each get their own lane.
    const spans: RunSpan[] = [
      mkSpan({
        span_id: "a",
        name: "first",
        started_at: "2026-05-18T00:00:00.000Z",
        finished_at: "2026-05-18T00:00:01.000Z",
      }),
      mkSpan({
        span_id: "b",
        name: "second",
        started_at: "2026-05-18T00:00:01.000Z",
        finished_at: "2026-05-18T00:00:02.000Z",
      }),
      mkSpan({
        span_id: "c",
        name: "third",
        started_at: "2026-05-18T00:00:02.000Z",
        finished_at: "2026-05-18T00:00:03.000Z",
      }),
    ];
    render(<FlameGraph spans={spans} selectedSpanId={null} onSelect={() => {}} />);
    const tops = ["a", "b", "c"].map(
      (id) => screen.getByTestId(`flame-bar-${id}`).style.top,
    );
    // Each row gets a distinct top offset.
    expect(new Set(tops).size).toBe(3);
  });

  test("single-span run renders as a chip, not full-width", () => {
    const spans: RunSpan[] = [
      mkSpan({
        span_id: "solo",
        name: "agent.run",
        started_at: "2026-05-18T00:00:00.000Z",
        finished_at: "2026-05-18T00:00:01.000Z",
      }),
    ];
    render(<FlameGraph spans={spans} selectedSpanId={null} onSelect={() => {}} />);
    const bar = screen.getByTestId("flame-bar-solo");
    const width = parseFloat(bar.style.width);
    // Capped: must not paint the entire dock width.
    expect(width).toBeLessThan(100);
    expect(width).toBeGreaterThan(0);
  });

  test("clicking a bar calls onSelect with span id", async () => {
    const onSelect = vi.fn();
    render(
      <FlameGraph
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByTestId("flame-bar-s4"));
    expect(onSelect).toHaveBeenCalledWith("s4");
  });

  test("renders cost from the normalised span.cost field (export-backed shape)", () => {
    // Export-backed model.call spans land on `span.cost` (see normalisation
    // in api/agent-runs.ts:217). `attributes.cost_usd` is empty on this
    // shape — the bar still needs to label the cost.
    const spans: RunSpan[] = [
      {
        ...mkSpan({ span_id: "mc-export", name: "model.call", kind: "model.call" }),
        cost: 0.000123,
      },
    ];
    render(<FlameGraph spans={spans} selectedSpanId={null} onSelect={() => {}} />);
    const bar = screen.getByTestId("flame-bar-mc-export");
    expect(bar.textContent).toContain("$0.000123");
    expect(bar.getAttribute("title")).toContain("$0.000123");
    // Tooltip carries the precise companion in parens.
    expect(bar.getAttribute("title")).toContain("($0.000123)");
  });

  test("falls back to attributes.cost_usd when span.cost is absent (mock-fixture shape)", () => {
    const spans: RunSpan[] = [
      mkSpan({
        span_id: "mc-mock",
        name: "model.call",
        kind: "model.call",
        attributes: { cost_usd: 0.04 },
      }),
    ];
    render(<FlameGraph spans={spans} selectedSpanId={null} onSelect={() => {}} />);
    const bar = screen.getByTestId("flame-bar-mc-mock");
    expect(bar.textContent).toContain("$0.0400");
  });

  test("prefers span.cost over attributes.cost_usd when both are set", () => {
    const spans: RunSpan[] = [
      {
        ...mkSpan({
          span_id: "mc-both",
          name: "model.call",
          kind: "model.call",
          attributes: { cost_usd: 999 },
        }),
        cost: 0.001234,
      },
    ];
    render(<FlameGraph spans={spans} selectedSpanId={null} onSelect={() => {}} />);
    const bar = screen.getByTestId("flame-bar-mc-both");
    expect(bar.textContent).toContain("$0.001234");
    expect(bar.textContent).not.toContain("999");
  });
});
