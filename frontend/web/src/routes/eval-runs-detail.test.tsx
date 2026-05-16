import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { EvalRunDetailRoute } from "./eval-runs-detail";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as evalReviewApi from "@/api/eval-review";
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
});
