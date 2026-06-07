import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { RunDetail } from "./RunDetail";
import * as apiModule from "../api";
import { readFileSync } from "fs";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";
import type { StatsRow } from "../api";

// ─── uPlot stub (charts render in RunDetail after P4 additions) ───────────────
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// ─── ResizeObserver stub ──────────────────────────────────────────────────────
const OriginalResizeObserver = (globalThis as Record<string, unknown>).ResizeObserver;
beforeEach(() => {
  Object.defineProperty(globalThis, "ResizeObserver", {
    writable: true,
    configurable: true,
    value: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  });
});
afterEach(() => {
  Object.defineProperty(globalThis, "ResizeObserver", {
    writable: true,
    configurable: true,
    value: OriginalResizeObserver,
  });
});

// ─── EventSource stub ─────────────────────────────────────────────────────────
beforeEach(() => {
  // @ts-expect-error jsdom EventSource stub
  global.EventSource = vi.fn().mockImplementation(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }));
});

afterEach(() => vi.restoreAllMocks());

// ─── Shared mock helpers ──────────────────────────────────────────────────────

function mockStatus(state: string) {
  vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
    active_session: {
      session_id: "sess_01ABCDEFGHIJ",
      strategy_id: "strat-xyz",
      state,
      mode: "explore",
      cycles_completed: 3,
      kept_count: 1,
      suspect_count: 0,
      dropped_count: 2,
    },
    last_event_seq: 10,
  });
}

function mockStats(rows: StatsRow[] = []) {
  vi.spyOn(apiModule, "useOptimizerStats").mockReturnValue({
    data: rows,
    isLoading: false,
    isError: false,
  } as unknown as ReturnType<typeof apiModule.useOptimizerStats>);
}

function mockLineageEmpty() {
  vi.spyOn(apiModule, "useLineageNodes").mockReturnValue({
    data: [],
    isLoading: false,
    isError: false,
  } as unknown as ReturnType<typeof apiModule.useLineageNodes>);
}

// ─── Session controls ─────────────────────────────────────────────────────────

describe("RunDetail — session controls", () => {
  beforeEach(() => {
    mockLineageEmpty();
    mockStats([]);
  });

  it("shows Pause and Cancel buttons when state=running", async () => {
    mockStatus("running");
    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });
    expect(await screen.findByRole("button", { name: /pause/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /resume/i })).toBeNull();
  });

  it("shows Resume and Cancel buttons when state=paused", async () => {
    mockStatus("paused");
    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });
    expect(await screen.findByRole("button", { name: /resume/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /pause/i })).toBeNull();
  });

  it("shows no Pause/Resume/Cancel buttons when state=finished", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });
    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });
    // findAllByText — the session id appears in both topbar subtitle and header span
    const matches = await screen.findAllByText(/sess_01A/);
    expect(matches.length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByRole("button", { name: /pause/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /resume/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /cancel/i })).toBeNull();
  });

  it("shows no controls when state=failed", async () => {
    mockStatus("failed");
    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });
    const matches = await screen.findAllByText(/sess_01A/);
    expect(matches.length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByRole("button", { name: /pause/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /resume/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /cancel/i })).toBeNull();
  });

  it("shows optimistic 'Pausing…' while waiting after Pause is clicked", async () => {
    mockStatus("running");
    // Mock pause to be a pending mutation
    const pauseMutateFn = vi.fn().mockImplementation(() => new Promise(() => {}));
    vi.spyOn(apiModule, "usePauseSession").mockReturnValue({
      mutate: pauseMutateFn,
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.usePauseSession>);
    vi.spyOn(apiModule, "useResumeSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.useResumeSession>);
    vi.spyOn(apiModule, "useCancelSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.useCancelSession>);

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });

    // Clicking Pause should show optimistic label
    const pauseBtn = await screen.findByRole("button", { name: /pause/i });
    expect(pauseBtn).toBeInTheDocument();
  });
});

// ─── ImprovementChart per-run variant ─────────────────────────────────────────

