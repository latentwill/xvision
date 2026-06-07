import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { ModePicker } from "./ModePicker";
import type { RunMode } from "./ModePicker";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function renderPicker(
  value: RunMode = "once",
  onChange = vi.fn(),
) {
  renderWithProviders(
    <ModePicker value={value} onChange={onChange} />,
  );
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("ModePicker", () => {
  it("renders three mode options: Single experiment, N experiments, Until budget", () => {
    renderPicker("once");
    expect(screen.getByRole("radio", { name: /single experiment/i })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /n experiments/i })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /until budget/i })).toBeInTheDocument();
  });

  it("selecting 'once' keeps only the three radio options visible (no extra inputs)", () => {
    renderPicker("once");
    // No count or budget input rendered for "once" mode
    expect(screen.queryByRole("spinbutton", { name: /count/i })).toBeNull();
    expect(screen.queryByRole("spinbutton", { name: /budget/i })).toBeNull();
  });

  it("selecting 'N experiments' shows count input", async () => {
    const user = userEvent.setup();
    renderPicker("once");
    const nExpRadio = screen.getByRole("radio", { name: /n experiments/i });
    await user.click(nExpRadio);
    expect(screen.getByRole("spinbutton", { name: /count/i })).toBeInTheDocument();
  });

  it("selecting 'Until budget' shows budget field", async () => {
    const user = userEvent.setup();
    renderPicker("once");
    const budgetRadio = screen.getByRole("radio", { name: /until budget/i });
    await user.click(budgetRadio);
    expect(screen.getByRole("spinbutton", { name: /budget/i })).toBeInTheDocument();
  });

  it("calls onChange with 'once' when Single experiment is selected", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("n_experiments", onChange);
    const onceRadio = screen.getByRole("radio", { name: /single experiment/i });
    await user.click(onceRadio);
    expect(onChange).toHaveBeenCalledWith("once", undefined, undefined);
  });

  it("calls onChange with 'n_experiments' and count when N experiments selected", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("n_experiments", onChange);
    // Already showing count input since controlled as n_experiments
    const countInput = screen.getByRole("spinbutton", { name: /count/i });
    await user.clear(countInput);
    await user.type(countInput, "5");
    // Expect onChange called with count=5
    expect(onChange).toHaveBeenLastCalledWith("n_experiments", 5, undefined);
  });

  it("calls onChange with 'until_budget' and budget when Until budget selected", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("until_budget", onChange);
    // Already showing budget input since controlled as until_budget
    const budgetInput = screen.getByRole("spinbutton", { name: /budget/i });
    await user.clear(budgetInput);
    await user.type(budgetInput, "2.5");
    expect(onChange).toHaveBeenLastCalledWith("until_budget", undefined, 2.5);
  });

  it("shows validation error for empty count in n_experiments mode", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("n_experiments", onChange);
    const countInput = screen.getByRole("spinbutton", { name: /count/i });
    await user.clear(countInput);
    await user.tab(); // blur to trigger validation
    expect(screen.getByText(/count must be/i)).toBeInTheDocument();
  });

  it("shows validation error for count < 1 in n_experiments mode", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("n_experiments", onChange);
    const countInput = screen.getByRole("spinbutton", { name: /count/i });
    await user.clear(countInput);
    await user.type(countInput, "0");
    await user.tab();
    expect(screen.getByText(/count must be/i)).toBeInTheDocument();
  });

  it("shows validation error for empty budget in until_budget mode", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("until_budget", onChange);
    const budgetInput = screen.getByRole("spinbutton", { name: /budget/i });
    await user.clear(budgetInput);
    await user.tab();
    expect(screen.getByText(/budget must be/i)).toBeInTheDocument();
  });

  it("shows validation error for negative budget in until_budget mode", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderPicker("until_budget", onChange);
    const budgetInput = screen.getByRole("spinbutton", { name: /budget/i });
    await user.clear(budgetInput);
    await user.type(budgetInput, "-1");
    await user.tab();
    expect(screen.getByText(/budget must be/i)).toBeInTheDocument();
  });

  it("does not render a popup/modal — is an inline form only", () => {
    renderPicker("once");
    // No dialog/modal roles
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
