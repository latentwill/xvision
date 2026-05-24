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
    expect(screen.getAllByTestId(/^span-row-/)).toHaveLength(
      MOCK_RUN_COMPLETED.spans.length - 1,
    );
    expect(screen.queryByTestId("span-row-s6")).not.toBeInTheDocument();
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

  test("each row renders a positioned waterfall bar", () => {
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    const root = screen.getByTestId("span-waterfall-bar-s1");
    expect(root.style.left).toBe("0%");
    expect(parseFloat(root.style.width)).toBeGreaterThan(0);

    const earlier = screen.getByTestId("span-waterfall-bar-s2");
    const later = screen.getByTestId("span-waterfall-bar-s4");
    expect(parseFloat(later.style.left)).toBeGreaterThan(parseFloat(earlier.style.left));

    for (const span of MOCK_RUN_COMPLETED.spans.filter((s) => s.span_id !== "s6")) {
      const bar = screen.getByTestId(`span-waterfall-bar-${span.span_id}`);
      const left = parseFloat(bar.style.left);
      const width = parseFloat(bar.style.width);
      expect(left + width).toBeLessThanOrEqual(100.0001);
    }
  });

  test("renders the kind chip label and span name as distinct fields", () => {
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    const row = screen.getByTestId("span-row-s3");
    expect(row).toHaveTextContent(/MODEL/);
    expect(row).toHaveTextContent(/claude-opus-4-7/);
  });
});
