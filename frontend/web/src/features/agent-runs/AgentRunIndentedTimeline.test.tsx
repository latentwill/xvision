// frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AgentRunIndentedTimeline } from "./AgentRunIndentedTimeline";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

describe("AgentRunIndentedTimeline", () => {
  test("renders one row per span with correct nesting", () => {
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId(/^span-row-/)).toHaveLength(MOCK_RUN_COMPLETED.spans.length);
    const child = screen.getByTestId("span-row-s4");
    expect(child).toHaveAttribute("data-depth", "2");
  });

  test("clicking a row calls onSelect with span id", async () => {
    const onSelect = vi.fn();
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByTestId("span-row-s3"));
    expect(onSelect).toHaveBeenCalledWith("s3");
  });

  test("selected row gets data-selected=true", () => {
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId="s3"
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId("span-row-s3")).toHaveAttribute("data-selected", "true");
  });
});
