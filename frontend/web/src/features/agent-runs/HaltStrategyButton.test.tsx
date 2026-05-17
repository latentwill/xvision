// frontend/web/src/features/agent-runs/HaltStrategyButton.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HaltStrategyButton } from "./HaltStrategyButton";

describe("HaltStrategyButton", () => {
  test("clicking once reveals the inline confirm input, not a popup", async () => {
    render(<HaltStrategyButton strategyName="btc_mr" onHalt={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /halt strategy/i }));
    expect(screen.getByPlaceholderText(/type btc_mr to confirm/i)).toBeInTheDocument();
    // The confirm row is part of the same tree, not a portal/dialog.
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("submit disabled until typed name matches", async () => {
    render(<HaltStrategyButton strategyName="btc_mr" onHalt={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /halt strategy/i }));
    const input = screen.getByPlaceholderText(/type btc_mr to confirm/i);
    const submit = screen.getByRole("button", { name: /^halt$/i });
    expect(submit).toBeDisabled();
    await userEvent.type(input, "btc_mr");
    expect(submit).toBeEnabled();
  });

  test("calling submit invokes onHalt", async () => {
    const onHalt = vi.fn();
    render(<HaltStrategyButton strategyName="btc_mr" onHalt={onHalt} />);
    await userEvent.click(screen.getByRole("button", { name: /halt strategy/i }));
    await userEvent.type(screen.getByPlaceholderText(/type btc_mr to confirm/i), "btc_mr");
    await userEvent.click(screen.getByRole("button", { name: /^halt$/i }));
    expect(onHalt).toHaveBeenCalledOnce();
  });
});
