import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { CreateScenarioRequest } from "@/api/types.gen";
import { ScenarioForm, estimateBars } from "./ScenarioForm";

afterEach(() => {
  cleanup();
});

// The From/To inputs are no longer native `<input type="date">` — they're
// the InlineRangeBar component. These tests don't exercise the picker
// directly; they just need a non-empty `from`/`to` so submit can succeed.
// `withDateRange` returns an `initial` prop that pre-populates the state
// without going through the picker UI.
function withDateRange(
  start = "2024-01-01",
  end = "2024-01-03",
): Partial<CreateScenarioRequest> {
  return {
    time_window: {
      start: `${start}T00:00:00Z`,
      end: `${end}T00:00:00Z`,
    },
  };
}

describe("ScenarioForm", () => {
  it("submits hour scenarios from the granularity control", () => {
    const onSubmit = vi.fn();
    const onDraftChange = vi.fn();

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        onDraftChange={onDraftChange}
        initial={withDateRange()}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 4H range" },
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

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        onDraftChange={onDraftChange}
        initial={withDateRange()}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 15m range" },
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

    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "   " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(screen.getByText("Scenario display name is required.")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it.each([
    ["Minute1", "1m"],
    ["Minute5", "5m"],
    ["Minute15", "15m"],
    ["Hour1", "1h"],
    ["Hour4", "4h"],
    ["Hour6", "6h"],
    ["Hour12", "12h"],
    ["Day1", "1d"],
    ["Week1", "1w"],
    ["Month1", "1mo"],
    ["Month3", "3mo"],
    ["1Min", "1m"],
    ["5Min", "5m"],
    ["1Hour", "1h"],
    ["12Hour", "12h"],
    ["1Day", "1d"],
    ["1Week", "1w"],
    ["1Month", "1mo"],
    ["12Month", "12mo"],
  ])("normalizes legacy granularity %s to %s", (legacy, canonical) => {
    render(
      <ScenarioForm
        onSubmit={vi.fn()}
        initial={{ granularity: legacy } as Partial<CreateScenarioRequest>}
      />,
    );
    const select = screen.getByLabelText("Granularity") as HTMLSelectElement;
    expect(select.value).toBe(canonical);
  });

  it("renders backend-valid out-of-palette values as a synthetic option without coercion", () => {
    const onSubmit = vi.fn();
    render(
      <ScenarioForm
        onSubmit={onSubmit}
        initial={{
          granularity: "2mo",
          ...withDateRange("2024-01-01", "2024-06-01"),
        } as Partial<CreateScenarioRequest>}
      />,
    );
    const select = screen.getByLabelText("Granularity") as HTMLSelectElement;
    expect(select.value).toBe("2mo");
    expect(Array.from(select.options).map((o) => o.value)).toContain("2mo");

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "ETH 2mo" } });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ granularity: "2mo" }),
    );
  });

  it.each([
    "2w",       // Week amount must be 1 on the backend
    "5mo",      // Month amount must be in {1, 2, 3, 4, 6, 12}
    "garbage",  // unrecognized form
    "",         // empty
  ])("coerces unsupported initial granularity %s to the 1h default", (bad) => {
    render(
      <ScenarioForm
        onSubmit={vi.fn()}
        initial={{ granularity: bad } as Partial<CreateScenarioRequest>}
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

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        onDraftChange={onDraftChange}
        initial={withDateRange("2024-01-01", "2024-01-10")}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 1d" },
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

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        initial={withDateRange("2024-01-03", "2024-01-01")}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH reversed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(screen.getByText("End date must be after start date.")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("submits advanced fee, slippage, and latency fields", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH advanced" },
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

  it("preserves initial linear slippage when the field is not edited", () => {
    const onSubmit = vi.fn();

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        initial={{
          ...withDateRange(),
          venue: { slippage: { model: "linear", bps: 25 } },
        } as Partial<CreateScenarioRequest>}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH cloned linear slippage" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        venue: expect.objectContaining({
          slippage: { model: "linear", bps: 25 },
        }),
      }),
    );
  });

  it("preserves initial none slippage when the field is not edited", () => {
    const onSubmit = vi.fn();

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        initial={{
          ...withDateRange(),
          venue: { slippage: { model: "none" } },
        } as Partial<CreateScenarioRequest>}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH cloned no slippage" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        venue: expect.objectContaining({
          slippage: { model: "none" },
        }),
      }),
    );
  });

  // ── q15-scenario-warmup-bars ─────────────────────────────────────────
  // The "Context bars" field is the operator surface for the warmup
  // window. These tests pin: (1) the default value, (2) round-trip
  // through onSubmit, and (3) the helper text mentioning the
  // strategy's min_warmup_bars guidance.

  it("scenario-form-warmup: defaults Context bars to 200 and submits it", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);

    const warmupField = screen.getByLabelText("Context bars") as HTMLInputElement;
    expect(warmupField.value).toBe("200");

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH default warmup" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        warmup_bars: 200,
      } satisfies Partial<CreateScenarioRequest>),
    );
  });

  it("scenario-form-warmup: round-trips a custom Context bars value through onSubmit", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH custom warmup" },
    });
    fireEvent.change(screen.getByLabelText("Context bars"), {
      target: { value: "50" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        warmup_bars: 50,
      } satisfies Partial<CreateScenarioRequest>),
    );
  });

  it("scenario-form-warmup: surfaces strategy-min_warmup_bars helper text", () => {
    render(<ScenarioForm onSubmit={vi.fn()} />);
    // Helper copy lives directly under the Context bars input. Match
    // a stable substring so future copy tweaks don't snap the test.
    expect(
      screen.getByText(/Bars pre-fetched before the scenario window/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/longest indicator period/i),
    ).toBeInTheDocument();
  });

  // ── Calendar select (scenario-form-date-range-picker) ────────────────

  it("defaults the calendar to Continuous24x7 and submits that shape", () => {
    const onSubmit = vi.fn();
    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH default calendar" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ calendar: "Continuous24x7" }),
    );
  });

  it("submits UsEquities when the calendar select switches", () => {
    const onSubmit = vi.fn();
    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH usequities" },
    });
    fireEvent.change(screen.getByLabelText("Calendar"), {
      target: { value: "UsEquities" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ calendar: "UsEquities" }),
    );
  });

  it("reveals the custom-id input when Custom is picked and submits the typed shape", () => {
    const onSubmit = vi.fn();
    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH custom calendar" },
    });
    fireEvent.change(screen.getByLabelText("Calendar"), {
      target: { value: "Custom" },
    });
    const customInput = screen.getByLabelText("Custom calendar id");
    fireEvent.change(customInput, { target: { value: "nyse-extended-hours" } });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        calendar: { Custom: "nyse-extended-hours" },
      }),
    );
  });
});

