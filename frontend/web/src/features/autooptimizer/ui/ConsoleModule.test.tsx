import { describe, expect, it, vi, beforeEach } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { ConsoleModule } from "./ConsoleModule";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { useCycleEvents, useCycleRuns, useCycleRun, useOptimizerStatus } from "../api";

// Mock ExpandableArtifact so board cards / feed lines don't pull the full
// experiment-detail network stack (same idiom as NarratedFeed.test.tsx).
vi.mock("./ExpandableArtifact", () => ({
  ExpandableArtifact: ({
    hash,
    summary,
    defaultOpen,
    open,
  }: {
    hash: string;
    summary: React.ReactNode;
    defaultOpen?: boolean;
    open?: boolean;
  }) => (
    <div
      data-testid={`artifact-${hash}`}
      role="button"
      aria-expanded={(open ?? defaultOpen) ? "true" : "false"}
    >
      {summary}
    </div>
  ),
}));

vi.mock("../hooks/useCycleEventStream", () => ({
  useCycleEventStream: vi.fn(),
}));

vi.mock("../api", async (importActual) => {
  const actual = await importActual<typeof import("../api")>();
  return {
    ...actual,
    useCycleEvents: vi.fn(),
    useCycleRuns: vi.fn(),
    useCycleRun: vi.fn(),
    useOptimizerStatus: vi.fn(),
  };
});

const mockStream = vi.mocked(useCycleEventStream);
const mockEvents = vi.mocked(useCycleEvents);
const mockRuns = vi.mocked(useCycleRuns);
const mockRun = vi.mocked(useCycleRun);
const mockStatus = vi.mocked(useOptimizerStatus);

// React-query result shapes (only the fields ConsoleModule reads).
const q = <T,>(data: T) =>
  ({ data, isLoading: false, isError: false, isSuccess: true }) as never;
const qError = () =>
  ({ data: undefined, isLoading: false, isError: true, isSuccess: false }) as never;

// ─── Fixtures: REAL flattened wire shapes (progress.rs) ──────────────────────

const liveEvents = [
  { _row_id: 1, type: "cycle_started", cycle_id: "c-live", ts: "2026-06-11T10:00:00Z" },
  {
    _row_id: 2,
    type: "mutation_proposed",
    cycle_id: "c-live",
    parent_hash: "ffff0000aa",
    child_hash: "abcd1234ef",
    mutator_model: "gemini-2.5-pro",
    ts: "2026-06-11T10:01:00Z",
  },
  {
    _row_id: 3,
    type: "mutation_proposed",
    cycle_id: "c-live",
    parent_hash: "ffff0000aa",
    child_hash: "beef5678cd",
    mutator_model: "gpt-5.2",
    ts: "2026-06-11T10:01:30Z",
  },
  {
    _row_id: 4,
    type: "mutation_gated",
    child_hash: "abcd1234ef",
    passed: true,
    outcome: "kept",
    delta_day: 0.21,
    ts: "2026-06-11T10:02:00Z",
  },
] as never[];

const persistedRow = (seq: number, payload: Record<string, unknown>) => ({
  seq,
  session_id: "sess-1",
  cycle_id: "cyc-1",
  kind: String(payload.type),
  payload_json: JSON.stringify(payload),
  ts: "2026-06-11T08:00:00Z",
});

const persistedEvents = [
  persistedRow(1, { type: "cycle_started", cycle_id: "cyc-1" }),
  persistedRow(2, {
    type: "mutation_proposed",
    cycle_id: "cyc-1",
    parent_hash: "ffff0000aa",
    child_hash: "abcd1234ef",
    mutator_model: "gemini-2.5-pro",
  }),
  persistedRow(3, {
    type: "mutation_gated",
    child_hash: "abcd1234ef",
    passed: true,
    outcome: "kept",
    delta_day: 0.21,
  }),
  persistedRow(4, { type: "cycle_finished", active_count: 1, rejected_count: 0 }),
];

const twoHoursAgo = new Date(Date.now() - 2 * 3600_000).toISOString();

const cycleSummary = {
  cycle_id: "cyc-1",
  node_count: 14,
  active_count: 2,
  rejected_count: 11,
  first_created_at: twoHoursAgo,
  last_created_at: twoHoursAgo,
};

