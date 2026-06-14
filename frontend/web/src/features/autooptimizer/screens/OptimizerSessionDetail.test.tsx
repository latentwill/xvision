import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { OptimizerSessionDetail } from "./OptimizerSessionDetail";
import * as client from "@/api/client";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const SESSION_ID = "01HXSESSION001";

const sessionDetail = {
  session_id: SESSION_ID,
  strategy_id: "strat-abc123",
  state: "finished",
  mode: "once",
  cycles_planned: 3,
  cycles_completed: 3,
  kept_count: 7,
  suspect_count: 2,
  dropped_count: 14,
  errored_count: 0,
  created_at: "2026-06-10T00:00:00Z",
  updated_at: "2026-06-10T02:00:00Z",
};

const cycleRow = (cycle_id: string, opts?: { active_count?: number; cost_usd?: number }) => ({
  cycle_id,
  node_count: 8,
  active_count: opts?.active_count ?? 3,
  suspect_count: 1,
  rejected_count: 4,
  first_created_at: "2026-06-10T00:00:00Z",
  last_created_at: "2026-06-10T00:30:00Z",
  cost_usd: opts?.cost_usd ?? 0.42,
  input_tokens: 1000,
  output_tokens: 500,
  unpriced_calls: 0,
});

const cycles = [
  cycleRow("cyc-01", { active_count: 3, cost_usd: 0.10 }),
  cycleRow("cyc-02", { active_count: 2, cost_usd: 0.18 }),
  cycleRow("cyc-03", { active_count: 2, cost_usd: 0.14 }),
];

const statsRows = [
  {
    cycle_id: "cyc-01",
    session_id: SESSION_ID,
    ts: "2026-06-10T00:05:00Z",
    kept: 3,
    suspect: 1,
    dropped: 4,
    best_delta_holdout: 0.15,
    best_edge_over_random: 0.08,
    best_parent_edge: 0.05,
    cost_usd: 0.10,
    cum_cost_usd: 0.10,
  },
  {
    cycle_id: "cyc-02",
    session_id: SESSION_ID,
    ts: "2026-06-10T01:00:00Z",
    kept: 2,
    suspect: 1,
    dropped: 5,
    best_delta_holdout: 0.11,
    best_edge_over_random: 0.06,
    best_parent_edge: 0.04,
    cost_usd: 0.18,
    cum_cost_usd: 0.28,
  },
];

function mockApi(opts?: {
  session?: Record<string, unknown>;
  cycles?: Record<string, unknown>[];
  stats?: Record<string, unknown>[];
}) {
  const session = opts?.session ?? sessionDetail;
  const cycs = opts?.cycles ?? cycles;
  const stats = opts?.stats ?? statsRows;

  return vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
    // Session detail
    if (url.includes(`/sessions/${SESSION_ID}`)) return session;
    // Cycles filtered by session_id
    if (url.includes("/cycles") && url.includes(`session_id=${SESSION_ID}`)) return cycs;
    // Stats filtered by session_id
    if (url.includes("/stats") && url.includes(`session_id=${SESSION_ID}`)) return stats;
    // Unscoped fallbacks
    if (url.includes("/sessions")) return [session];
    if (url.includes("/status")) return { active_session: null, last_event_seq: 0 };
    if (url.includes("/health")) return { status: "ok", probes: [] };
    return {};
  }) as unknown as ReturnType<typeof vi.spyOn>;
}

function renderSessionDetail(route = `/optimizer/run/${SESSION_ID}`) {
  return renderWithProviders(
    <Routes>
      <Route path="/optimizer/run/:sessionId" element={<OptimizerSessionDetail />} />
      <Route path="/optimizer" element={<div>Optimizer home</div>} />
    </Routes>,
    { route },
  );
}

