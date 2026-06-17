// frontend/web/src/features/agent-runs/TraceDock.test.tsx
import { beforeEach, describe, expect, test } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { TraceDock } from "./TraceDock";
import { useTraceDock } from "@/stores/trace-dock";
import type { DockMode } from "@/stores/trace-dock";

/**
 * Seed the eval scope slice (the default MemoryRouter path "/" maps to
 * the eval scope, which is what TraceDock reads here). Leaves the live
 * scope at its init state.
 */
function setEvalScope(slice: {
  activeRunId?: string | null;
  selectedSpanId?: string | null;
  mode?: DockMode;
  costOverrideUsd?: number | null;
}) {
  useTraceDock.setState((s) => ({
    byScope: {
      ...s.byScope,
      eval: {
        activeRunId: slice.activeRunId ?? null,
        selectedSpanId: slice.selectedSpanId ?? null,
        mode: slice.mode ?? "post-hoc",
        costOverrideUsd: slice.costOverrideUsd ?? null,
      },
    },
  }));
}

function renderDock() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <TraceDock />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("TraceDock", () => {
  beforeEach(() => {
    localStorage.clear();
    // useSpanFilter persists the active filter to BOTH localStorage and the
    // URL (`?q=kind:model` etc.) — reset the URL too so a kind filter set by
    // an earlier test can't leak in and hide spans the next test asserts on.
    window.history.replaceState({}, "", "/");
    useTraceDock.setState({
      height: "collapsed",
      lastOpenHeight: "working",
    });
    // WS-16: collapse state is a SHARED store slice (persisted) — reset it
    // so tree-collapse state can't leak between tests in this file.
    useTraceDock.getState().expandAllSpans();
    setEvalScope({ activeRunId: null, selectedSpanId: null, costOverrideUsd: null });
  });

  test("renders nothing when activeRunId is null", () => {
    renderDock();
    expect(screen.queryByTestId("trace-dock")).toBeNull();
  });

  test("renders header when activeRunId set, hidden body when collapsed", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "collapsed" });
    renderDock();
    // Header still hidden at collapsed — strip handles that.
    expect(screen.queryByTestId("trace-dock-body")).toBeNull();
  });

  test("shows body at working height", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    expect(await screen.findByTestId("trace-dock-body")).toBeInTheDocument();
  });

  test("minimize button collapses the dock", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    await userEvent.click(screen.getByLabelText(/minimize/i));
    expect(useTraceDock.getState().height).toBe("collapsed");
  });

  test("renders no Full preset button (resize handle owns height)", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    expect(screen.queryByRole("button", { name: /^full$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^peek$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^working$/i })).toBeNull();
    // The pop-out arrows remain — the sole fullscreen affordance.
    expect(
      screen.getByLabelText(/pop out to dedicated view/i),
    ).toBeInTheDocument();
    // The resize handle is mounted.
    expect(screen.getByTestId("trace-dock-resize-handle")).toBeInTheDocument();
  });

  test("dock renders at the store's heightPx", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working", heightPx: 612 });
    renderDock();
    const dock = await screen.findByTestId("trace-dock");
    expect(dock).toHaveStyle({ height: "612px" });
  });

  test("inspector selection falls back to the first filtered span", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working", advanced_view: false });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    // WS-16: the tree is the default structured view; this test asserts
    // flame-graph rendering, so switch to the FLAME view first.
    await userEvent.click(screen.getByRole("button", { name: /flame/i }));
    await screen.findByTestId("flame-bar-s1");

    // The Simple-mode inspector summary embeds the span_id inside a
    // longer string ("s1 · agent.run · …"), so we assert by reading
    // the summary container's text content rather than searching for
    // a bare "s1" / "s3" text node.
    const inspectorBefore = await screen.findByTestId("span-inspector-fields-simple");
    expect(inspectorBefore).toHaveTextContent(/s1 · agent\.run/);

    await userEvent.click(screen.getByRole("button", { name: /^MODEL$/i }));

    const inspectorAfter = await screen.findByTestId("span-inspector-fields-simple");
    expect(inspectorAfter).toHaveTextContent(/s3 · model\.call/);
    expect(inspectorAfter).not.toHaveTextContent(/s1 · agent\.run/);
    expect(screen.queryByTestId("flame-bar-s1")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("span-inspector")).toHaveAttribute(
        "data-span-id",
        "s3",
      ),
    );
  });

  test("Trade button (F-7) renders disabled when no broker.call spans are present", async () => {
    // MOCK_RUN_COMPLETED has no broker.call span — the affordance still
    // appears so the operator knows the concept exists, but a click is
    // a no-op. The disabled state surfaces the *reason* via title.
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    const btn = screen.getByTestId("trace-dock-trade-button");
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute(
      "title",
      expect.stringContaining("No broker.call spans"),
    );
  });

  test("capsule cost reflects the eval-side override when one is pinned", async () => {
    // MOCK_RUN_COMPLETED.summary.total_cost_usd is 0.0624; the eval-side
    // `inference_cost_quote_total` may differ when pricing rolled up on
    // the eval table only. The capsule must prefer the pinned override
    // so it matches the meta-strip number rather than the stale rollup.
    setEvalScope({ activeRunId: "run_abc1234", costOverrideUsd: 0.4242 });
    useTraceDock.setState({ height: "working" });
    renderDock();
    const cost = await screen.findByTestId("trace-dock-cost");
    expect(cost.textContent).toBe("$0.4242");
    expect(cost.getAttribute("title")).toBe("$0.4242");
  });

  test("capsule cost falls back to the agent-run summary when no override is pinned", async () => {
    setEvalScope({ activeRunId: "run_abc1234", costOverrideUsd: null });
    useTraceDock.setState({ height: "working" });
    renderDock();
    const cost = await screen.findByTestId("trace-dock-cost");
    // MOCK_RUN_COMPLETED.summary.total_cost_usd === 0.0624.
    expect(cost.textContent).toBe("$0.0624");
  });

  test("WS-16: renders the collapsible span tree as the default structured view", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    // The span tree is the default structured view (parents + children).
    expect(await screen.findByTestId("span-tree-row-s1")).toBeInTheDocument();
    expect(screen.getByTestId("span-tree-row-s3")).toBeInTheDocument();
    // FlameGraph is not the default — but its toggle exists.
    expect(screen.queryByTestId("flame-bar-s1")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /flame/i }),
    ).toBeInTheDocument();
  });

  test("WS-16: the view toggle swaps the tree for the flame graph", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    await screen.findByTestId("span-tree-row-s1");
    await userEvent.click(screen.getByRole("button", { name: /flame/i }));
    expect(await screen.findByTestId("flame-bar-s1")).toBeInTheDocument();
    expect(screen.queryByTestId("span-tree-row-s1")).not.toBeInTheDocument();
    // And back to the tree.
    await userEvent.click(screen.getByRole("button", { name: /^tree$/i }));
    expect(await screen.findByTestId("span-tree-row-s1")).toBeInTheDocument();
  });

  test("WS-16: collapsing a DECISION in the tree hides its subtree", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working", advanced_view: true });
    useTraceDock.getState().expandAllSpans();
    renderDock();
    await screen.findByTestId("span-tree-row-s3");
    // s3 (model.call) has a child s4 (tool.call) in the fixture.
    expect(screen.getByTestId("span-tree-row-s4")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("span-tree-caret-s3"));
    expect(screen.queryByTestId("span-tree-row-s4")).not.toBeInTheDocument();
    // Collapsed parent shows its one-line rollup.
    expect(screen.getByTestId("span-tree-rollup-s3")).toBeInTheDocument();
  });

  test("a sticky filter that hides every span shows Clear filters, and clicking it reveals the spans", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working", advanced_view: true });
    // Reproduce the real bug: a sticky URL filter (the `?q=` useSpanFilter
    // persists + reloads) matching no span in this run.
    window.history.replaceState({}, "", "/?q=zzz_no_such_span");
    renderDock();
    await screen.findByTestId("trace-dock-body");

    // Honest empty state — names the filter as the cause and offers recovery,
    // instead of dead-ending the operator with an unexplained "no spans".
    expect(
      await screen.findByText(/no spans match the current filter/i),
    ).toBeInTheDocument();
    // The spans were NOT lost — they're hidden, not absent.
    expect(screen.queryByTestId("span-tree-row-s1")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /clear filters/i }));

    // One click brings the run's spans back.
    expect(await screen.findByTestId("span-tree-row-s1")).toBeInTheDocument();
    expect(screen.queryByText(/no spans match the current filter/i)).toBeNull();
  });

  test("filter bar is the first row — above the TRACE header and the TREE/FLAME view toggle", async () => {
    setEvalScope({ activeRunId: "run_abc1234" });
    useTraceDock.setState({ height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    const filterInput = screen.getByPlaceholderText(/^filter/i);
    const viewToggle = screen.getByTestId("trace-dock-view-toggle");
    // Filter-first layout: the filter must precede the view toggle (and the
    // TRACE header it sits in) in document order.
    expect(
      filterInput.compareDocumentPosition(viewToggle) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
