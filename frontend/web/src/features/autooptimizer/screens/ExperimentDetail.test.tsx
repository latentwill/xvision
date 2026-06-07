import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { ExperimentDetail } from "./ExperimentDetail";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

const FIXTURE_NODE = {
  bundle_hash: "deadbeefcafe",
  parent_hash: "0000abcd",
  gate_verdict: "Pass",
  status: "active",
  cycle_id: "cyc-1",
  created_at: "2026-06-01T00:00:00Z",
};

const FIXTURE_DETAIL = {
  lineage_node: FIXTURE_NODE,
  rationale: "Increased temperature to explore broader hypothesis space",
  gate_record: {
    bundle_hash: "deadbeefcafe",
    parent_day_score: 1.2,
    child_day_score: 1.45,
    parent_holdout_score: 1.1,
    child_holdout_score: 1.25,
    gate_epsilon: 0.05,
    delta_day: 0.25,
    delta_holdout: 0.15,
    drawdown_ratio: 0.88,
    verdict: "passed",
    reason: null,
  },
  findings: [
    {
      id: 1,
      bundle_hash: "deadbeefcafe",
      severity: "info",
      code: "INFO_001",
      summary: "Diversity score is normal",
      detail: null,
      model: "gpt-4o",
    },
  ],
  regime_results: [
    {
      regime_label: "bull",
      side: "bull",
      delta_sharpe: 0.22,
      verdict: "passed",
      metrics_day: { total_return_pct: 10, sharpe: 1.3, max_drawdown_pct: 3, win_rate: 0.6, n_trades: 12 },
      metrics_untouched: { total_return_pct: 9, sharpe: 1.2, max_drawdown_pct: 3.5, win_rate: 0.58, n_trades: 11 },
    },
  ],
};

const FIXTURE_CYCLE = {
  cycle_id: "cyc-1",
  node_count: 1,
  active_count: 1,
  suspect_count: 0,
  rejected_count: 0,
  first_created_at: "2026-06-01T00:00:00Z",
  last_created_at: "2026-06-01T01:00:00Z",
  cost_usd: 1,
  input_tokens: 100,
  output_tokens: 50,
  unpriced_calls: 0,
  nodes: [
    {
      ...FIXTURE_NODE,
      regime_results: FIXTURE_DETAIL.regime_results,
    },
  ],
};

function mockFetch(override?: { detail?: object | null }) {
  vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
    if (url.includes("/experiments/") && url.includes("/detail")) {
      if (override?.detail === null) throw new Error("404");
      return override?.detail ?? FIXTURE_DETAIL;
    }
    if (url.includes("/lineage/")) return FIXTURE_NODE;
    if (url.includes("/cycles/")) return FIXTURE_CYCLE;
    if (url.includes("/health")) return { status: "ok", probes: [] };
    return {}; // blobs
  });
}

function setup() {
  return renderWithProviders(
    <Routes>
      <Route path="/optimizer/experiment/:hash" element={<ExperimentDetail />} />
    </Routes>,
    { route: "/optimizer/experiment/deadbeefcafe" },
  );
}

describe("ExperimentDetail", () => {
  it("renders all 5 sections from fixture ExperimentDetailResponse", async () => {
    mockFetch();
    setup();

    await waitFor(() => {
      // Section 1: Why tested — rationale
      expect(screen.getByText("Why tested")).toBeInTheDocument();
    });

    // Section 1: rationale text
    expect(
      screen.getByText("Increased temperature to explore broader hypothesis space"),
    ).toBeInTheDocument();

    // Section 1: ParentDiffPanel still present
    expect(screen.getByText("What this experiment changed")).toBeInTheDocument();

    // Section 2: What happened
    expect(screen.getByText("What happened")).toBeInTheDocument();

    // Section 3: The numbers — GateScorecard
    expect(screen.getByText("The numbers")).toBeInTheDocument();
    // GateScorecard renders bars with window labels
    await waitFor(() => {
      expect(screen.getByText(/Today's window/i)).toBeInTheDocument();
      expect(screen.getByText(/Untouched period/i)).toBeInTheDocument();
    });

    // Section 4: Decision — verdict badge
    expect(screen.getByText("Decision")).toBeInTheDocument();

    // Section 5: Reviewer notes — FindingsList
    expect(screen.getByText("Reviewer notes")).toBeInTheDocument();
    expect(screen.getByText("INFO_001")).toBeInTheDocument();
    expect(screen.getByText("Diversity score is normal")).toBeInTheDocument();
  });

  it("shows 'No rationale recorded' when rationale is null", async () => {
    mockFetch({ detail: { ...FIXTURE_DETAIL, rationale: null } });
    setup();

    await waitFor(() => {
      expect(screen.getByText("Why tested")).toBeInTheDocument();
    });
    expect(screen.getByText("No rationale recorded")).toBeInTheDocument();
  });

  it("still renders the experiment hero and regime cards", async () => {
    mockFetch();
    setup();

    await waitFor(() => {
      const headings = screen.getAllByRole("heading", { level: 1 });
      expect(headings.some((h) => /deadbeef/.test(h.textContent ?? ""))).toBe(true);
    });

    // RegimeCards still rendered
    expect(screen.getByText("Per-regime evaluation")).toBeInTheDocument();
    expect(await screen.findByText("bull")).toBeInTheDocument();
    expect(await screen.findByText("+0.22")).toBeInTheDocument();
  });

  it("renders verdict badge in decision section", async () => {
    mockFetch();
    setup();

    await waitFor(() => {
      expect(screen.getByText("Decision")).toBeInTheDocument();
    });

    // GateBadge shows "Kept" for active/Pass verdict — may appear in both hero and Decision section
    expect(screen.getAllByText("Kept").length).toBeGreaterThanOrEqual(1);
  });

  it("shows FindingsList empty state when no findings", async () => {
    mockFetch({ detail: { ...FIXTURE_DETAIL, findings: [] } });
    setup();

    await waitFor(() => {
      expect(screen.getByText("Reviewer notes")).toBeInTheDocument();
    });
    expect(screen.getByText(/No reviewer notes for this experiment/i)).toBeInTheDocument();
  });

  it("falls back gracefully when detail endpoint not yet available", async () => {
    // When detail fetch fails, page still renders from lineage node
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/experiments/") && url.includes("/detail")) {
        throw new Error("404");
      }
      if (url.includes("/lineage/")) return FIXTURE_NODE;
      if (url.includes("/cycles/")) return FIXTURE_CYCLE;
      if (url.includes("/health")) return { status: "ok", probes: [] };
      return {};
    });
    setup();

    await waitFor(() => {
      const headings = screen.getAllByRole("heading", { level: 1 });
      expect(headings.some((h) => /deadbeef/.test(h.textContent ?? ""))).toBe(true);
    });

    // Should still show the 5 section headers
    expect(screen.getByText("Why tested")).toBeInTheDocument();
  });
});
