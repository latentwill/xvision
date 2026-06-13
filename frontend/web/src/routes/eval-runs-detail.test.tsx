import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Link, MemoryRouter, Route, Routes } from "react-router-dom";

import { EvalRunDetailRoute } from "./eval-runs-detail";
import { ApiError } from "@/api/client";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as evalReviewApi from "@/api/eval-review";
import * as scenariosApi from "@/api/scenarios";
import * as settingsApi from "@/api/settings";
import * as strategyApi from "@/api/strategies";
import { useTraceDock } from "@/stores/trace-dock";
import type { DecisionRowDto, FilterEventV1, RunDetail } from "@/api/types.gen";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    getRun: vi.fn(),
    cancelRun: vi.fn(),
    downloadEvalRunExport: vi.fn(),
    retryRun: vi.fn(),
    listRuns: vi.fn(),
    deleteRun: vi.fn(),
  };
});

vi.mock("@/api/eval-review", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval-review")>(
    "@/api/eval-review",
  );
  return {
    ...actual,
    listReviewsForRun: vi.fn(),
    getReview: vi.fn(),
    generateReview: vi.fn(),
    listAgentProfiles: vi.fn(),
    updateAgentProfile: vi.fn(),
  };
});

vi.mock("@/api/chart", () => ({
  chartKeys: {
    run: (id: string) => ["chart", "run", id],
  },
  getRunChart: vi.fn(),
  openRunStream: vi.fn((runId: string) => new EventSource(`/stream/${runId}`)),
}));

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>(
    "@/api/scenarios",
  );
  return {
    ...actual,
    listScenarios: vi.fn(),
  };
});

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    listProviders: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    listStrategies: vi.fn(),
  };
});

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  listeners = new Map<string, Set<(ev: MessageEvent) => void>>();
  closed = false;

  constructor(public url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(name: string, cb: (ev: MessageEvent) => void) {
    const listeners = this.listeners.get(name) ?? new Set();
    listeners.add(cb);
    this.listeners.set(name, listeners);
  }

  removeEventListener(name: string, cb: (ev: MessageEvent) => void) {
    this.listeners.get(name)?.delete(cb);
  }

  close() {
    this.closed = true;
  }

  emit(name: string, payload: unknown) {
    const ev = { data: JSON.stringify(payload) } as MessageEvent;
    this.listeners.get(name)?.forEach((cb) => cb(ev));
  }
}

