// frontend/web/src/features/agent-runs/DecisionJump.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DecisionJump } from "./DecisionJump";

const decisions = [11, 12, 13, 14, 15, 16, 17, 18].map((i) => ({ i }));

describe("DecisionJump", () => {
  test('renders "of N" when inactive', () => {
    render(<DecisionJump value="all" onChange={() => {}} decisions={decisions} />);
    expect(screen.getByText(/of 8/)).toBeInTheDocument();
  });

  test('renders "k/N" position when active', () => {
    render(<DecisionJump value="14" onChange={() => {}} decisions={decisions} />);
    expect(screen.getByText(/4\/8/)).toBeInTheDocument();
  });

  test("Enter commits the typed value (snaps to nearest existing decision)", async () => {
    const onChange = vi.fn();
    render(<DecisionJump value="all" onChange={onChange} decisions={decisions} />);
    const input = screen.getByPlaceholderText("—");
    await userEvent.type(input, "99{Enter}");
    expect(onChange).toHaveBeenLastCalledWith("18"); // nearest to 99
  });

  test("ArrowUp / ArrowDown steps through decisions", async () => {
    const onChange = vi.fn();
    render(<DecisionJump value="14" onChange={onChange} decisions={decisions} />);
    const input = screen.getByPlaceholderText("—");
    input.focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(onChange).toHaveBeenLastCalledWith("15");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(onChange).toHaveBeenLastCalledWith("13");
  });

  test("Escape clears the active filter", async () => {
    const onChange = vi.fn();
    render(<DecisionJump value="14" onChange={onChange} decisions={decisions} />);
    const input = screen.getByPlaceholderText("—");
    input.focus();
    await userEvent.keyboard("{Escape}");
    expect(onChange).toHaveBeenCalledWith("all");
  });
});
