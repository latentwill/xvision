import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Link, MemoryRouter, Route, Routes } from "react-router-dom";

import { EvalRunDetailRoute } from "./eval-runs-detail";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as evalReviewApi from "@/api/eval-review";
import * as scenariosApi from "@/api/scenarios";
import * as strategyApi from "@/api/strategies";
import { useTraceDock } from "@/stores/trace-dock";
import type { DecisionRowDto, RunDetail } from "@/api/types.gen";

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
    },
    decisions: [],
    equity_curve: [],
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

    await screen.findByText("no decisions");
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    FakeEventSource.instances[0].emit("decision", {
      event: "decision",
      data: decision(),
    });

    expect(await screen.findByText("long_open")).toBeInTheDocument();
    expect(screen.getByText("BTC/USD")).toBeInTheDocument();
    expect(screen.getByText("0.77")).toBeInTheDocument();
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
    // run-id chip with full id available via title attribute
    const runChip = meta.querySelector('[aria-label="Run id 01LIVE"]');
    expect(runChip).not.toBeNull();
    expect(runChip?.getAttribute("title")).toBe("01LIVE");
    // The redundant `strategy <id>` / `scenario <id>` chips are gone.
    expect(meta.textContent ?? "").not.toMatch(/strategy 01AGENT/);
    expect(meta.textContent ?? "").not.toMatch(/scenario btc-4h/);
  });

  it("renders the action-row buttons at uniform widths via per-button min-w", async () => {
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
    // Each button carries the same min-width floor so they read at
    // uniform widths regardless of label length. The previous
    // `grid grid-flow-col auto-cols-fr` shell (PR #255) did NOT
    // equalize widths in an unconstrained inline-grid (`1fr`
    // collapses to content size when there's no fixed container
    // width), so the operator still saw uneven buttons — see
    // `qa-eval-inspector-buttons-actually-uniform`.
    for (const button of Array.from(buttons)) {
      expect(button.className).toContain("min-w-[16ch]");
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
    // The route renders an "Reviews are available after the run finishes"
    // empty state until status === "completed".
    await screen.findByRole("button", { name: /stop eval run/i });
    expect(
      screen.getByText(/reviews are available after the run finishes/i),
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
    // Agent picker shows the four canonical personas.
    expect(
      screen.getByRole("button", { name: "Fast Trader" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Reasoning" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Risk" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Research" }),
    ).toBeInTheDocument();
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

    const button = await screen.findByRole("button", { name: "Reasoning" });
    fireEvent.click(button);

    await waitFor(() =>
      expect(evalReviewApi.generateReview).toHaveBeenCalledWith("01LIVE", {
        agent_profile_id: "reasoning-agent",
        force: true,
      }),
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
    expect(
      screen.getByRole("button", { name: "Reasoning" }),
    ).toBeInTheDocument();
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
    expect(useTraceDock.getState().activeRunId).toBe("01LIVE");

    unmount();

    expect(useTraceDock.getState().activeRunId).toBeNull();
  });

  it("renders status pill from run.status while the run is running", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    // Wait for actual content to render, not the loading skeleton.
    await screen.findByRole("button", { name: /stop eval run/i });
    // The pill's text content equals run.status — not the trailing
    // span's state. While running, never "completed".
    const pill = document.querySelector(".xvn-pill-animated");
    expect(pill).not.toBeNull();
    expect(pill?.textContent).toContain("running");
    expect(pill?.textContent).not.toContain("completed");
  });

  it("animates the running pill via prefers-reduced-motion-aware xvn-pill-animated class", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    await screen.findByRole("button", { name: /stop eval run/i });
    const pill = document.querySelector(".xvn-pill-animated");
    expect(pill).not.toBeNull();
    expect(pill?.getAttribute("data-running")).toBe("true");
    expect(pill?.getAttribute("aria-busy")).toBe("true");
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
    // The animated class only attaches while status === "running".
    expect(document.querySelector(".xvn-pill-animated")).toBeNull();
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

  it.each(["completed", "queued", "running", "cancelled"] as const)(
    "hides the Retry button on %s runs",
    async (status) => {
      vi.mocked(evalApi.getRun).mockResolvedValue(
        detail({
          summary: {
            ...detail().summary,
            status,
            completed_at:
              status === "running" || status === "queued"
                ? null
                : "2026-05-13T14:30:00Z",
          },
        }),
      );

      renderDetail();

      // Wait for some content to render (the id is enough to confirm load).
      await screen.findByText("01LIVE");
      expect(
        screen.queryByRole("button", { name: "Retry eval run 01LIVE" }),
      ).not.toBeInTheDocument();
    },
  );

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