function renderDetail() {
  return render(
    <MemoryRouter initialEntries={["/eval-runs/01LIVE"]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <Routes>
          <Route path="/eval-runs/:runId" element={<EvalRunDetailRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

function decision(overrides: Partial<DecisionRowDto> = {}): DecisionRowDto {
  return {
    decision_index: 0,
    timestamp: "2026-05-13T15:00:00Z",
    asset: "BTC/USD",
    action: "long_open",
    conviction: 0.77,
    justification: "breakout confirmed",
    reasoning: null,
    order_size: 0.1,
    fill_price: 69000,
    fill_size: 0.1,
    fee: 0.25,
    pnl_realized: null,
    ...overrides,
  };
}

function makeReview(
  overrides: Partial<evalReviewApi.EvalReview> = {},
): evalReviewApi.EvalReview {
  return {
    id: "01REVIEW",
    eval_run_id: "01LIVE",
    agent_profile_id: "reasoning-agent",
    status: "completed",
    verdict: "promising",
    confidence: 0.72,
    score: 75,
    summary: "Looks plausible.",
    raw_output_json: JSON.stringify({
      risks: ["concentration risk"],
      next_tests: ["test on longer window", "stress test", "out-of-sample"],
      questions: ["does this survive 2022 chop?"],
    }),
    error: null,
    created_at: "2026-05-13T14:01:30Z",
    updated_at: "2026-05-13T14:02:00Z",
    ...overrides,
  };
}

function detail(overrides: Partial<RunDetail> = {}): RunDetail {
  return {
    summary: {
      id: "01LIVE",
      agent_id: "01AGENT",
      scenario_id: "btc-4h",
      strategy: null,
      scenario: null,
      mode: "backtest",
      status: "running",
      started_at: "2026-05-13T14:00:00Z",
      completed_at: null,
      sharpe: null,
      max_drawdown_pct: null,
      total_return_pct: null,
      actual_input_tokens: null,
      actual_output_tokens: null,
      error: null,
      inference_cost_quote_total: null,
      net_return_pct: null,
      filter_summaries: [],
      auto_fire_review: false,
      review_model: null,
      max_annotations_per_review: 8,
      paused: false,
      paused_at: null,
      flatten_requested: false,
    },
    decisions: [],
    equity_curve: [],
    filter_events: [],
    filter_summaries: [],
    ...overrides,
  };
}

describe("EvalRunDetailRoute", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    vi.mocked(evalApi.cancelRun).mockResolvedValue({
      ...detail().summary,
      status: "cancelled",
      completed_at: "2026-05-13T14:01:00Z",
      error: "cancelled by user",
    });
    // siblings query: default empty so the disambiguator falls back to
    // "Run #1 · …". Individual tests override when ordinal matters.
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(chartApi.openRunStream).mockImplementation(
      (runId: string) => new EventSource(`/stream/${runId}`),
    );
    // ReviewPanel queries listReviewsForRun whenever runIsCompleted is
    // true; default to an empty list so tests that don't care about the
    // review surface don't have to set this up.
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([]);
    vi.mocked(evalReviewApi.getReview).mockResolvedValue({
      review: makeReview(),
      findings: [],
    });
    vi.mocked(evalReviewApi.generateReview).mockResolvedValue({
      review: makeReview(),
      findings: [],
    });
    const reviewProfiles = evalReviewApi.CANONICAL_AGENT_PROFILES.map((p) => ({
      id: p.id,
      name: p.label,
      type: "review",
      provider: "openrouter",
      model: "anthropic/claude-sonnet-4.5",
      temperature: 0.2,
      max_tokens: 4096,
      system_prompt: `Review prompt for ${p.label}.`,
      enabled: true,
      created_at: "2026-05-23T00:00:00Z",
      updated_at: "2026-05-23T00:00:00Z",
    }));
    vi.mocked(evalReviewApi.listAgentProfiles).mockResolvedValue(reviewProfiles);
    vi.mocked(evalReviewApi.updateAgentProfile).mockImplementation(
      async (id, patch) => ({
        ...reviewProfiles.find((p) => p.id === id)!,
        ...patch,
      }),
    );
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        {
          name: "openrouter",
          kind: "openai-compat",
          base_url: "https://openrouter.ai/api/v1",
          api_key_env: "OPENROUTER_API_KEY",
          api_key_set: true,
          synthetic: false,
          is_default: true,
          enabled_models: ["google/gemini-3.1-flash-lite"],
        },
      ],
    } as any);
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01AGENT",
        display_name: "BTC Momentum",
        template: "momentum",
        decision_cadence_minutes: 60,
      },
    ]);
    vi.mocked(scenariosApi.listScenarios).mockResolvedValue([
      {
        id: "btc-4h",
        display_name: "BTC 4h breakout",
      } as any,
    ]);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("appends streamed decisions while a run is active", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    // Empty-state copy in the redesigned Decisions card.
    await screen.findByText("No decisions");
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    FakeEventSource.instances[0].emit("decision", {
      event: "decision",
      data: decision(),
    });

    // The Signal table renders a direction-aware ActionPill (long_open → BUY)
    // and the engaged PhaseChip. Conviction renders as a rounded percentage
    // (0.77 → 77%) in the redesigned table.
    expect((await screen.findAllByText("BUY")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("ENGAGED").length).toBeGreaterThan(0);
    expect(screen.getByText("77%")).toBeInTheDocument();
  });

  // The filter-event timeline is fed by `detail.filter_events`, which only
  // refreshes via `GET /runs/:id`. SSE does not carry per-bar filter events,
  // so without a streaming-side refetch nudge the strip lags the 2s adaptive
  // poll (and can sit stale until a terminal status invalidates the cache).
  // useLiveRunStream debounces a refetch off every decision/status event so
  // the strip grows in step with streamed decisions.
  it("grows the filter timeline as decisions stream in", async () => {
    const filterEvent = (i: number): FilterEventV1 => ({
      schema_version: 1,
      bar_timestamp: `2026-05-13T15:0${i}:00Z`,
      filter_id: "01FILTER",
      triggered: i % 2 === 0,
      suppressed_reason: null,
      conditions_passed: [],
      conditions_failed: [],
      indicator_snapshot: {},
    });
    let calls = 0;
    vi.mocked(evalApi.getRun).mockImplementation(async () => {
      const next = detail({
        filter_events: Array.from({ length: calls }, (_, i) => filterEvent(i)),
      });
      calls += 1;
      return next;
    });

    renderDetail();

    // First render: filter_events length 0 → timeline section is hidden.
    await screen.findByText("No decisions");
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(screen.queryByTestId("filter-event-timeline")).toBeNull();

    const es = FakeEventSource.instances[0];

    for (let i = 1; i <= 3; i += 1) {
      es.emit("decision", {
        event: "decision",
        data: decision({ decision_index: i - 1 }),
      });
      // The strip lives in `detail.filter_events`, which only refreshes via
      // `GET /runs/:id`. Each emit schedules a debounced refetch; wait for
      // the resulting render rather than the request itself so the test
      // stays insensitive to the exact debounce timing.
      await waitFor(() =>
        expect(screen.getAllByTestId("filter-event-tick")).toHaveLength(i),
      );
    }
  });

  it("keeps rendering cached run detail when a live refetch gets a transient not_found", async () => {
    vi.mocked(evalApi.getRun)
      .mockResolvedValueOnce(detail())
      .mockRejectedValueOnce(new ApiError(404, "not_found", "eval run '01LIVE'"));

    renderDetail();

    await screen.findByRole("button", { name: /stop eval run/i });
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    (FakeEventSource.instances[0] as unknown as { onerror?: (ev: Event) => void }).onerror?.(
      new Event("error"),
    );

    await waitFor(() => expect(evalApi.getRun).toHaveBeenCalledTimes(2));
    expect(screen.getByTestId("eval-run-id")).toHaveTextContent("01LIVE");
    expect(screen.queryByText("Run not found")).not.toBeInTheDocument();
  });

  it("shows an explicit stop control for active runs", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    const stop = await screen.findByRole("button", {
      name: "Stop eval run 01LIVE",
    });
    expect(stop).toHaveTextContent("Stop eval");

    fireEvent.click(stop);

    await waitFor(() => expect(evalApi.cancelRun).toHaveBeenCalled());
    expect(vi.mocked(evalApi.cancelRun).mock.calls[0]?.[0]).toBe("01LIVE");
  });

  it("hides Download JSON while a run is still active", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    // Confirm the page rendered (queued/running render the Stop button).
    await screen.findByRole("button", { name: /stop eval run/i });
    expect(
      screen.queryByRole("button", { name: /download .* json/i }),
    ).not.toBeInTheDocument();
  });

  it("offers Download JSON on terminal runs and routes through the export helper", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    vi.mocked(evalApi.downloadEvalRunExport).mockResolvedValue();

    renderDetail();

    const download = await screen.findByRole("button", {
      name: /download eval run 01LIVE as json/i,
    });
    fireEvent.click(download);

    await waitFor(() =>
      expect(evalApi.downloadEvalRunExport).toHaveBeenCalledWith("01LIVE"),
    );
  });

  it("derives ENGAGED vs FILTERED phase from synthesized-row markers", async () => {
    // The Signal redesign replaces the old Decision-provenance panel with a
    // PHASE column. Synthesized rows (noop_skip / early-stop markers) derive to
    // FILTERED; a real trader decision derives to ENGAGED. No backend phase
    // field — phase is computed in the adapter from existing fields.
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        decisions: [
          // Each fixture row gets a distinct timestamp so it represents its
          // own decision step. The PHASE chip dedupe collapses per-asset
          // child rows of the SAME step, which isn't what this test is
          // exercising — the contract here is phase derivation from the
          // synthesized-row markers, one row per step.
          decision({
            decision_index: 0,
            timestamp: "2026-05-13T15:00:00Z",
            justification: "breakout confirmed",
          }),
          decision({
            decision_index: 1,
            timestamp: "2026-05-13T16:00:00Z",
            action: "hold",
            conviction: null,
            order_size: null,
            justification: "noop_skip: only hold is available",
            reasoning: null,
          }),
          decision({
            decision_index: 2,
            timestamp: "2026-05-13T17:00:00Z",
            action: "flat",
            conviction: null,
            order_size: null,
            justification: "inherited from early-stop policy",
            reasoning: null,
          }),
        ],
      }),
    );

    renderDetail();

    // One engaged (the breakout) + two no-op (the synthesized rows).
    expect((await screen.findAllByText("ENGAGED")).length).toBe(1);
    expect(screen.getAllByText("NO-OP").length).toBeGreaterThanOrEqual(2);
  });

  it("dims filtered rows and dashes out their engaged-only cells", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        decisions: [
          // Distinct timestamps so each row is its own step — the dimming /
          // dash-out contract is per-row, separate from the PHASE-chip
          // per-step dedupe.
          decision({
            decision_index: 0,
            timestamp: "2026-05-13T15:00:00Z",
            justification: "breakout confirmed",
          }),
          decision({
            decision_index: 1,
            timestamp: "2026-05-13T16:00:00Z",
            action: "hold",
            conviction: null,
            order_size: null,
            justification: "noop_skip: only hold is available",
            reasoning: null,
          }),
        ],
      }),
    );

    renderDetail();

    const filteredChip = await screen.findByText("NO-OP");
    const row = filteredChip.closest("tr");
    expect(row).not.toBeNull();
    // No-op rows render at reduced opacity (0.78) and replace engaged-only
    // cells with em dashes.
    expect(row?.getAttribute("style") ?? "").toMatch(/opacity:\s*0\.78/);
    expect((row?.querySelectorAll("td") ?? [])).not.toHaveLength(0);
    expect(row?.textContent ?? "").toMatch(/—/);
  });

  it("renders the per-decision density strip above the table", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        decisions: [
          decision({ decision_index: 0, justification: "breakout confirmed" }),
          decision({ decision_index: 1, action: "hold", conviction: 0.4, justification: "trim" }),
        ],
      }),
    );

    renderDetail();

    const strip = await screen.findByTestId("decision-density-strip");
    expect(strip).toBeInTheDocument();
    // Each decision is a full-height clickable tick (filtered ticks included).
    expect(
      strip.querySelectorAll('[role="button"][aria-label^="Jump to decision"]')
        .length,
    ).toBe(2);
  });

  it("renders the disambiguator label in the metadata strip and drops the strategy/scenario id chips", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      detail().summary,
      {
        ...detail().summary,
        id: "01OLDER",
        started_at: "2026-05-13T10:00:00Z",
      },
    ]);

    renderDetail();

    const meta = await screen.findByTestId("eval-run-meta");
    await waitFor(() =>
      expect(meta.textContent ?? "").toMatch(/Run #2\/2/),
    );
    // Full eval id is surfaced below the title (not inside the meta strip).
    // No truncation anywhere — the full ULID renders verbatim and is
    // `select-all` for easy copy. See QA22 / `eval-id-resurface-no-truncate`.
    const idEl = await screen.findByTestId("eval-run-id");
    expect(idEl.textContent).toBe("01LIVE");
    expect(idEl.getAttribute("aria-label")).toBe("Eval run id 01LIVE");
    // Meta no longer carries the run-id (it moved up). Strategy/scenario id
    // chips are also gone (they moved to display names).
    expect(meta.textContent ?? "").not.toMatch(/strategy 01AGENT/);
    expect(meta.textContent ?? "").not.toMatch(/scenario btc-4h/);
  });

  it("renders the action-row buttons as one quiet toolbar sharing the ACTION_BTN base", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "failed",
          completed_at: "2026-05-13T14:30:00Z",
          error: "boom",
        },
      }),
    );

    renderDetail();

    const actions = await screen.findByTestId("eval-run-actions");
    // Failed terminal run shows Retry + Download + Delete in the
    // same row (Delete added by qa-eval-action-lifecycle / #260).
    const buttons = actions.querySelectorAll("button");
    expect(buttons.length).toBe(3);
    // The action row reads as one quiet toolbar: every button shares the
    // ACTION_BTN base (soft #141414 border on the elevated surface, accent
    // only on hover). The earlier design forced a `min-w-[16ch]` uniform
    // floor + loud colored borders, which read as four chunky competing
    // boxes; that was deliberately removed in favor of content-sized
    // buttons with a shared base. Assert the shared base is present and the
    // old hard colored outlines are gone.
    for (const button of Array.from(buttons)) {
      // Rest state is the shared quiet base — soft border on the elevated
      // surface. Accent borders/tints are hover-only, so the rest className
      // begins with this exact prefix and never carries a loud at-rest box.
      expect(button.className).toMatch(
        /^inline-flex items-center gap-1\.5 rounded-sm border border-border-soft bg-surface-elev\b/,
      );
      expect(button.className).not.toContain("min-w-[16ch]");
    }
  });

  it("links the trace surface to the actual eval run id", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );

    renderDetail();

    const link = await screen.findByRole("link", { name: /view agent trace/i });
    expect(link).toHaveAttribute("href", "/agent-runs/01LIVE");
  });

  it("surfaces an inline error when the export helper rejects", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    vi.mocked(evalApi.downloadEvalRunExport).mockRejectedValue(
      new Error("server unreachable"),
    );

    renderDetail();

    const download = await screen.findByRole("button", {
      name: /download eval run 01LIVE as json/i,
    });
    fireEvent.click(download);

    expect(
      await screen.findByText(/download failed: server unreachable/i),
    ).toBeInTheDocument();
  });

  // ── review panel ──────────────────────────────────────────────────────

  it("hides the review panel while the run is still active", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());
    renderDetail();
    // The route renders a placeholder until status === "completed".
    await screen.findByRole("button", { name: /stop eval run/i });
    expect(
      screen.getByText(/reviews are available once the run is no longer active/i),
    ).toBeInTheDocument();
    // listReviewsForRun must not fire for non-completed runs.
    expect(evalReviewApi.listReviewsForRun).not.toHaveBeenCalled();
  });

  it("shows the empty state with an agent picker on a fresh completed run", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([]);

    renderDetail();

    expect(
      await screen.findByText(/no review yet for this run/i),
    ).toBeInTheDocument();
    const preset = screen.getByLabelText("Review prompt preset");
    expect(preset).toHaveTextContent("Fast Trader");
    expect(preset).toHaveTextContent("Reasoning");
    expect(preset).toHaveTextContent("Risk");
    expect(preset).toHaveTextContent("Research");
    expect(screen.queryByText(/claude-sonnet/i)).not.toBeInTheDocument();
  });

  it("calls generateReview with force=true when the operator picks an agent", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([]);

    renderDetail();

    const preset = await screen.findByLabelText("Review prompt preset");
    fireEvent.change(preset, { target: { value: "reasoning-agent" } });
    const button = screen.getByRole("button", { name: /generate review/i });
    await waitFor(() => expect(button).not.toBeDisabled());
    fireEvent.click(button);

    await waitFor(() =>
      expect(evalReviewApi.generateReview).toHaveBeenCalledWith("01LIVE", {
        agent_profile_id: "reasoning-agent",
        force: true,
      }),
    );
    expect(evalReviewApi.updateAgentProfile).toHaveBeenCalledWith(
      "reasoning-agent",
      {
        model: "google/gemini-3.1-flash-lite",
      },
    );
  });

  it("renders verdict + summary + sections + findings for a completed review", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    const review = makeReview();
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([review]);
    vi.mocked(evalReviewApi.getReview).mockResolvedValue({
      review,
      findings: [
        {
          id: "f1",
          run_id: "01LIVE",
          kind: "performance",
          severity: "medium",
          summary: "Modest sharpe",
          evidence: [{ kind: "metric", reference: "metric:sharpe" }],
          extracted_at: "2026-05-13T14:02:00Z",
          schema_version: "2",
          eval_review_id: "01REVIEW",
          type: "performance",
          confidence: 0.6,
          title: "Modest sharpe",
          description: "Sharpe 1.2 is modest given the 5% return.",
          recommendation: "Test on a longer window.",
        },
      ],
    });

    renderDetail();

    // Verdict badge + summary + section headers.
    expect(await screen.findByText("Promising")).toBeInTheDocument();
    expect(screen.getByText("Looks plausible.")).toBeInTheDocument();
    expect(screen.getByText("Executive summary")).toBeInTheDocument();
    expect(screen.getByText("Key findings")).toBeInTheDocument();
    expect(screen.getByText("Risks")).toBeInTheDocument();
    expect(screen.getByText("Recommended next tests")).toBeInTheDocument();
    expect(screen.getByText("Open questions")).toBeInTheDocument();
    // Risk + next-test bullet from raw_output_json.
    expect(screen.getByText("concentration risk")).toBeInTheDocument();
    expect(screen.getByText("test on longer window")).toBeInTheDocument();
    // Finding card renders title + recommendation.
    expect(screen.getByText("Modest sharpe")).toBeInTheDocument();
    expect(
      screen.getByText("Test on a longer window."),
    ).toBeInTheDocument();
  });

  it("does not leak the previous run's selected review when navigating to a new run", async () => {
    // Two completed runs, each with one review. Render run A first,
    // then navigate to run B (different :runId) and assert the panel
    // displays run B's review id — not run A's. The fix uses
    // `key={runId}` on ReviewPanel to remount the component on
    // navigation; without it, `selectedId` survives and pins the
    // panel to the previous run.
    const runADetail: RunDetail = detail({
      summary: {
        ...detail().summary,
        id: "01RUN_A",
        status: "completed",
        completed_at: "2026-05-13T14:01:00Z",
      },
    });
    const runBDetail: RunDetail = detail({
      summary: {
        ...detail().summary,
        id: "01RUN_B",
        status: "completed",
        completed_at: "2026-05-13T15:01:00Z",
      },
    });
    const reviewA = makeReview({
      id: "01REVIEW_A",
      eval_run_id: "01RUN_A",
      summary: "Review for run A.",
    });
    const reviewB = makeReview({
      id: "01REVIEW_B",
      eval_run_id: "01RUN_B",
      summary: "Review for run B.",
    });

    vi.mocked(evalApi.getRun).mockImplementation(async (id: string) =>
      id === "01RUN_A" ? runADetail : runBDetail,
    );
    vi.mocked(evalReviewApi.listReviewsForRun).mockImplementation(
      async (id: string) => (id === "01RUN_A" ? [reviewA] : [reviewB]),
    );
    vi.mocked(evalReviewApi.getReview).mockImplementation(
      async (reviewId: string) => ({
        review: reviewId === "01REVIEW_A" ? reviewA : reviewB,
        findings: [],
      }),
    );

    // Render-with-navigation helper: a child route + nav button so the
    // test can drive `useParams` updates without manually unmounting.
    function NavApp() {
      return (
        <MemoryRouter initialEntries={["/eval-runs/01RUN_A"]}>
          <QueryClientProvider
            client={
              new QueryClient({
                defaultOptions: { queries: { retry: false } },
              })
            }
          >
            <Routes>
              <Route
                path="/eval-runs/:runId"
                element={
                  <>
                    <Link to="/eval-runs/01RUN_B">go to B</Link>
                    <EvalRunDetailRoute />
                  </>
                }
              />
            </Routes>
          </QueryClientProvider>
        </MemoryRouter>
      );
    }

    render(<NavApp />);

    // Run A is current → its review summary renders.
    expect(await screen.findByText("Review for run A.")).toBeInTheDocument();

    // Navigate to B via a same-origin link click.
    fireEvent.click(screen.getByRole("link", { name: "go to B" }));

    // Run B's review must render, A's must be gone.
    expect(await screen.findByText("Review for run B.")).toBeInTheDocument();
    expect(
      screen.queryByText("Review for run A."),
    ).not.toBeInTheDocument();
  });

  it("surfaces a review list error inline with a retry control", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    vi.mocked(evalReviewApi.listReviewsForRun).mockRejectedValueOnce(
      new Error("reviews endpoint unreachable"),
    );

    renderDetail();

    // The error alert is rendered with role=alert. The picker stays
    // visible so the operator can still trigger a generate, but the
    // failure is not silent.
    expect(
      await screen.findByText(/couldn't load review history/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/reviews endpoint unreachable/i),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Review prompt preset")).toHaveTextContent(
      "Reasoning",
    );
  });

  it("renders the inconclusive-state explanation when verdict is inconclusive with no findings", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    const review = makeReview({
      verdict: "inconclusive",
      summary: "Payload was sparse.",
      raw_output_json: JSON.stringify({
        risks: [],
        next_tests: [],
        questions: [],
      }),
    });
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([review]);
    vi.mocked(evalReviewApi.getReview).mockResolvedValue({
      review,
      findings: [],
    });

    renderDetail();

    expect(await screen.findByText("Inconclusive")).toBeInTheDocument();
    expect(
      screen.getByText(/verdict was inconclusive — no findings were produced/i),
    ).toBeInTheDocument();
  });

  // ── status pill + running animation ──────────────────────────────────

  it("offers Retry on cancelled runs (alongside failed)", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "cancelled",
          completed_at: "2026-05-13T14:01:00Z",
          error: "cancelled by user",
        },
      }),
    );

    renderDetail();

    expect(
      await screen.findByRole("button", { name: "Retry eval run 01LIVE" }),
    ).toBeInTheDocument();
  });

  it("clicking Retry on a cancelled run requeues and navigates to the new run id", async () => {
    const cancelledDetail = detail({
      summary: {
        ...detail().summary,
        status: "cancelled",
        completed_at: "2026-05-13T14:01:00Z",
        error: "cancelled by user",
      },
    });
    const newRunDetail = detail({
      summary: {
        ...detail().summary,
        id: "01CANCELRETRY",
        status: "queued",
      },
    });
    vi.mocked(evalApi.getRun).mockImplementation(async (id: string) =>
      id === "01CANCELRETRY" ? newRunDetail : cancelledDetail,
    );
    vi.mocked(evalApi.retryRun).mockResolvedValue(newRunDetail);

    renderDetail();

    const retry = await screen.findByRole("button", {
      name: "Retry eval run 01LIVE",
    });
    fireEvent.click(retry);

    await waitFor(() => expect(evalApi.retryRun).toHaveBeenCalled());
    expect(vi.mocked(evalApi.retryRun).mock.calls[0]?.[0]).toBe("01LIVE");
    await waitFor(() =>
      expect(
        vi
          .mocked(evalApi.getRun)
          .mock.calls.some(([id]) => id === "01CANCELRETRY"),
      ).toBe(true),
    );
  });

  it("Delete button calls the eval DELETE route and navigates back to /eval-runs", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );
    vi.mocked(evalApi.deleteRun).mockResolvedValue(undefined as never);

    render(
      <MemoryRouter initialEntries={["/eval-runs/01LIVE"]}>
        <QueryClientProvider
          client={
            new QueryClient({
              defaultOptions: { queries: { retry: false } },
            })
          }
        >
          <Routes>
            <Route path="/eval-runs/:runId" element={<EvalRunDetailRoute />} />
            <Route
              path="/eval-runs"
              element={<div data-testid="eval-runs-landing">runs landing</div>}
            />
          </Routes>
        </QueryClientProvider>
      </MemoryRouter>,
    );

    const del = await screen.findByRole("button", {
      name: "Delete eval run 01LIVE",
    });
    fireEvent.click(del);

    await waitFor(() => expect(evalApi.deleteRun).toHaveBeenCalled());
    expect(vi.mocked(evalApi.deleteRun).mock.calls[0]?.[0]).toBe("01LIVE");
    await screen.findByTestId("eval-runs-landing");
  });

  it("clears the trace-dock active run when the inspector unmounts", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    const { unmount } = renderDetail();

    await screen.findByRole("button", { name: /stop eval run/i });
    expect(useTraceDock.getState().byScope.eval.activeRunId).toBe("01LIVE");

    unmount();

    expect(useTraceDock.getState().byScope.eval.activeRunId).toBeNull();
  });

  it("pushes the eval-side cost into the trace dock so the capsule matches the meta strip", async () => {
    // Pricing rolled up on the eval table (`inference_cost_quote_total`)
    // but not on the linked agent-run summary (`total_cost_usd === 0`):
    // the meta strip uses `displayCost`, which prefers the eval-side
    // value. The capsule reads from the trace-dock store's
    // `costOverrideUsd`, so the eval-detail page must push the same
    // computed value or the capsule will show "—" / "$0.00" while the
    // strip shows the real cost.
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
          inference_cost_quote_total: 0.4242,
        },
      }),
    );

    renderDetail();

    // Wait for the page to settle on the completed-run surface (the
    // Rerun button is only mounted after the run summary loads).
    await screen.findByRole("button", { name: /rerun eval run 01live/i });

    await waitFor(() =>
      expect(useTraceDock.getState().byScope.eval.costOverrideUsd).toBe(0.4242),
    );

    // And the stat rail in the page header renders the same number.
    await waitFor(() =>
      expect(document.body.textContent ?? "").toMatch(/\$0\.4242/),
    );
  });

  it("renders the topbar status pill from run.status while the run is running", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    // Wait for actual content to render, not the loading skeleton.
    await screen.findByRole("button", { name: /stop eval run/i });
    // The Signal topbar carries the lifecycle status. While running it reads
    // "EVAL RUNNING" — never "COMPLETED".
    const status = screen.getByTestId("eval-topbar-status");
    expect(status.textContent).toContain("EVAL RUNNING");
    expect(status.textContent).not.toContain("COMPLETED");
  });

  it("pulses the topbar status dot while the run is running", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    await screen.findByRole("button", { name: /stop eval run/i });
    const status = screen.getByTestId("eval-topbar-status");
    // The running dot animates; terminal statuses do not.
    expect(status.querySelector(".animate-pulse")).not.toBeNull();
  });

  it("does not render a separate streaming capsule alongside the running pill", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    await screen.findByRole("button", { name: /stop eval run/i });
    // Single status indicator on the surface: the animated pill.
    // The legacy duplicate "streaming" indicator must not appear.
    expect(screen.queryByText(/^streaming$/i)).not.toBeInTheDocument();
  });

  it("strips animation off the pill once the run reaches a terminal state", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:01:00Z",
        },
      }),
    );

    renderDetail();

    await screen.findByRole("button", { name: /download eval run/i });
    // On a terminal run the topbar reads COMPLETED with a static (non-pulsing)
    // dot — no animation once the run finishes.
    const status = screen.getByTestId("eval-topbar-status");
    expect(status.textContent).toContain("EVAL COMPLETED");
    expect(status.querySelector(".animate-pulse")).toBeNull();
  });

  it("shows a Retry button on failed terminal runs", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "failed",
          completed_at: "2026-05-13T14:30:00Z",
          error: "provider 5xx",
        },
      }),
    );

    renderDetail();

    expect(
      await screen.findByRole("button", { name: "Retry eval run 01LIVE" }),
    ).toBeInTheDocument();
    // Stop button is gone on a terminal run.
    expect(
      screen.queryByRole("button", { name: "Stop eval run 01LIVE" }),
    ).not.toBeInTheDocument();
  });

  it.each(["queued", "running"] as const)(
    "hides the Retry/Rerun button on %s runs",
    async (status) => {
      vi.mocked(evalApi.getRun).mockResolvedValue(
        detail({
          summary: {
            ...detail().summary,
            status,
            completed_at: null,
          },
        }),
      );

      renderDetail();

      // Wait for some content to render (the id is enough to confirm load).
      await screen.findByText("01LIVE");
      // Neither label should appear on in-flight runs.
      expect(
        screen.queryByRole("button", { name: "Retry eval run 01LIVE" }),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Rerun eval run 01LIVE" }),
      ).not.toBeInTheDocument();
    },
  );

  // eval-rerun-from-completed (2026-05-19): completed runs now show a
  // distinct "Rerun" button (semantics: fresh trace against the same
  // agent/scenario inputs). The tooltip text disambiguates it from the
  // failure-recovery "Retry" button.
  it("renders 'Rerun' (not 'Retry') on completed runs with disambiguating tooltip", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "completed",
          completed_at: "2026-05-13T14:30:00Z",
        },
      }),
    );

    renderDetail();

    const rerun = await screen.findByRole("button", {
      name: "Rerun eval run 01LIVE",
    });
    expect(rerun).toBeInTheDocument();
    expect(rerun.getAttribute("title")).toMatch(
      /fresh trace.*same agent\/scenario/i,
    );
    // The failure-recovery label must not appear on a completed run.
    expect(
      screen.queryByRole("button", { name: "Retry eval run 01LIVE" }),
    ).not.toBeInTheDocument();
  });

  it("clicking Rerun on a completed run posts and navigates to the new run id", async () => {
    const completedDetail = detail({
      summary: {
        ...detail().summary,
        status: "completed",
        completed_at: "2026-05-13T14:30:00Z",
      },
    });
    const newRunDetail = detail({
      summary: { ...detail().summary, id: "01RERUN", status: "queued" },
    });
    vi.mocked(evalApi.getRun).mockImplementation(async (id: string) =>
      id === "01RERUN" ? newRunDetail : completedDetail,
    );
    vi.mocked(evalApi.retryRun).mockResolvedValue(newRunDetail);

    renderDetail();

    const rerun = await screen.findByRole("button", {
      name: "Rerun eval run 01LIVE",
    });
    fireEvent.click(rerun);

    await waitFor(() => expect(evalApi.retryRun).toHaveBeenCalled());
    expect(vi.mocked(evalApi.retryRun).mock.calls[0]?.[0]).toBe("01LIVE");
    await waitFor(() =>
      expect(
        vi.mocked(evalApi.getRun).mock.calls.some(([id]) => id === "01RERUN"),
      ).toBe(true),
    );
  });

  // Pin: the "Retry" label on a failed run still includes the
  // failure-recovery tooltip wording (NOT the rerun wording).
  it("renders 'Retry' label with failure-recovery tooltip on failed runs", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "failed",
          completed_at: "2026-05-13T14:30:00Z",
          error: "provider 5xx",
        },
      }),
    );

    renderDetail();

    const retry = await screen.findByRole("button", {
      name: "Retry eval run 01LIVE",
    });
    expect(retry.getAttribute("title")).not.toMatch(
      /fresh trace.*same agent\/scenario/i,
    );
  });

  it("surfaces a classified retry error inline when the mutation rejects", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "failed",
          completed_at: "2026-05-13T14:30:00Z",
          error: "provider 5xx",
        },
      }),
    );
    vi.mocked(evalApi.retryRun).mockRejectedValue(
      new Error("backend says no"),
    );

    renderDetail();

    const retry = await screen.findByRole("button", {
      name: "Retry eval run 01LIVE",
    });
    fireEvent.click(retry);

    const banner = await screen.findByTestId("eval-retry-error");
    expect(banner.textContent).toMatch(/Retry failed: backend says no/);
  });

  it("clicking Retry posts and navigates to the new run id", async () => {
    const failedDetail = detail({
      summary: {
        ...detail().summary,
        status: "failed",
        completed_at: "2026-05-13T14:30:00Z",
        error: "provider 5xx",
      },
    });
    const newRunDetail = detail({
      summary: { ...detail().summary, id: "01NEWRUN", status: "queued" },
    });
    vi.mocked(evalApi.getRun).mockImplementation(async (id: string) =>
      id === "01NEWRUN" ? newRunDetail : failedDetail,
    );
    vi.mocked(evalApi.retryRun).mockResolvedValue(newRunDetail);

    renderDetail();

    const retry = await screen.findByRole("button", {
      name: "Retry eval run 01LIVE",
    });
    fireEvent.click(retry);

    await waitFor(() => expect(evalApi.retryRun).toHaveBeenCalled());
    expect(vi.mocked(evalApi.retryRun).mock.calls[0]?.[0]).toBe("01LIVE");
    await waitFor(() =>
      expect(
        vi.mocked(evalApi.getRun).mock.calls.some(([id]) => id === "01NEWRUN"),
      ).toBe(true),
    );
  });
});

