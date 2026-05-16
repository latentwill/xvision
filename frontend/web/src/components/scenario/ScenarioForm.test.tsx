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

  it("requires a non-empty scenario display name before submit", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "   " },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2024-01-01" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2024-01-03" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(screen.getByText("Scenario display name is required.")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("coerces an unsupported initial granularity to the default", () => {
    const onSubmit = vi.fn();

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        initial={{ granularity: "2w" } as Partial<CreateScenarioRequest>}
      />,
    );

    const select = screen.getByLabelText("Granularity") as HTMLSelectElement;
    expect(select.value).toBe("1h");
  });

  it("renders granularity as a native select with all supported options", () => {
    render(<ScenarioForm onSubmit={vi.fn()} />);

    const select = screen.getByLabelText("Granularity") as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");

    const optionValues = Array.from(select.options).map((o) => o.value);
    expect(optionValues).toEqual([
      "1m",
      "5m",
      "15m",
      "30m",
      "1h",
      "4h",
      "6h",
      "12h",
      "1d",
      "1w",
      "1mo",
      "3mo",
      "6mo",
      "12mo",
    ]);
  });

  it("flows granularity selection through draft and submit", () => {
    const onSubmit = vi.fn();
    const onDraftChange = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} onDraftChange={onDraftChange} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 1d" },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2024-01-01" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2024-01-10" },
    });
    fireEvent.change(screen.getByLabelText("Granularity"), {
      target: { value: "1d" },
    });

    expect(onDraftChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ granularity: "1d" }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Create →" }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ granularity: "1d" }),
    );
  });

  it("requires the end date to be after the start date", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH reversed" },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2024-01-03" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2024-01-01" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(screen.getByText("End date must be after start date.")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("submits advanced fee, slippage, and latency fields", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH advanced" },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2024-01-01" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2024-01-03" },
    });
    fireEvent.click(screen.getByRole("button", { name: "▸ Advanced" }));
    fireEvent.change(screen.getByLabelText("Fees taker (bps)"), {
      target: { value: "31" },
    });
    fireEvent.change(screen.getByLabelText("Slippage (linear bps)"), {
      target: { value: "7" },
    });
    fireEvent.change(screen.getByLabelText("Latency (ms)"), {
      target: { value: "750" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        venue: expect.objectContaining({
          fees: expect.objectContaining({ taker_bps: 31 }),
          slippage: expect.objectContaining({ bps: 7 }),
          latency: expect.objectContaining({ decision_to_fill_ms: 750 }),
        }),
      }),
    );
  });
});
