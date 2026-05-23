// Render-level smoke tests for `<FiringSection>`. Covers:
//   1. Filter-capability refs render nothing (the Filter is the gate).
//   2. Default state shows "Every bar." + an "Add filter →" button.
//   3. Active state shows the "Fires when …" summary derived from the
//      incoming PipelineEdge.condition.
//   4. Clicking "Add filter →" opens the inline composer.

import { describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach } from "vitest";

import { FiringSection } from "./FiringSection";
import type { AgentRef, PipelineDef } from "@/api/strategies";
import type { ProviderRow } from "@/api/types.gen/ProviderRow";

afterEach(() => cleanup());

const noopMutated = () => {};

const traderRef: AgentRef = {
  agent_id: "01HTRADER000000000000000000",
  role: "trader",
};
const filterRef: AgentRef = {
  agent_id: "01HFILTER000000000000000000",
  role: "regime_filter",
  activates: "filter",
};

describe("FiringSection", () => {
  it("renders nothing for Filter-capability refs", () => {
    const { container } = render(
      <FiringSection
        strategyId="01STRAT0000000000000000000"
        agentRef={filterRef}
        refs={[filterRef]}
        pipeline={{ kind: "single", edges: [] }}
        filterCandidates={[]}
        providers={[]}
        onMutated={noopMutated}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows 'Every bar.' + Add filter button when no incoming edge", () => {
    render(
      <FiringSection
        strategyId="01STRAT0000000000000000000"
        agentRef={traderRef}
        refs={[traderRef]}
        pipeline={{ kind: "single", edges: [] }}
        filterCandidates={[]}
        providers={[]}
        onMutated={noopMutated}
      />,
    );
    expect(screen.getByText("Every bar.")).toBeInTheDocument();
    expect(
      screen.getByTestId(`firing-add-filter-${traderRef.role}`),
    ).toBeInTheDocument();
  });

  it("shows the 'Fires when …' summary when an upstream Filter gates this ref", () => {
    const refs = [filterRef, traderRef];
    const pipeline: PipelineDef = {
      kind: "graph",
      edges: [
        {
          from_role: "regime_filter",
          to_role: "trader",
          condition: { eq: { signal_field: "regime", value: "trend" } },
        },
      ],
    };
    render(
      <FiringSection
        strategyId="01STRAT0000000000000000000"
        agentRef={traderRef}
        refs={refs}
        pipeline={pipeline}
        filterCandidates={[]}
        providers={[]}
        onMutated={noopMutated}
      />,
    );
    expect(screen.getByText("regime_filter")).toBeInTheDocument();
    expect(screen.getByText("regime")).toBeInTheDocument();
    expect(screen.getByText("==")).toBeInTheDocument();
    expect(screen.getByText('"trend"')).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove" })).toBeInTheDocument();
  });

  it("opens the inline composer when Add filter is clicked", () => {
    const onMutated = vi.fn();
    render(
      <FiringSection
        strategyId="01STRAT0000000000000000000"
        agentRef={traderRef}
        refs={[traderRef]}
        pipeline={{ kind: "single", edges: [] }}
        filterCandidates={[]}
        providers={[] as ProviderRow[]}
        onMutated={onMutated}
      />,
    );
    expect(screen.queryByTestId(`inline-filter-composer-${traderRef.role}`)).toBeNull();
    fireEvent.click(screen.getByTestId(`firing-add-filter-${traderRef.role}`));
    expect(
      screen.getByTestId(`inline-filter-composer-${traderRef.role}`),
    ).toBeInTheDocument();
    // No mutations yet — composer hasn't been submitted.
    expect(onMutated).not.toHaveBeenCalled();
  });
});