const node = (bundle_hash: string, status: string) => ({
  bundle_hash,
  parent_hash: "ffff0000aa",
  status,
  cycle_id: "cyc-1",
  created_at: twoHoursAgo,
  regime_results: [],
});

const cycleDetail = {
  ...cycleSummary,
  node_count: 3,
  nodes: [node("aaaa1111bb", "active"), node("bbbb2222cc", "rejected"), node("cccc3333dd", "quarantined")],
};

function setStream(opts: Partial<ReturnType<typeof useCycleEventStream>> = {}) {
  mockStream.mockReturnValue({
    events: [],
    connected: false,
    isRunning: false,
    activeCycleId: null,
    ...opts,
  } as never);
}

beforeEach(() => {
  vi.clearAllMocks();
  setStream();
  mockRuns.mockReturnValue(q([]));
  mockEvents.mockReturnValue(q([]));
  mockRun.mockReturnValue(q(undefined));
  mockStatus.mockReturnValue(undefined);
});

describe("ConsoleModule", () => {
  it("live mode renders ribbon, board and feed from the stream", () => {
    setStream({ events: liveEvents, connected: true, isRunning: true, activeCycleId: "c-live" });
    mockRuns.mockReturnValue(q([cycleSummary]));
    mockStatus.mockReturnValue({
      active_session: {
        session_id: "sess-1",
        strategy_id: "strat-abc",
        state: "running",
        mode: "explore",
        cycles_completed: 0,
        kept_count: 0,
        suspect_count: 0,
        dropped_count: 0,
      },
      last_event_seq: 4,
    });

    renderWithProviders(<ConsoleModule />);

    const liveHeader = screen.getByText(/Live · cycle/i);
    expect(liveHeader).toHaveTextContent("Live · cycle c-live · strat-abc");
    // ribbon: one phase active (after a gated event → "gate")
    const active = document.querySelector('[aria-current="step"]');
    expect(active).not.toBeNull();
    expect(active!.textContent).toMatch(/gate/i);
    // 2 board cards (both proposed hashes)
    expect(screen.getAllByTestId("artifact-abcd1234ef").length).toBeGreaterThan(0);
    expect(screen.getAllByTestId("artifact-beef5678cd").length).toBeGreaterThan(0);
    // feed lines: narrated sentences from the same events
    expect(screen.getAllByText(/proposed/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
  });

  it("idle mode replays the last completed cycle with full identity in the header", async () => {
    setStream({ isRunning: false, connected: true });
    mockRuns.mockReturnValue(q([cycleSummary]));
    mockEvents.mockReturnValue(q(persistedEvents));
    mockRun.mockReturnValue(q(cycleDetail));

    renderWithProviders(<ConsoleModule />);

    // cycle id (8-char) + parent strategy hash (most common parent_hash, 8 chars) + relative time
    expect(await screen.findByText(/Last cycle/i)).toHaveTextContent(
      "Last cycle · cyc-1 · strategy ffff0000 · 2h ago",
    );
    // ribbon all done: no active step
    expect(document.querySelector('[aria-current="step"]')).toBeNull();
    // feed rendered from the persisted events
    expect(screen.getAllByText(/kept/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
  });

  it("replay header omits the strategy segment when nodes aren't available", () => {
    setStream({ isRunning: false, connected: true });
    mockRuns.mockReturnValue(q([cycleSummary]));
    mockEvents.mockReturnValue(q(persistedEvents));
    mockRun.mockReturnValue(q(undefined)); // detail not loaded / no nodes

    renderWithProviders(<ConsoleModule />);

    const header = screen.getByText(/Last cycle/i);
    expect(header).toHaveTextContent("Last cycle · cyc-1 · 2h ago");
    expect(header).not.toHaveTextContent(/strategy/i);
  });

  it("respects an explicit cycleId prop (replay even while a run is live)", () => {
    setStream({ events: liveEvents, connected: true, isRunning: true, activeCycleId: "c-live" });
    mockRuns.mockReturnValue(q([cycleSummary]));
    mockEvents.mockReturnValue(q(persistedEvents));

    renderWithProviders(<ConsoleModule cycleId="cyc-1" />);

    expect(screen.queryByText(/Live · cycle/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Last cycle/i)).toBeInTheDocument();
    expect(document.querySelector('[aria-current="step"]')).toBeNull();
  });

  it("never renders waiting copy", () => {
    setStream({ isRunning: false, connected: false });
    mockRuns.mockReturnValue(q([]));

    renderWithProviders(<ConsoleModule />);

    expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
  });

  it("never-ran renders the phase explainer with launch slot", () => {
    setStream({ isRunning: false, connected: false });
    mockRuns.mockReturnValue(q([]));

    renderWithProviders(<ConsoleModule launchAction={<button>Launch first cycle</button>} />);

    expect(screen.getByText(/Each cycle runs four phases/i)).toBeInTheDocument();
    expect(screen.getByText("Propose")).toBeInTheDocument();
    expect(screen.getByText("Eval")).toBeInTheDocument();
    expect(screen.getByText("Gate")).toBeInTheDocument();
    expect(screen.getByText("Keep")).toBeInTheDocument();
    expect(screen.getByText(/Experiment writers draft variations/i)).toBeInTheDocument();
    expect(screen.getByText(/backtested across regimes/i)).toBeInTheDocument();
    expect(screen.getByText(/compares each result to its parent/i)).toBeInTheDocument();
    expect(screen.getByText(/Winners join the lineage/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /launch first cycle/i })).toBeInTheDocument();
    expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
  });

  it("replay with pruned/empty events falls back to a node-derived board", () => {
    setStream({ isRunning: false, connected: true });
    mockRuns.mockReturnValue(q([cycleSummary]));
    mockEvents.mockReturnValue(q([])); // pruned / pre-persistence cycle
    mockRun.mockReturnValue(q(cycleDetail));

    renderWithProviders(<ConsoleModule defaultOpenHash="bbbb2222cc" />);

    expect(screen.getByText(/Last cycle/i)).toBeInTheDocument();
    expect(document.querySelector('[aria-current="step"]')).toBeNull();
    // 3 cards derived from nodes: kept / rejected / suspect
    expect(screen.getByTestId("artifact-aaaa1111bb")).toHaveTextContent(/kept/i);
    expect(screen.getByTestId("artifact-bbbb2222cc")).toHaveTextContent(/rejected/i);
    expect(screen.getByTestId("artifact-cccc3333dd")).toHaveTextContent(/suspect/i);
    // ?exp= deep links still work: defaultOpenHash forwarded to the board
    expect(screen.getByTestId("artifact-bbbb2222cc")).toHaveAttribute("aria-expanded", "true");
    // no apology copy — the node-derived board IS the content
    expect(
      screen.queryByText("Event log unavailable for this cycle."),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
  });

  it("replay falls back to the stream buffer when persisted events haven't landed yet", () => {
    // The cycle just finished: the SSE buffer still holds its events, but the
    // persistence worker hasn't drained the queue — the persisted fetch is empty.
    const finishedStream = [
      ...liveEvents,
      {
        _row_id: 5,
        type: "cycle_finished",
        cycle_id: "c-live",
        active_count: 1,
        rejected_count: 1,
        ts: "2026-06-11T10:03:00Z",
      },
    ] as never[];
    setStream({ events: finishedStream, connected: true, isRunning: false });
    mockRuns.mockReturnValue(q([{ ...cycleSummary, cycle_id: "c-live" }]));
    mockEvents.mockReturnValue(q([])); // worker race: nothing persisted yet
    mockRun.mockReturnValue(q(cycleDetail));

    renderWithProviders(<ConsoleModule />);

    // No "unavailable" copy — the full board + feed render from the buffer.
    expect(
      screen.queryByText("Event log unavailable for this cycle."),
    ).not.toBeInTheDocument();
    expect(screen.getAllByTestId("artifact-abcd1234ef").length).toBeGreaterThan(0);
    expect(screen.getAllByTestId("artifact-beef5678cd").length).toBeGreaterThan(0);
    expect(screen.getAllByText(/proposed/i).length).toBeGreaterThan(0);
    // Still replay mode, not live.
    expect(screen.queryByText(/Live · cycle/i)).not.toBeInTheDocument();
  });

  it("invalidates the persisted cycle-events query when cycle_finished arrives", () => {
    const invalidateSpy = vi.spyOn(QueryClient.prototype, "invalidateQueries");
    const finishedStream = [
      ...liveEvents,
      {
        _row_id: 5,
        type: "cycle_finished",
        cycle_id: "c-live",
        ts: "2026-06-11T10:03:00Z",
      },
    ] as never[];
    setStream({ events: finishedStream, connected: true, isRunning: false });
    mockRuns.mockReturnValue(q([{ ...cycleSummary, cycle_id: "c-live" }]));
    mockEvents.mockReturnValue(q([]));

    renderWithProviders(<ConsoleModule />);

    expect(invalidateSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        queryKey: ["autooptimizer", "cycle-events", "c-live"],
      }),
    );
    invalidateSpy.mockRestore();
  });

  // ─── The operator's bug: a live run the tab didn't witness from the start ──

  const inflightRows = (cycleId: string) => [
    {
      seq: 1,
      session_id: "s",
      cycle_id: cycleId,
      kind: "cycle_started",
      payload_json: JSON.stringify({ type: "cycle_started", cycle_id: cycleId }),
      ts: new Date().toISOString(),
    },
    {
      seq: 2,
      session_id: "s",
      cycle_id: cycleId,
      kind: "mutation_proposed",
      payload_json: JSON.stringify({
        type: "mutation_proposed",
        cycle_id: cycleId,
        parent_hash: "ffff0000aa",
        child_hash: "abcd1234ef",
        mutator_model: "gpt-5.2",
      }),
      ts: new Date().toISOString(),
    },
  ];

  it("goes live from server status with an EMPTY SSE buffer (CLI run, no IPC)", () => {
    // The tab joined after cycle_started, so the SSE buffer never saw it. The
    // console must still go live — driven by /status + the persisted log — and
    // must NOT show the all-green 'Cycle complete' ribbon.
    setStream({ events: [], connected: false, isRunning: false, activeCycleId: null });
    mockRuns.mockReturnValue(q([{ ...cycleSummary, cycle_id: "c-live" }]));
    mockStatus.mockReturnValue({
      active_session: {
        session_id: "sess-1",
        strategy_id: "strat-abc",
        state: "running",
        mode: "explore",
        cycles_completed: 0,
        kept_count: 0,
        suspect_count: 0,
        dropped_count: 0,
      },
      last_event_seq: 2,
      active_cycle_id: "c-live",
    });
    mockEvents.mockReturnValue(q(inflightRows("c-live")));

    renderWithProviders(<ConsoleModule />);

    expect(screen.getByText(/Live · cycle/i)).toHaveTextContent(
      "Live · cycle c-live · strat-abc",
    );
    // A real, in-progress step — not the all-done "Cycle complete" chrome.
    expect(document.querySelector('[aria-current="step"]')).not.toBeNull();
    expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("artifact-abcd1234ef").length).toBeGreaterThan(0);
  });

  it("infers a live run from an in-flight latest cycle when status AND SSE are absent", () => {
    // The exact reported case: no session row, no live SSE — only the DB event
    // log proves the CLI run is alive.
    setStream({ events: [], connected: false, isRunning: false, activeCycleId: null });
    mockRuns.mockReturnValue(q([{ ...cycleSummary, cycle_id: "c-live" }]));
    mockStatus.mockReturnValue(undefined);
    mockEvents.mockReturnValue(q(inflightRows("c-live")));

    renderWithProviders(<ConsoleModule />);

    expect(screen.getByText(/Live · cycle/i)).toHaveTextContent("Live · cycle c-live");
    expect(document.querySelector('[aria-current="step"]')).not.toBeNull();
    expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("artifact-abcd1234ef").length).toBeGreaterThan(0);
  });

  it("replay with an erroring events endpoint also falls back to nodes", () => {
    setStream({ isRunning: false, connected: true });
    mockRuns.mockReturnValue(q([cycleSummary]));
    mockEvents.mockReturnValue(qError()); // older backend without the endpoint
    mockRun.mockReturnValue(q(cycleDetail));

    renderWithProviders(<ConsoleModule />);

    expect(screen.getByTestId("artifact-aaaa1111bb")).toBeInTheDocument();
    expect(
      screen.queryByText("Event log unavailable for this cycle."),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
  });
});
