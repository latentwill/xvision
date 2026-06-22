import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Agent } from "@/api/agents";
import type { AgentRef, PipelineDef } from "@/api/strategies";
import { InlineFilterComposer } from "./InlineFilterComposer";

function agent(overrides: Partial<Agent>): Agent {
  return {
    agent_id: "trend-agent",
    name: "Trend filter",
    description: "Tracks trend state",
    tags: ["filter"],
    slots: [],
    archived: false,
    created_at: "2026-06-22T00:00:00Z",
    updated_at: "2026-06-22T00:00:00Z",
    ...overrides,
  };
}

const target: AgentRef = {
  agent_id: "trader-agent",
  role: "trader",
};

const pipeline: PipelineDef = { kind: "single" };

describe("InlineFilterComposer", () => {
  it("searches filter-capable agents by name and id", async () => {
    const user = userEvent.setup();

    render(
      <InlineFilterComposer
        strategyId="strategy-1"
        target={target}
        pipeline={pipeline}
        filterCandidates={[
          agent({
            agent_id: "trend-agent",
            name: "Trend detector",
            description: "Only visible before search",
          }),
          agent({
            agent_id: "regime-agent",
            name: "Regime detector",
            description: "Selected regime filter description",
            scope_strategy_id: "strategy-1",
          }),
        ]}
        providers={[]}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /filter agent/i }));
    await user.type(
      screen.getByRole("textbox", { name: /search filter agent/i }),
      "regime-agent",
    );
    await user.click(await screen.findByRole("option", { name: /Regime detector/i }));

    expect(screen.getByText("Selected regime filter description")).toBeInTheDocument();
  });
});
