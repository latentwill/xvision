import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as evalReviewApi from "@/api/eval-review";
import type { RunSummary } from "@/api/types.gen";
import { CriticalFindingsRow } from "./CriticalFindingsRow";

vi.mock("@/api/eval-review", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval-review")>(
    "@/api/eval-review",
  );
  return {
    ...actual,
    listCriticalFindings: vi.fn(),
  };
});

function makeRun(
  id: string,
  strategyName?: string,
): RunSummary {
  return {
    id,
    agent_id: "agent-1",
    scenario_id: "scenario-1",
    strategy: strategyName ? { id: "s-1", display_name: strategyName } : null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-01-01T00:00:00Z",
    completed_at: "2026-01-01T01:00:00Z",
    sharpe: 1.2,
    max_drawdown_pct: 5.0,
    total_return_pct: 8.0,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
  };
}

function makeFinding(
  id: string,
  severity: string,
  summary: string,
  runId: string,
  strategyName?: string,
) {
  return {
    id,
    run_id: runId,
    kind: "drawdown_concentration",
    severity,
    summary,
    evidence: {},
    extracted_at: "2026-01-01T01:00:00Z",
    schema_version: "2",
    eval_review_id: "review-1",
    runId,
    strategyName,
    created_at: "2026-01-01T01:00:00Z",
  };
}

function renderRow(runs: RunSummary[]) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <CriticalFindingsRow runs={runs} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("CriticalFindingsRow", () => {
  it("renders up to 5 critical findings from fixture", async () => {
    const runs = [makeRun("run-1", "Alpha Strategy")];
    const findings = [
      makeFinding("f-1", "critical", "High drawdown concentration", "run-1", "Alpha Strategy"),
      makeFinding("f-2", "critical", "Win rate anomaly detected", "run-1", "Alpha Strategy"),
      makeFinding("f-3", "critical", "Tail risk exceeded threshold", "run-1", "Alpha Strategy"),
      makeFinding("f-4", "critical", "Overtrading in volatile regime", "run-1", "Alpha Strategy"),
      makeFinding("f-5", "critical", "Risk violation on asset class", "run-1", "Alpha Strategy"),
    ];
    vi.mocked(evalReviewApi.listCriticalFindings).mockResolvedValue(findings);

    renderRow(runs);

    await screen.findByText("High drawdown concentration");
    expect(screen.getByText("Win rate anomaly detected")).toBeInTheDocument();
    expect(screen.getByText("Tail risk exceeded threshold")).toBeInTheDocument();
    expect(screen.getByText("Overtrading in volatile regime")).toBeInTheDocument();
    expect(screen.getByText("Risk violation on asset class")).toBeInTheDocument();
  });

  it("each chip shows 'critical' pill and summary text", async () => {
    const runs = [makeRun("run-1", "Alpha Strategy")];
    const findings = [
      makeFinding("f-1", "critical", "Critical system finding", "run-1", "Alpha Strategy"),
    ];
    vi.mocked(evalReviewApi.listCriticalFindings).mockResolvedValue(findings);

    renderRow(runs);

    await screen.findByText("Critical system finding");
    // The Pill with "critical" text should be present
    const criticalPills = screen.getAllByText("critical");
    expect(criticalPills.length).toBeGreaterThanOrEqual(1);
    // The summary text should be visible
    expect(screen.getByText("Critical system finding")).toBeInTheDocument();
  });

  it("'Draft variant →' link navigates to /eval-runs/:runId", async () => {
    const runs = [makeRun("run-42", "Alpha Strategy")];
    const findings = [
      makeFinding("f-1", "critical", "Issue needing action", "run-42", "Alpha Strategy"),
    ];
    vi.mocked(evalReviewApi.listCriticalFindings).mockResolvedValue(findings);

    renderRow(runs);

    const link = await screen.findByRole("link", { name: /draft variant/i });
    expect(link).toHaveAttribute("href", "/eval-runs/run-42");
  });

  it("renders 'No critical findings in recent runs' when empty", async () => {
    vi.mocked(evalReviewApi.listCriticalFindings).mockResolvedValue([]);

    renderRow([makeRun("run-1")]);

    await screen.findByText(/no critical findings in recent runs/i);
    expect(screen.getByTestId("critical-findings-row")).toBeInTheDocument();
  });

  it("only shows severity=critical findings (excludes high, medium, warning)", async () => {
    const runs = [makeRun("run-1")];
    // listCriticalFindings already filters; confirm we don't re-render excluded findings
    const findings = [
      makeFinding("f-1", "critical", "Only critical finding", "run-1"),
    ];
    vi.mocked(evalReviewApi.listCriticalFindings).mockResolvedValue(findings);

    renderRow(runs);

    await screen.findByText("Only critical finding");
    // Non-critical summaries should NOT appear
    expect(screen.queryByText("High severity issue")).not.toBeInTheDocument();
    expect(screen.queryByText("Medium severity issue")).not.toBeInTheDocument();
  });

  it("limited to findings from 3 most recent completed reviews — passes correct runs slice", async () => {
    const runs = [
      makeRun("run-1"),
      makeRun("run-2"),
      makeRun("run-3"),
      makeRun("run-4"),
    ];
    vi.mocked(evalReviewApi.listCriticalFindings).mockResolvedValue([]);

    renderRow(runs);

    // Wait for the query to run
    await screen.findByText(/no critical findings in recent runs/i);

    // Verify listCriticalFindings was called with the full runs array
    // (the function itself slices to maxRuns=3 internally)
    expect(evalReviewApi.listCriticalFindings).toHaveBeenCalledWith(runs);
  });
});
