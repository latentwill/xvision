// frontend/web/src/features/agent-runs/SpanTree.test.tsx
import { beforeEach, describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { RunSpan } from "@/api/types-agent-runs";
import { SpanTree } from "./SpanTree";
import { useTraceDock } from "@/stores/trace-dock";

/**
 * A small parented fixture:
 *   s1 (agent.run)
 *     ├─ s2 (agent.plan)
 *     └─ s3 (model.call)            ← parent the test collapses/expands
 *          └─ s4 (tool.call)        ← grandchild — must hide when s3 collapses
 *   orphan (model.call, parent missing from set)
 */
function mkSpan(
  p: Partial<RunSpan> & Pick<RunSpan, "span_id" | "name" | "kind">,
): RunSpan {
  return {
    parent_span_id: null,
    started_at: "2026-06-14T10:00:00.000Z",
    finished_at: "2026-06-14T10:00:01.000Z",
    status: "ok",
    attributes: {},
    ...p,
  };
}

const SPANS: RunSpan[] = [
  mkSpan({
    span_id: "s1",
    name: "agent.run",
    kind: "agent.run",
    started_at: "2026-06-14T10:00:00.000Z",
    finished_at: "2026-06-14T10:00:05.000Z",
  }),
  mkSpan({
    span_id: "s2",
    parent_span_id: "s1",
    name: "plan",
    kind: "agent.plan",
    started_at: "2026-06-14T10:00:00.100Z",
    finished_at: "2026-06-14T10:00:00.500Z",
  }),
  mkSpan({
    span_id: "s3",
    parent_span_id: "s1",
    name: "claude-opus-4-7",
    kind: "model.call",
    started_at: "2026-06-14T10:00:00.500Z",
    finished_at: "2026-06-14T10:00:02.000Z",
    attributes: { cost_usd: 0.04, input_tokens: 8412, output_tokens: 1204 },
  }),
  mkSpan({
    span_id: "s4",
    parent_span_id: "s3",
    name: "run_backtest",
    kind: "tool.call",
    started_at: "2026-06-14T10:00:01.000Z",
    finished_at: "2026-06-14T10:00:01.800Z",
  }),
  mkSpan({
    // Parent id points at a span NOT in the set → orphan, renders top-level.
    span_id: "orphan",
    parent_span_id: "missing-parent",
    name: "dangling.call",
    kind: "model.call",
    started_at: "2026-06-14T10:00:03.000Z",
    finished_at: "2026-06-14T10:00:03.400Z",
  }),
];

function renderTree(
  overrides: Partial<React.ComponentProps<typeof SpanTree>> = {},
) {
  return render(
    <SpanTree
      spans={SPANS}
      selectedSpanId={null}
      onSelect={() => {}}
      {...overrides}
    />,
  );
}

describe("SpanTree", () => {
  beforeEach(() => {
    // Clean shared collapse slice + storage before each test.
    localStorage.clear();
    useTraceDock.getState().expandAllSpans();
  });

  test("(a) renders parents containing children, indented by depth", () => {
    renderTree();
    // Every span renders a row when nothing is collapsed.
    for (const id of ["s1", "s2", "s3", "s4", "orphan"]) {
      expect(screen.getByTestId(`span-tree-row-${id}`)).toBeInTheDocument();
    }
    // Depth reflects the parent_span_id chain.
    expect(screen.getByTestId("span-tree-row-s1")).toHaveAttribute("data-depth", "0");
    expect(screen.getByTestId("span-tree-row-s3")).toHaveAttribute("data-depth", "1");
    expect(screen.getByTestId("span-tree-row-s4")).toHaveAttribute("data-depth", "2");
  });

  test("(a) collapsing a parent hides its entire subtree; expanding shows it", async () => {
    renderTree();
    // s3 has a child s4 → it has a disclosure caret.
    const caret = screen.getByTestId("span-tree-caret-s3");
    await userEvent.click(caret);

    // s3 stays, its descendant s4 is hidden.
    expect(screen.getByTestId("span-tree-row-s3")).toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-row-s4")).not.toBeInTheDocument();
    // Siblings unaffected.
    expect(screen.getByTestId("span-tree-row-s2")).toBeInTheDocument();

    // Expanding again restores the subtree.
    await userEvent.click(screen.getByTestId("span-tree-caret-s3"));
    expect(screen.getByTestId("span-tree-row-s4")).toBeInTheDocument();
  });

  test("(a) collapsing the root hides ALL descendants", async () => {
    renderTree();
    await userEvent.click(screen.getByTestId("span-tree-caret-s1"));
    for (const id of ["s2", "s3", "s4"]) {
      expect(screen.queryByTestId(`span-tree-row-${id}`)).not.toBeInTheDocument();
    }
    // Root + the unrelated orphan still render.
    expect(screen.getByTestId("span-tree-row-s1")).toBeInTheDocument();
    expect(screen.getByTestId("span-tree-row-orphan")).toBeInTheDocument();
  });

  test("(b) a collapsed parent shows a one-line rollup (kind, duration, status, child count)", async () => {
    renderTree();
    await userEvent.click(screen.getByTestId("span-tree-caret-s3"));
    const rollup = screen.getByTestId("span-tree-rollup-s3");
    // Kind label.
    expect(rollup).toHaveTextContent(/MODEL/);
    // Duration (s3 ran 1.5s).
    expect(rollup).toHaveTextContent(/1\.5/);
    // Key metric: the descendant count it is hiding.
    expect(rollup).toHaveTextContent(/1/);
  });

  test("(b) the rollup surfaces a key metric (cost) when present on the span", async () => {
    renderTree();
    await userEvent.click(screen.getByTestId("span-tree-caret-s3"));
    // s3 carries attributes.cost_usd = 0.04 → surfaced in the rollup.
    expect(screen.getByTestId("span-tree-rollup-s3")).toHaveTextContent(/\$0\.04/);
  });

  test("(c) collapse-all collapses every node with children; expand-all reopens", async () => {
    renderTree();
    await userEvent.click(screen.getByRole("button", { name: /collapse all/i }));
    // Only the roots (s1, orphan) remain visible; nested nodes hide.
    expect(screen.getByTestId("span-tree-row-s1")).toBeInTheDocument();
    expect(screen.getByTestId("span-tree-row-orphan")).toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-row-s2")).not.toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-row-s3")).not.toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-row-s4")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /expand all/i }));
    for (const id of ["s2", "s3", "s4"]) {
      expect(screen.getByTestId(`span-tree-row-${id}`)).toBeInTheDocument();
    }
  });

  test("(d) collapse state persists across a re-render (store + localStorage)", async () => {
    const { unmount } = renderTree();
    await userEvent.click(screen.getByTestId("span-tree-caret-s3"));
    expect(screen.queryByTestId("span-tree-row-s4")).not.toBeInTheDocument();

    // Persisted in the shared store / localStorage.
    expect(useTraceDock.getState().collapsedSpanIds.has("s3")).toBe(true);
    expect(
      JSON.parse(
        localStorage.getItem("xvision.trace-dock.collapsed-spans") ?? "[]",
      ),
    ).toContain("s3");

    // Re-mounting a fresh tree reads the persisted collapse → s4 stays hidden.
    unmount();
    renderTree();
    expect(screen.getByTestId("span-tree-row-s3")).toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-row-s4")).not.toBeInTheDocument();
  });

  test("(e) orphan / parentless spans render at the top level (not dropped)", () => {
    renderTree();
    const orphan = screen.getByTestId("span-tree-row-orphan");
    // Orphan's parent is missing from the set → depth 0, rendered top-level.
    expect(orphan).toHaveAttribute("data-depth", "0");
    expect(orphan).toBeInTheDocument();
  });

  test("a leaf span renders no disclosure caret", () => {
    renderTree();
    // s2 (agent.plan) and s4 (tool.call) are leaves.
    expect(screen.queryByTestId("span-tree-caret-s2")).not.toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-caret-s4")).not.toBeInTheDocument();
    // s4 only after expand — but s2 is always a leaf.
  });

  test("clicking a row calls onSelect with the span id (not the caret toggle)", async () => {
    const onSelect = vi.fn();
    renderTree({ onSelect });
    await userEvent.click(screen.getByTestId("span-tree-label-s2"));
    expect(onSelect).toHaveBeenCalledWith("s2");
  });

  test("selected row is marked data-selected", () => {
    renderTree({ selectedSpanId: "s3" });
    expect(screen.getByTestId("span-tree-row-s3")).toHaveAttribute(
      "data-selected",
      "true",
    );
  });

  test("renders an empty-state message when there are no spans", () => {
    render(<SpanTree spans={[]} selectedSpanId={null} onSelect={() => {}} />);
    expect(screen.getByText(/no spans/i)).toBeInTheDocument();
  });
});