describe("EvalRunDetailRoute — Signal decisions table", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(chartApi.openRunStream).mockImplementation(
      (runId: string) => new EventSource(`/stream/${runId}`),
    );
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([]);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders absolute Total PnL ($) in the summary card from the equity curve (QA22)", async () => {
    // Alongside `Net %` the Summary card surfaces the absolute terminal-PnL in
    // account currency. Equity opens at $10,000, closes at $10,642 → +$642.00.
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        equity_curve: [
          { timestamp: "2026-05-13T14:00:00Z", equity_usd: 10000 },
          { timestamp: "2026-05-13T14:30:00Z", equity_usd: 10642 },
        ],
      }),
    );
    renderDetail();
    expect(await screen.findByText("TOTAL PNL")).toBeInTheDocument();
    expect(screen.getByText("+$642.00")).toBeInTheDocument();
    expect(screen.getByText(/Realized unavailable/i)).toBeInTheDocument();
    expect(screen.getByText(/Unrealized unavailable/i)).toBeInTheDocument();
  });

  it("splits summary PnL into realized and unrealized components", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        equity_curve: [
          { timestamp: "2026-05-13T14:00:00Z", equity_usd: 10000 },
          { timestamp: "2026-05-13T14:30:00Z", equity_usd: 10642 },
        ],
        decisions: [
          decision({ decision_index: 0, action: "long_open", pnl_realized: null }),
          decision({ decision_index: 1, action: "flat", pnl_realized: 250 }),
        ],
      }),
    );

    renderDetail();

    expect(await screen.findByText("TOTAL PNL")).toBeInTheDocument();
    expect(screen.getByText("+$642.00")).toBeInTheDocument();
    expect(screen.getByText(/Realized \+\$250\.00/i)).toBeInTheDocument();
    expect(screen.getByText(/Unrealized \+\$392\.00/i)).toBeInTheDocument();
  });

  it("surfaces buy/sell decision context with the chart before the full decisions table", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        equity_curve: [
          { timestamp: "2026-05-13T14:00:00Z", equity_usd: 10000 },
          { timestamp: "2026-05-13T14:30:00Z", equity_usd: 10642 },
        ],
        decisions: [
          decision({ decision_index: 0, action: "long_open", fill_size: 1, fill_price: 50_000 }),
          decision({
            decision_index: 1,
            action: "flat",
            fill_size: 1,
            fill_price: 51_000,
            pnl_realized: 250,
          }),
        ],
      }),
    );

    renderDetail();

    const tape = await screen.findByTestId("eval-decision-tape");
    expect(within(tape).getByText("BUY")).toBeInTheDocument();
    expect(within(tape).getByText("SELL")).toBeInTheDocument();

    const chartState = screen.getByText("No chart data.");
    const fullDecisionsHeading = screen.getByRole("heading", {
      name: "Decisions",
    });
    expect(
      tape.compareDocumentPosition(chartState) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      chartState.compareDocumentPosition(fullDecisionsHeading) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("maps engine actions to the Signal action vocabulary (BUY/SELL/SHORT/CLOSE/HOLD)", async () => {
    // Signal action vocabulary:
    //   0  long_open            → BUY
    //   1  flat (prior=long)    → SELL   (exit a long)
    //   2  short_open           → SHORT  (short entry)
    //   3  flat (prior=short)   → CLOSE  (cover the short)
    //   4  hold (reasoned)      → HOLD
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        decisions: [
          decision({ decision_index: 0, action: "long_open", fill_size: 1, fill_price: 50_000 }),
          decision({
            decision_index: 1,
            action: "flat",
            fill_size: 1,
            fill_price: 51_000,
            pnl_realized: 1000,
          }),
          decision({ decision_index: 2, action: "short_open", fill_size: 0.5, fill_price: 49_000 }),
          decision({
            decision_index: 3,
            action: "flat",
            fill_size: 0.5,
            fill_price: 48_000,
            pnl_realized: 250,
          }),
          decision({
            decision_index: 4,
            action: "hold",
            conviction: 0.3,
            justification: "stay flat, chop",
          }),
        ],
      }),
    );

    renderDetail();

    // Each action verb appears at least once across the rows + density legend.
    expect((await screen.findAllByText("BUY")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("SELL").length).toBeGreaterThan(0);
    // short_open now maps to SHORT (distinct from SELL which is long-exit)
    expect(screen.getAllByText("SHORT").length).toBeGreaterThan(0);
    expect(screen.getAllByText("CLOSE").length).toBeGreaterThan(0);
    expect(screen.getAllByText("HOLD").length).toBeGreaterThan(0);
    // COVER is gone from the redesigned table.
    expect(screen.queryByText("COVER")).not.toBeInTheDocument();
  });

  it("shows realized PnL on a close row as a signed $ amount", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        decisions: [
          decision({ decision_index: 0, action: "long_open", fill_size: 1, fill_price: 50_000 }),
          decision({
            decision_index: 1,
            action: "flat",
            fill_size: 1,
            fill_price: 51_000,
            pnl_realized: 999,
          }),
        ],
      }),
    );

    renderDetail();

    // The Signal PnL cell renders a signed currency amount, not a bare number.
    expect(await screen.findByText("+$999")).toBeInTheDocument();
  });

  it("the mutually-exclusive filter pill row narrows the table to a single action", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: { ...detail().summary, status: "completed" },
        decisions: [
          decision({ decision_index: 0, action: "long_open", justification: "entry" }),
          decision({
            decision_index: 1,
            action: "hold",
            conviction: 0.2,
            justification: "wait",
          }),
        ],
      }),
    );

    renderDetail();

    // The Buy pill carries a count badge; clicking it scopes to the one buy row
    // and hides the hold row's action pill.
    const buyPill = await screen.findByRole("button", { name: /Buy\s*1/ });
    fireEvent.click(buyPill);
    expect(buyPill).toHaveAttribute("aria-pressed", "true");
    // After scoping to Buy, the HOLD action pill is no longer in the table body.
    await waitFor(() =>
      expect(screen.queryByText("HOLD")).not.toBeInTheDocument(),
    );
    expect(screen.getAllByText("BUY").length).toBeGreaterThan(0);
  });
});
