import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { OptimizerHome } from "./OptimizerHome";
import * as apiModule from "../api";
import * as cycleEventStreamModule from "../hooks/useCycleEventStream";
import type { CycleRunSummary, StatsRow } from "../api";

// The launch panel has its own tests. Stub it so opening the launcher
// doesn't pull the strategies network stack into this suite.
vi.mock("../ui/LaunchPanel", () => ({
  LaunchPanel: () => <div data-testid="launch-panel">launch-panel</div>,
}));

// Mock uPlot — EdgeVsRandomChart renders inside the charts row.
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// Stub ResizeObserver for uPlot charts
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

afterEach(() => vi.restoreAllMocks());

// ─── React-query result helper ────────────────────────────────────────────────

const q = <T,>(data: T) =>
  ({ data, isLoading: false, isError: false, isSuccess: true }) as never;

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const ACTIVE_CYCLE_ID = "cycle_ABCDEFGH";

const HOURS_3_AGO = new Date(Date.now() - 3 * 3600_000).toISOString();
const HOURS_2_AGO = new Date(Date.now() - 2 * 3600_000).toISOString();

const lastCycle: CycleRunSummary = {
  cycle_id: "c-77",
  node_count: 14,
  active_count: 2,
  rejected_count: 12,
  first_created_at: HOURS_3_AGO,
  last_created_at: HOURS_3_AGO,
  cost_usd: 4.5,
  input_tokens: 30_000_000,
  output_tokens: 1_800_000,
};

const statsRows: StatsRow[] = [
  {
    cycle_id: "c-77",
    session_id: "s-1",
    ts: HOURS_2_AGO,
    kept: 2,
    suspect: 0,
    dropped: 12,
    best_delta_holdout: 0.21,
    cost_usd: 4.5,
    cum_cost_usd: 4.5,
  },
];

const idleStatus = { active_session: null, last_event_seq: 0 };

const runningSessionStatus = {
  active_session: {
    session_id: "sess_01ABCDEFGHIJ",
    strategy_id: "strat-xyz",
    state: "running",
    mode: "explore",
    cycles_completed: 3,
    kept_count: 1,
    suspect_count: 0,
    dropped_count: 2,
  },
  last_event_seq: 10,
};

const pausedSessionStatus = {
  active_session: { ...runningSessionStatus.active_session, state: "paused" },
  last_event_seq: 10,
};

// ─── Default mocks ────────────────────────────────────────────────────────────

function mockMutations() {
  const pauseMutateMock = vi.fn();
  const resumeMutateMock = vi.fn();
  const cancelMutateMock = vi.fn();
  vi.spyOn(apiModule, "usePauseCycle").mockReturnValue({
    mutate: pauseMutateMock,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.usePauseCycle>);
  vi.spyOn(apiModule, "useResumeCycle").mockReturnValue({
    mutate: resumeMutateMock,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.useResumeCycle>);
  vi.spyOn(apiModule, "useCancelCycle").mockReturnValue({
    mutate: cancelMutateMock,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.useCancelCycle>);
  return { pauseMutateMock, resumeMutateMock, cancelMutateMock };
}

beforeEach(() => {
  // @ts-expect-error jsdom EventSource stub
  global.EventSource = vi.fn().mockImplementation(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }));
  vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(idleStatus);
  vi.spyOn(apiModule, "useOptimizerStats").mockReturnValue(q([]));
  vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([]));
  vi.spyOn(apiModule, "useCycleRun").mockReturnValue(q(undefined));
  vi.spyOn(apiModule, "useCycleEvents").mockReturnValue(q([]));
  vi.spyOn(apiModule, "useLineageNodes").mockReturnValue(q([]));
  vi.spyOn(apiModule, "useSchedule").mockReturnValue(q(null));
  vi.spyOn(apiModule, "useRiver").mockReturnValue(q([]));
  vi.spyOn(apiModule, "useLadder").mockReturnValue(q([]));
  vi.spyOn(cycleEventStreamModule, "useCycleEventStream").mockReturnValue({
    events: [],
    connected: false,
    isRunning: false,
    activeCycleId: null,
  });
});