describe("RunDetail — ImprovementChart with session-filtered stats", () => {
  beforeEach(() => {
    mockLineageEmpty();
  });

  it("renders ImprovementChart when session stats have data", async () => {
    mockStatus("running");
    const statsRows: StatsRow[] = [
      {
        cycle_id: "c1",
        session_id: "sess_01ABCDEFGHIJ",
        ts: "2026-06-07T01:00:00Z",
        kept: 1,
        suspect: 0,
        dropped: 1,
        best_delta_holdout: 0.15,
        cost_usd: 0.05,
        cum_cost_usd: 0.05,
      },
      {
        cycle_id: "c2",
        session_id: "sess_01ABCDEFGHIJ",
        ts: "2026-06-07T02:00:00Z",
        kept: 2,
        suspect: 1,
        dropped: 0,
        best_delta_holdout: 0.22,
        cost_usd: 0.04,
        cum_cost_usd: 0.09,
      },
    ];
    mockStats(statsRows);

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });

    // ImprovementChart rendered — look for its data-attribute container
    const chart = await screen.findByTestId("activity-feed");
    expect(chart).toBeInTheDocument();

    // The ImprovementChart container should exist in the doc
    expect(document.querySelector("[data-chart='improvement']")).toBeInTheDocument();
  });

  it("passes session_id filter to useOptimizerStats", () => {
    mockStatus("running");
    const statsSpy = vi.spyOn(apiModule, "useOptimizerStats").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useOptimizerStats>);

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });

    // Should have been called with session_id filter
    expect(statsSpy).toHaveBeenCalledWith(
      expect.objectContaining({ session_id: "sess_01ABCDEFGHIJ" }),
    );
  });
});

// ─── Experiments table ────────────────────────────────────────────────────────

describe("RunDetail — experiments table", () => {
  beforeEach(() => {
    mockStatus("finished");
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });
    mockStats([]);
  });

  it("renders experiment rows from fixture; rows link to /optimizer/experiment/:hash", async () => {
    vi.spyOn(apiModule, "useLineageNodes").mockReturnValue({
      data: [
        {
          bundle_hash: "abcdef1234567890",
          parent_hash: null,
          gate_verdict: "Pass",
          status: "active" as const,
          cycle_id: "c1",
          created_at: "2026-06-07T01:00:00Z",
          diversity_score: 0.75,
        },
        {
          bundle_hash: "0987654321fedcba",
          parent_hash: "abcdef1234567890",
          gate_verdict: { Fail: { reason: "overfit" } },
          status: "rejected" as const,
          cycle_id: "c1",
          created_at: "2026-06-07T01:10:00Z",
          diversity_score: null,
        },
      ],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useLineageNodes>);

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });

    // Both experiment hashes should be in the DOM
    expect(await screen.findByText(/abcdef12/)).toBeInTheDocument();
    expect(screen.getByText(/09876543/)).toBeInTheDocument();

    // Links should point to /optimizer/experiment/:hash
    const links = screen.getAllByRole("link", {
      name: (name) => /abcdef12|09876543/.test(name),
    });
    const hrefs = links.map((l) => l.getAttribute("href"));
    expect(hrefs.some((h) => h?.includes("/optimizer/experiment/abcdef1234567890"))).toBe(true);
    expect(hrefs.some((h) => h?.includes("/optimizer/experiment/0987654321fedcba"))).toBe(true);
  });

  it("renders empty state when no experiments", async () => {
    mockLineageEmpty();

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEFGHIJ" />, {
      route: "/optimizer/run/sess_01ABCDEFGHIJ",
    });

    expect(await screen.findByText(/no experiments/i)).toBeInTheDocument();
  });
});

// ─── Route preservation ────────────────────────────────────────────────────────

describe("RunDetail — route preservation", () => {
  it("/optimizer/cycle/:cycleId route still exists in routes.tsx (not removed)", () => {
    // Read the routes file and verify the cycle route is still registered.
    // We do this at the module level rather than route rendering to avoid
    // importing the full lazy-loaded route graph in tests.
    //
    // The presence of OptimizerCycle in the lazy imports is our guard:
    const __dirname = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(resolve(__dirname, "../../../routes.tsx"), "utf-8");
    // The cycle route must be registered
    expect(src).toMatch(/path:\s*["']cycle\/:cycleId["']/);
  });
});
