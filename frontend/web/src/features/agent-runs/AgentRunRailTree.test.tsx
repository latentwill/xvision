// frontend/web/src/features/agent-runs/AgentRunRailTree.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AgentRunRailTree } from "./AgentRunRailTree";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

describe("AgentRunRailTree", () => {
  test("renders one node per span with kind labels", () => {
    render(
      <AgentRunRailTree
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId(/^rail-node-/)).toHaveLength(
      MOCK_RUN_COMPLETED.spans.length,
    );
  });

  test("clicking a node calls onSelect", async () => {
    const onSelect = vi.fn();
    render(
      <AgentRunRailTree
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByTestId("rail-node-s4"));
    expect(onSelect).toHaveBeenCalledWith("s4");
  });
});
