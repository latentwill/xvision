import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PulseViewSwitcher } from "./PulseViewSwitcher";

describe("PulseViewSwitcher", () => {
  it("renders all five views and marks the active one", () => {
    render(<PulseViewSwitcher view="return" onViewChange={() => {}} />);
    for (const label of [
      "Return %",
      "Price + trades",
      "vs Buy & Hold",
      "Drawdown",
      "All runs",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.getByRole("button", { name: "Return %" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "Drawdown" }),
    ).toHaveAttribute("aria-pressed", "false");
  });

  it("fires onViewChange with the view id", () => {
    const onViewChange = vi.fn();
    render(<PulseViewSwitcher view="return" onViewChange={onViewChange} />);
    fireEvent.click(screen.getByRole("button", { name: "All runs" }));
    expect(onViewChange).toHaveBeenCalledWith("field");
  });
});
