import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
  it("submits with the hard-coded 1h granularity default when none supplied", () => {
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
      target: { value: "ETH default" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "ETH default",
        granularity: "1h",
      } satisfies Partial<CreateScenarioRequest>),
    );
    expect(onDraftChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ granularity: "1h" }),
    );
  });

  it("passes a supported `initial.granularity` through to submit", () => {
    const onSubmit = vi.fn();
    const onDraftChange = vi.fn();

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        onDraftChange={onDraftChange}
        initial={{ ...withDateRange(), granularity: "15m" } as Partial<CreateScenarioRequest>}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH 15m range" },
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

  it("submits a typed tag draft without requiring Enter", () => {
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH tagged" },
    });
    fireEvent.change(screen.getByPlaceholderText("+ add tag"), {
      target: { value: "  momentum  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        tags: ["momentum"],
      } satisfies Partial<CreateScenarioRequest>),
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
    "Hour4",
    "2mo",
    "garbage",
    "",
  ])(
    "coerces unsupported initial granularity %s to the 1h default on submit",
    (bad) => {
      const onSubmit = vi.fn();
      render(
        <ScenarioForm
          onSubmit={onSubmit}
          initial={{ ...withDateRange(), granularity: bad } as Partial<CreateScenarioRequest>}
        />,
      );
      fireEvent.change(screen.getByLabelText("Name"), { target: { value: "x" } });
      fireEvent.click(screen.getByRole("button", { name: "Create →" }));
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({ granularity: "1h" }),
      );
    },
  );

  it("does not render a user-facing granularity control", () => {
    render(<ScenarioForm onSubmit={vi.fn()} />);
    expect(screen.queryByLabelText("Granularity")).not.toBeInTheDocument();
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

  it("selects calendar kind through the styled menu", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();

    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);

    await user.click(screen.getByRole("button", { name: "Calendar" }));
    await user.click(await screen.findByRole("option", { name: /US equities/i }));
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "US equity calendar" },
    });
    await user.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        calendar: "UsEquities",
      } satisfies Partial<CreateScenarioRequest>),
    );
  });

  it("preserves initial venue overrides when submitting a cloned scenario", () => {
    const onSubmit = vi.fn();
    const overrides = [
      {
        symbol_pattern: "ETH/*",
        fees: { maker_bps: 2, taker_bps: 3 },
        slippage: { model: "none" },
      },
    ];

    render(
      <ScenarioForm
        onSubmit={onSubmit}
        initial={{
          ...withDateRange(),
          venue: { overrides },
        } as Partial<CreateScenarioRequest>}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH cloned venue overrides" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        venue: expect.objectContaining({ overrides }),
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

  it("submits UsEquities when the calendar select switches", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH usequities" },
    });
    await user.click(screen.getByRole("button", { name: "Calendar" }));
    await user.click(await screen.findByRole("option", { name: /US equities/i }));
    fireEvent.click(screen.getByRole("button", { name: "Create →" }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ calendar: "UsEquities" }),
    );
  });

  it("reveals the custom-id input when Custom is picked and submits the typed shape", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<ScenarioForm onSubmit={onSubmit} initial={withDateRange()} />);
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "ETH custom calendar" },
    });
    await user.click(screen.getByRole("button", { name: "Calendar" }));
    await user.click(await screen.findByRole("option", { name: /Custom/i }));
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
