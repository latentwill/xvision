import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useLiveActivity } from "./useLiveActivity";
import { useCycleEventStream } from "./useCycleEventStream";
import { useCycleEvents, useCycleRuns, useOptimizerStatus } from "../api";

vi.mock("./useCycleEventStream", () => ({ useCycleEventStream: vi.fn() }));
vi.mock("../api", async (importActual) => {
  const actual = await importActual<typeof import("../api")>();
  return {
    ...actual,
    useCycleEvents: vi.fn(),
    useCycleRuns: vi.fn(),
    useOptimizerStatus: vi.fn(),
  };
});

const mockStream = vi.mocked(useCycleEventStream);
const mockEvents = vi.mocked(useCycleEvents);
const mockRuns = vi.mocked(useCycleRuns);
const mockStatus = vi.mocked(useOptimizerStatus);

const q = <T,>(data: T) => ({ data, isLoading: false, isError: false, isSuccess: true }) as never;

const now = () => new Date().toISOString();
const old = () => new Date(Date.now() - 60 * 60_000).toISOString(); // 1h ago

const summary = (over: Record<string, unknown> = {}) => ({
  cycle_id: "c-1",
  node_count: 3,
  active_count: 1,
  rejected_count: 2,
  first_created_at: old(),
  last_created_at: old(),
  ...over,
});

const inflight = (cycleId: string) =>
  [
    { seq: 1, session_id: "s", cycle_id: cycleId, kind: "cycle_started", payload_json: JSON.stringify({ type: "cycle_started", cycle_id: cycleId }), ts: now() },
    { seq: 2, session_id: "s", cycle_id: cycleId, kind: "mutation_proposed", payload_json: JSON.stringify({ type: "mutation_proposed", cycle_id: cycleId, child_hash: "abc" }), ts: now() },
  ] as never;

const finished = (cycleId: string) =>
  [
    { seq: 1, session_id: "s", cycle_id: cycleId, kind: "cycle_started", payload_json: JSON.stringify({ type: "cycle_started", cycle_id: cycleId }), ts: old() },
    { seq: 2, session_id: "s", cycle_id: cycleId, kind: "cycle_finished", payload_json: JSON.stringify({ type: "cycle_finished" }), ts: old() },
  ] as never;

function stream(over: Partial<ReturnType<typeof useCycleEventStream>> = {}) {
  mockStream.mockReturnValue({ events: [], connected: false, isRunning: false, activeCycleId: null, ...over } as never);
}

beforeEach(() => {
  vi.clearAllMocks();
  stream();
  mockStatus.mockReturnValue(undefined);
  mockRuns.mockReturnValue(q([]));
  mockEvents.mockReturnValue(q([]));
});

describe("useLiveActivity", () => {
  it("is idle when nothing is active", () => {
    const { result } = renderHook(() => useLiveActivity());
    expect(result.current.activity).toBe("idle");
    expect(result.current.source).toBe("none");
  });

  it("reports a controllable running session from status", () => {
    mockStatus.mockReturnValue({
      active_session: { session_id: "s", strategy_id: "x", state: "running", mode: "explore", cycles_completed: 2, kept_count: 1, suspect_count: 0, dropped_count: 0 },
      last_event_seq: 1,
      active_cycle_id: "c-live",
    });
    const { result } = renderHook(() => useLiveActivity());
    expect(result.current.activity).toBe("running");
    expect(result.current.source).toBe("status");
    expect(result.current.activeCycleId).toBe("c-live");
    expect(result.current.session?.strategy_id).toBe("x");
  });

  it("infers running from an in-flight latest cycle when status & stream are absent", () => {
    mockRuns.mockReturnValue(q([summary({ cycle_id: "c-1", last_created_at: now() })]));
    mockEvents.mockReturnValue(q(inflight("c-1")));
    const { result } = renderHook(() => useLiveActivity());
    expect(result.current.activity).toBe("running");
    expect(result.current.source).toBe("events");
    expect(result.current.activeCycleId).toBe("c-1");
    expect(result.current.session).toBeNull(); // not a controllable session
    expect(result.current.startedAtMs).not.toBeNull();
  });

  it("is idle when the latest cycle is finished", () => {
    mockRuns.mockReturnValue(q([summary({ cycle_id: "c-1", last_created_at: now() })]));
    mockEvents.mockReturnValue(q(finished("c-1")));
    const { result } = renderHook(() => useLiveActivity());
    expect(result.current.activity).toBe("idle");
  });

  it("polls the latest cycle's events only when recent / active (not for stale idle history)", () => {
    // Stale history → fetch once, no poll.
    mockRuns.mockReturnValue(q([summary({ cycle_id: "c-1", last_created_at: old() })]));
    mockEvents.mockReturnValue(q(finished("c-1")));
    renderHook(() => useLiveActivity());
    expect(mockEvents).toHaveBeenCalledWith("c-1", undefined);

    vi.clearAllMocks();
    stream();
    mockStatus.mockReturnValue(undefined);
    // Recently-touched cycle → poll.
    mockRuns.mockReturnValue(q([summary({ cycle_id: "c-1", last_created_at: now() })]));
    mockEvents.mockReturnValue(q(inflight("c-1")));
    renderHook(() => useLiveActivity());
    expect(mockEvents).toHaveBeenCalledWith("c-1", { pollMs: expect.any(Number) });
  });
});
