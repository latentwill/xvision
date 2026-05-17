// frontend/web/src/features/agent-runs/FilterBar.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FilterBar } from "./FilterBar";

const baseProps = {
  query: "",
  setQuery: vi.fn(),
  kinds: new Set<string>(),
  toggleKind: vi.fn(),
  status: "all" as const,
  setStatus: vi.fn(),
  decisionFilter: "all",
  setDecisionFilter: vi.fn(),
  decisions: [{ i: 14 }],
  total: 12,
  filtered: 5,
};

describe("FilterBar", () => {
  test("renders the search input with placeholder hint", () => {
    render(<FilterBar {...baseProps} />);
    expect(
      screen.getByPlaceholderText(/title:agent\.plan/i),
    ).toBeInTheDocument();
  });

  test("renders 5 kind chips: AGENT MODEL TOOL SUPER ARTIF", () => {
    render(<FilterBar {...baseProps} />);
    ["AGENT", "MODEL", "TOOL", "SUPER", "ARTIF"].forEach((label) => {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    });
  });

  test("clicking a kind chip calls toggleKind with that category", async () => {
    const props = { ...baseProps, toggleKind: vi.fn() };
    render(<FilterBar {...props} />);
    await userEvent.click(screen.getByRole("button", { name: "MODEL" }));
    expect(props.toggleKind).toHaveBeenCalledWith("model");
  });

  test("counter shows `<filtered>/<total> spans`", () => {
    render(<FilterBar {...baseProps} />);
    expect(screen.getByText(/5/)).toBeInTheDocument();
    expect(screen.getByText(/12/)).toBeInTheDocument();
    expect(screen.getByText(/spans/)).toBeInTheDocument();
  });

  test("typing in the search input calls setQuery", async () => {
    const setQuery = vi.fn();
    render(<FilterBar {...baseProps} setQuery={setQuery} />);
    await userEvent.type(screen.getByPlaceholderText(/title:agent\.plan/i), "tool:run_backtest");
    expect(setQuery).toHaveBeenLastCalledWith("tool:run_backtest");
  });
});