function setupIdleWithHistory() {
  vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([lastCycle]));
  vi.spyOn(apiModule, "useOptimizerStats").mockReturnValue(q(statsRows));
  return mockMutations();
}

function setupRunning() {
  vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(runningSessionStatus);
  vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([lastCycle]));
  vi.spyOn(cycleEventStreamModule, "useCycleEventStream").mockReturnValue({
    events: [],
    connected: true,
    isRunning: true,
    activeCycleId: ACTIVE_CYCLE_ID,
  });
  return mockMutations();
}

function setupPaused() {
  vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(pausedSessionStatus);
  vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([lastCycle]));
  vi.spyOn(cycleEventStreamModule, "useCycleEventStream").mockReturnValue({
    events: [],
    connected: true,
    isRunning: false,
    activeCycleId: ACTIVE_CYCLE_ID,
  });
  return mockMutations();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("OptimizerHome — idle with history", () => {
  it("renders the editorial headline, Launch action, console replay, charts, writers and history", async () => {
    setupIdleWithHistory();
    renderWithProviders(<OptimizerHome />);

    // Editorial headline
    expect(
      await screen.findByRole("heading", { level: 1, name: /Last ran .* — kept 2 of 14 experiments/ }),
    ).toBeInTheDocument();
    // Contextual action
    expect(screen.getByRole("button", { name: /launch run/i })).toBeInTheDocument();
    // Console module replay label
    expect(screen.getByText(/Last cycle/)).toBeInTheDocument();
    // Charts row: river section label (LineageRiver renders its own)
    expect(screen.getByText(/Lineage · Sharpe over generations/)).toBeInTheDocument();
    // Writers panel
    expect(screen.getByText("Experiment writers")).toBeInTheDocument();
    // Cycle history table
    expect(screen.getByRole("link", { name: "c-77" })).toBeInTheDocument();
    // No "waiting for…" state, ever
    expect(screen.queryByText(/waiting for/i)).toBeNull();
  });

  it("clicking Launch run toggles the inline launch panel", async () => {
    setupIdleWithHistory();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    expect(screen.queryByTestId("launch-panel")).toBeNull();
    await user.click(await screen.findByRole("button", { name: /launch run/i }));
    expect(screen.getByTestId("launch-panel")).toBeInTheDocument();
  });

  it("renders the weekly digest with tokens and the freshness stamp", async () => {
    setupIdleWithHistory();
    renderWithProviders(<OptimizerHome />);
    expect(await screen.findByText("14 experiments")).toBeInTheDocument();
    expect(screen.getByText(/2 kept/)).toBeInTheDocument();
    expect(screen.getByText(/31\.8M tokens/)).toBeInTheDocument();
    expect(screen.getByText(/\$4\.50 spend/)).toBeInTheDocument();
    // Honesty chip: freshness stamp from the newest StatsRow ts
    expect(screen.getByText(/as of 2h ago/)).toBeInTheDocument();
  });

  it("charts-row section header carries sample sizes (attempts + cycles)", async () => {
    setupIdleWithHistory();
    renderWithProviders(<OptimizerHome />);
    expect(await screen.findByText(/14 attempts/)).toBeInTheDocument();
    expect(screen.getByText(/1 cycle\b/)).toBeInTheDocument();
  });
});

