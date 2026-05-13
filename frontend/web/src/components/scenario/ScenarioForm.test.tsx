import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { CreateScenarioRequest } from "@/api/types.gen";
import { ScenarioForm } from "./ScenarioForm";

describe("ScenarioForm", () => {
  it("submits Hour4 scenarios from the granularity controls", () => {
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
    fireEvent.click(screen.getByLabelText("4h"));
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "ETH 4H range",
        granularity: "Hour4",
      } satisfies Partial<CreateScenarioRequest>),
    );
    expect(onDraftChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ granularity: "Hour4" }),
    );
  });
});