afterEach(() => vi.restoreAllMocks());
beforeEach(() => vi.clearAllMocks());

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("OptimizerSessionDetail", () => {
  it("renders the Topbar with title 'Optimizer' and sub 'Session'", async () => {
    mockApi();
    renderSessionDetail();

    // Topbar renders the title as part of the accessible text; check for the
    // landmark or heading that contains the expected text.
    await waitFor(() =>
      expect(screen.getByText("Optimizer")).toBeInTheDocument(),
    );
    expect(screen.getByText("Session")).toBeInTheDocument();
  });

  it("renders a breadcrumb with a back link to /optimizer", async () => {
    mockApi();
    renderSessionDetail();

    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    const link = within(nav).getByRole("link", { name: /optimizer/i });
    expect(link).toHaveAttribute("href", "/optimizer");
  });

  it("displays session state and counts from the session detail API", async () => {
    mockApi();
    renderSessionDetail();

    // Wait for loading to complete then verify the loaded state
    const header = await screen.findByLabelText("Session header");
    await waitFor(() =>
      expect(within(header).getByText("Finished")).toBeInTheDocument(),
    );
    // Counts
    expect(within(header).getByText("7")).toBeInTheDocument(); // kept
    expect(within(header).getByText("2")).toBeInTheDocument(); // suspect
    expect(within(header).getByText("14")).toBeInTheDocument(); // dropped
    // Strategy id
    expect(within(header).getByText("strat-abc123")).toBeInTheDocument();
  });

  it("shows a loading state while session is fetching", async () => {
    // Return a promise that never resolves to keep the loading state visible
    vi.spyOn(client, "apiFetch").mockImplementation(
      () => new Promise(() => {}),
    );
    renderSessionDetail();

    const header = screen.getByLabelText("Session header");
    expect(within(header).getByText("Loading session…")).toBeInTheDocument();
  });

  it("shows an error state when session fetch fails", async () => {
    vi.spyOn(client, "apiFetch").mockRejectedValue(new Error("network error"));
    renderSessionDetail();

    const header = await screen.findByLabelText("Session header");
    await waitFor(() =>
      expect(within(header).getByText("Couldn't load session details.")).toBeInTheDocument(),
    );
  });

  it("renders the cycle list table with all session cycles", async () => {
    mockApi();
    renderSessionDetail();

    // All three cycle ids must appear as links
    for (const cid of ["cyc-01", "cyc-02", "cyc-03"]) {
      const link = await screen.findByRole("link", { name: cid });
      expect(link).toHaveAttribute("href", `/optimizer/cycle/${cid}`);
    }
  });

  it("passes session_id to the cycles endpoint", async () => {
    const spy = mockApi();
    renderSessionDetail();

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(
        expect.stringContaining(`session_id=${SESSION_ID}`),
      ),
    );
  });

  it("passes session_id to the stats endpoint for the chart", async () => {
    const spy = mockApi();
    renderSessionDetail();

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(
        expect.stringContaining(`session_id=${SESSION_ID}`),
      ),
    );
  });

  it("shows 'Cycles this session' section heading", async () => {
    mockApi();
    renderSessionDetail();

    await screen.findByLabelText("Session header");
    expect(
      screen.getByRole("heading", { name: "Cycles this session" }),
    ).toBeInTheDocument();
  });

  it("renders the 'View eval runs' cross-link to /eval-runs", async () => {
    mockApi();
    renderSessionDetail();

    await screen.findByLabelText("Session header");
    const evalLink = screen.getByRole("link", { name: /view eval runs/i });
    expect(evalLink).toHaveAttribute("href", "/eval-runs");
  });

  it("shows a 'no cycles' message when cycles list is empty", async () => {
    mockApi({ cycles: [] });
    renderSessionDetail();

    await screen.findByLabelText("Session header");
    await waitFor(() =>
      expect(
        screen.getByText("No cycles recorded for this session yet."),
      ).toBeInTheDocument(),
    );
  });

  it("renders the session id in the header", async () => {
    mockApi();
    renderSessionDetail();

    const header = await screen.findByLabelText("Session header");
    await waitFor(() =>
      expect(within(header).getByText(SESSION_ID)).toBeInTheDocument(),
    );
  });

  it("redirects to /optimizer when no sessionId param is present", () => {
    renderWithProviders(
      <Routes>
        <Route path="/optimizer/run" element={<OptimizerSessionDetail />} />
        <Route path="/optimizer" element={<div>Optimizer home</div>} />
      </Routes>,
      { route: "/optimizer/run" },
    );

    expect(screen.getByText("Optimizer home")).toBeInTheDocument();
  });
});
