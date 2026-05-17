// frontend/web/src/features/agent-runs/FlameGraph.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FlameGraph } from "./FlameGraph";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

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
});
