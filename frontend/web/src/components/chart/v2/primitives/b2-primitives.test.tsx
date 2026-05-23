/**
 * Tests for B2 primitives:
 *  - columnCountForN math (StrategyCardGrid)
 *  - StrategyRosterPills interaction (toggle, remove, disabled at min)
 *  - LeadCardChrome conditional gradient/border
 *
 * Mock UplotDrawdownPane (transitively pulled by Card via Mini?) — not
 * needed here; B2 primitives don't import it. The matchMedia polyfill
 * lives in test-setup.ts.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

import { columnCountForN, StrategyCardGrid } from "./StrategyCardGrid";
import { StrategyRosterPills, type RosterPillItem } from "./StrategyRosterPills";
import { LeadCardChrome } from "./LeadCardChrome";

describe("columnCountForN", () => {
  it.each([
    [0, 2],
    [1, 2],
    [2, 2],
    [3, 4],
    [4, 4],
    [5, 3],
    [6, 3],
    [7, 4],
    [12, 4],
  ])("n=%i → cols=%i", (n, expected) => {
    expect(columnCountForN(n)).toBe(expected);
  });
});

describe("StrategyCardGrid", () => {
  it("applies the expected gridTemplateColumns + data-cols attribute", () => {
    render(
      <StrategyCardGrid count={5}>
        <div>a</div>
        <div>b</div>
      </StrategyCardGrid>,
    );
    const grid = screen.getByTestId("strategy-card-grid");
    expect(grid.getAttribute("data-cols")).toBe("3");
    expect(grid.style.gridTemplateColumns).toBe("repeat(3, minmax(0, 1fr))");
  });
});

describe("StrategyRosterPills", () => {
  const available: RosterPillItem[] = [
    { id: "fib", label: "Fib · GC", color: "#D4A547" },
    { id: "ema", label: "EMA · 50/200", color: "#E8DCB0" },
    { id: "brk", label: "BRK · 4h", color: "#E07A3A" },
  ];

  it("renders all available pills regardless of selection", () => {
    render(
      <StrategyRosterPills
        available={available}
        selectedIds={["fib"]}
        onToggle={() => {}}
        onRemove={() => {}}
        canRemove={() => false}
      />,
    );
    expect(screen.getByText("Fib · GC")).toBeInTheDocument();
    expect(screen.getByText("EMA · 50/200")).toBeInTheDocument();
    expect(screen.getByText("BRK · 4h")).toBeInTheDocument();
  });

  it("calls onToggle when a pill is clicked", () => {
    const onToggle = vi.fn();
    render(
      <StrategyRosterPills
        available={available}
        selectedIds={["fib"]}
        onToggle={onToggle}
        onRemove={() => {}}
        canRemove={() => true}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Enable EMA/ }));
    expect(onToggle).toHaveBeenCalledWith("ema");
  });

  it("only renders the × button on selected pills, and disables when canRemove is false", () => {
    const onRemove = vi.fn();
    render(
      <StrategyRosterPills
        available={available}
        selectedIds={["fib", "ema"]}
        onToggle={() => {}}
        onRemove={onRemove}
        canRemove={(id) => id === "ema"}
      />,
    );
    // 'brk' is not selected — no × button
    expect(screen.queryByRole("button", { name: /Remove BRK/ })).toBeNull();
    // 'fib' is selected but not removable
    const fibBtn = screen.getByRole("button", { name: /Remove Fib · GC/ });
    expect(fibBtn).toBeDisabled();
    // 'ema' is removable
    const emaBtn = screen.getByRole("button", { name: /Remove EMA/ });
    expect(emaBtn).not.toBeDisabled();
    fireEvent.click(emaBtn);
    expect(onRemove).toHaveBeenCalledWith("ema");
  });
});

describe("LeadCardChrome", () => {
  it("renders children inside a plain card when lead=false", () => {
    render(
      <LeadCardChrome lead={false}>
        <div data-testid="inner">x</div>
      </LeadCardChrome>,
    );
    expect(screen.getByTestId("inner")).toBeInTheDocument();
    expect(screen.queryByTestId("lead-card-chrome")).toBeNull();
  });

  it("applies the gold gradient chrome when lead=true", () => {
    render(
      <LeadCardChrome lead={true}>
        <div data-testid="inner">x</div>
      </LeadCardChrome>,
    );
    expect(screen.getByTestId("inner")).toBeInTheDocument();
    expect(screen.getByTestId("lead-card-chrome")).toBeInTheDocument();
  });
});