describe("OptimizerHome — running", () => {
  it("shows the running headline with Pause + Cancel and no Launch", async () => {
    const { pauseMutateMock, cancelMutateMock } = setupRunning();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);

    expect(
      await screen.findByRole("heading", { level: 1, name: /A run is in progress\./ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /launch run/i })).toBeNull();

    await user.click(screen.getByRole("button", { name: /pause/i }));
    expect(pauseMutateMock).toHaveBeenCalledWith(ACTIVE_CYCLE_ID);
    // Cancel targets the MOUNTED cycle-level route (POST /cycles/:id/cancel),
    // not the unmounted /sessions/:id/cancel surface.
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(cancelMutateMock).toHaveBeenCalledWith(ACTIVE_CYCLE_ID);

    expect(screen.queryByText(/waiting for/i)).toBeNull();
  });

  it("Pause/Cancel still work after a page reload mid-run (empty stream buffer, cycle id from status)", async () => {
    // After a reload the SSE buffer is empty — activeCycleId from the stream
    // is null. StatusResponse.active_cycle_id is the fallback target.
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      ...runningSessionStatus,
      active_cycle_id: "cycle_FROMSTATUS",
    });
    vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([lastCycle]));
    vi.spyOn(cycleEventStreamModule, "useCycleEventStream").mockReturnValue({
      events: [],
      connected: true,
      isRunning: false,
      activeCycleId: null,
    });
    const { pauseMutateMock, cancelMutateMock } = mockMutations();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);

    await screen.findByRole("heading", { level: 1, name: /A run is in progress\./ });
    await user.click(screen.getByRole("button", { name: /pause/i }));
    expect(pauseMutateMock).toHaveBeenCalledWith("cycle_FROMSTATUS");
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(cancelMutateMock).toHaveBeenCalledWith("cycle_FROMSTATUS");
  });
});

describe("OptimizerHome — live indication", () => {
  it("shows a prominent RUNNING status bar (with a live pulse) while a session runs", async () => {
    setupRunning();
    renderWithProviders(<OptimizerHome />);

    const bar = await screen.findByRole("status", { name: /optimizer running/i });
    expect(bar).toHaveTextContent(/running/i);
    // Reuses the shared in-flight Pill animation.
    expect(bar.querySelector(".xvn-pill-animated")).not.toBeNull();
  });

  it("infers a running run from the latest cycle's event log even without a session row, hiding Launch", async () => {
    // The operator's exact case: /status has no active_session and the SSE
    // buffer is empty, but the latest cycle is in-flight in the DB.
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(idleStatus);
    vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([lastCycle]));
    vi.spyOn(apiModule, "useCycleEvents").mockReturnValue(
      q([
        {
          seq: 1,
          session_id: "s",
          cycle_id: lastCycle.cycle_id,
          kind: "cycle_started",
          payload_json: JSON.stringify({ type: "cycle_started", cycle_id: lastCycle.cycle_id }),
          ts: new Date().toISOString(),
        },
        {
          seq: 2,
          session_id: "s",
          cycle_id: lastCycle.cycle_id,
          kind: "mutation_proposed",
          payload_json: JSON.stringify({
            type: "mutation_proposed",
            cycle_id: lastCycle.cycle_id,
            child_hash: "abcd1234ef",
            mutator_model: "gpt-5.2",
          }),
          ts: new Date().toISOString(),
        },
      ]),
    );
    mockMutations();

    renderWithProviders(<OptimizerHome />);

    // Running is announced…
    expect(await screen.findByRole("status", { name: /optimizer running/i })).toBeInTheDocument();
    // …Launch is hidden (no implying the optimizer is idle)…
    expect(screen.queryByRole("button", { name: /launch run/i })).toBeNull();
    // …and no controls, since an inferred run isn't a controllable session.
    expect(screen.queryByRole("button", { name: /^pause$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^cancel$/i })).toBeNull();
  });
});

describe("OptimizerHome — paused", () => {
  it("shows the paused headline with Resume + Cancel and no Launch", async () => {
    const { resumeMutateMock, cancelMutateMock } = setupPaused();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);

    expect(
      await screen.findByRole("heading", { level: 1, name: /A run is paused\./ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /launch run/i })).toBeNull();

    await user.click(screen.getByRole("button", { name: /resume/i }));
    expect(resumeMutateMock).toHaveBeenCalledWith(ACTIVE_CYCLE_ID);
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(cancelMutateMock).toHaveBeenCalledWith(ACTIVE_CYCLE_ID);

    expect(screen.queryByText(/waiting for/i)).toBeNull();
  });
});