describe("estimateBars", () => {
  // (a) Time-window only — operator picked dates but left context at 0.
  it("time-window only adds zero context bars", () => {
    expect(estimateBars("2024-01-01", "2024-01-03", "1d", 0)).toBe(2);
  });

  // (b) Context bars only — operator typed a context value before
  // picking a time window. The estimate must NOT stay at 0 just
  // because from/to are blank.
  it("context bars only (empty time window) returns the context count", () => {
    expect(estimateBars("", "", "1d", 100)).toBe(100);
  });

  // (c) Both summed — the live form case.
  it("sums time-window bars and context bars", () => {
    expect(estimateBars("2024-01-01", "2024-01-03", "1d", 100)).toBe(102);
  });

  // (d) Zero context degrades to the time-window-only number.
  it("zero / negative / NaN context bars degrade cleanly", () => {
    expect(estimateBars("2024-01-01", "2024-01-03", "1d", 0)).toBe(2);
    expect(estimateBars("2024-01-01", "2024-01-03", "1d", -5)).toBe(2);
    expect(estimateBars("2024-01-01", "2024-01-03", "1d", Number.NaN)).toBe(2);
  });
});

describe("ScenarioForm — estimated bars UI", () => {
  it("updates the live estimate when Context bars input changes", () => {
    // Operator repro from team/intake/2026-05-18-qa-operator-round-3.md:
    // "Added 100 bars context in scenario, but it still says
    // Estimated bars to fetch: 0." Without a time window picked
    // the estimate must still react to the Context bars input.
    render(<ScenarioForm onSubmit={vi.fn()} />);

    const contextInput = screen.getByLabelText("Context bars");
    fireEvent.change(contextInput, { target: { value: "100" } });

    // The estimate node renders the formatted number; use a regex
    // anchored on "Estimated bars to fetch" so the label match
    // survives copy tweaks elsewhere on the form.
    const estimate = screen.getByText(/Estimated bars to fetch:/i).parentElement;
    expect(estimate).toBeTruthy();
    expect(estimate!.textContent).toMatch(/100/);
    expect(estimate!.textContent).not.toMatch(/Estimated bars to fetch:\s*0\b/);
  });
});
