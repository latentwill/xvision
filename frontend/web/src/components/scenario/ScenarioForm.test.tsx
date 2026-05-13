import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { CreateScenarioRequest } from "@/api/types.gen";
import { ScenarioForm } from "./ScenarioForm";

afterEach(() => {
  cleanup();
});

describe("ScenarioForm", () => {
  it("submits hour scenarios from the granularity control", () => {
    const onSubmit = vi.fn();
    const onDraftChange = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} onDraftChange={onDraftChange} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 4H range" },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2024-01-01" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2024-01-03" },
    });
    fireEvent.change(screen.getByLabelText("Granularity"), {
      target: { value: "4h" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "ETH 4H range",
        granularity: "4h",
      } satisfies Partial<CreateScenarioRequest>),
    );
    expect(onDraftChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ granularity: "4h" }),
    );
  });

  it("submits minute scenarios from the granularity control", () => {
    const onSubmit = vi.fn();
    const onDraftChange = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} onDraftChange={onDraftChange} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 15m range" },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2024-01-01" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2024-01-03" },
    });
    fireEvent.change(screen.getByLabelText("Granularity"), {
      target: { value: "15m" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "ETH 15m range",
        granularity: "15m",
      } satisfies Partial<CreateScenarioRequest>),
    );
    expect(onDraftChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ granularity: "15m" }),
    );
  });
});