describe("OptimizerHome — ?session= scoping", () => {
  it("passes session_id to useOptimizerStats and shows a clearable session-scope banner", async () => {
    const statsSpy = vi
      .spyOn(apiModule, "useOptimizerStats")
      .mockReturnValue(q(statsRows));
    vi.spyOn(apiModule, "useCycleRuns").mockReturnValue(q([lastCycle]));
    mockMutations();

    renderWithProviders(<OptimizerHome />, { route: "/optimizer?session=sess-1" });

    // W19: session scope is now a prominent inline banner (not a cramped chip).
    expect(await screen.findByText("Session view")).toBeInTheDocument();
    expect(screen.getByText("sess-1")).toBeInTheDocument();
    expect(statsSpy).toHaveBeenCalledWith({ session_id: "sess-1" });
    const clear = screen.getByRole("link", { name: /exit session view/i });
    expect(clear).toHaveAttribute("href", "/optimizer");
    expect(screen.queryByText(/waiting for/i)).toBeNull();
  });
});

describe("OptimizerHome — never ran", () => {
  it("shows the never-ran headline and the four-phase explainer", async () => {
    mockMutations();
    renderWithProviders(<OptimizerHome />);

    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: /The optimizer hasn't run yet\./,
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Each cycle runs four phases/)).toBeInTheDocument();
    expect(screen.getByText("Propose")).toBeInTheDocument();
    expect(screen.getByText("Keep")).toBeInTheDocument();
    expect(screen.queryByText(/waiting for/i)).toBeNull();
  });

  it("(M-5) renders exactly one Launch run button — ConsoleModule is the sole owner in never-ran state", async () => {
    mockMutations();
    renderWithProviders(<OptimizerHome />);

    await screen.findByRole("heading", {
      level: 1,
      name: /The optimizer hasn't run yet\./,
    });
    expect(screen.getAllByRole("button", { name: /launch run/i })).toHaveLength(1);
  });
});

describe("OptimizerHome — schedule subtitle", () => {
  it("idle with an enabled schedule shows 'next run <relative time>'", async () => {
    setupIdleWithHistory();
    vi.spyOn(apiModule, "useSchedule").mockReturnValue(
      q({
        id: 1,
        enabled: true,
        time_local: "21:00",
        strategy_id: "strat-xyz",
        last_run_at: null,
        next_run_at: new Date(Date.now() + 5 * 3600_000).toISOString(),
      }),
    );
    renderWithProviders(<OptimizerHome />);
    expect(await screen.findByText(/next run in 5h/i)).toBeInTheDocument();
  });

  it("does not show 'next run' when the schedule is disabled", async () => {
    setupIdleWithHistory();
    vi.spyOn(apiModule, "useSchedule").mockReturnValue(
      q({
        id: 1,
        enabled: false,
        time_local: "21:00",
        strategy_id: "strat-xyz",
        last_run_at: null,
        next_run_at: new Date(Date.now() + 5 * 3600_000).toISOString(),
      }),
    );
    renderWithProviders(<OptimizerHome />);
    await screen.findByRole("heading", { level: 1, name: /Last ran/ });
    expect(screen.queryByText(/next run/i)).toBeNull();
  });
});

describe("OptimizerHome — dropped surfaces", () => {
  it("renders no flywheel / schedule strips or phase-stepper project labels", async () => {
    setupIdleWithHistory();
    renderWithProviders(<OptimizerHome />);
    await screen.findByRole("heading", { level: 1, name: /Last ran/ });
    expect(screen.queryByText(/Observations toward next prompt compile/i)).toBeNull();
    expect(screen.queryByText(/No scheduled run/i)).toBeNull();
    expect(screen.queryByText(/Show outcome mix/i)).toBeNull();
  });
});
