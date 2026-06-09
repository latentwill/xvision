import { describe, expect, test, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { agentRunKeys } from "@/api/agent-runs";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { RunSummary } from "@/api/types.gen";
import { useTransport } from "./useTransport";

vi.mock("@/api/eval", () => ({
  pauseRun: vi.fn(),
  resumeRun: vi.fn(),
  flattenRun: vi.fn(),
  cancelRun: vi.fn(),
}));
import { pauseRun, resumeRun, flattenRun, cancelRun } from "@/api/eval";

function mkRun(over: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return {
    run_id: "run_1",
    objective: "BTC Momentum",
    strategy_id: "strat_1",
    agent_id: null,
    started_at: "2026-06-09T10:00:00Z",
    finished_at: null,
    status: "running",
    span_count: 0,
    model_call_count: 1,
    tool_call_count: 1,
    error_count: 0,
    total_cost_usd: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
    duration_ms: null,
    financial_eval_id: null,
    retention_mode: "hash_only",
    ...over,
  };
}

function mkEval(over: Partial<RunSummary> = {}): RunSummary {
  return {
    id: "run_1",
    agent_id: "strat_1",
    scenario_id: "scen_1",
    strategy: null,
    scenario: null,
    mode: "live",
    status: "running",
    started_at: "2026-06-09T10:00:00Z",
    completed_at: null,
    sharpe: null,
    max_drawdown_pct: null,
    total_return_pct: null,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
    paused: false,
    paused_at: null,
    flatten_requested: false,
    ...over,
  } as RunSummary;
}

let qc: QueryClient;
function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

function listInCache(): AgentRunSummary[] | undefined {
  return qc.getQueryData<AgentRunSummary[]>(agentRunKeys.list());
}

beforeEach(() => {
  qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  qc.setQueryData(agentRunKeys.list(), [mkRun()]);
});
afterEach(() => vi.clearAllMocks());

describe("useTransport optimistic cache", () => {
  test("pause optimistically flips paused:true, then opens the paused expander", async () => {
    let resolve!: (v: RunSummary) => void;
    vi.mocked(pauseRun).mockReturnValue(
      new Promise<RunSummary>((r) => (resolve = r)),
    );
    const { result } = renderHook(() => useTransport(false), { wrapper });

    act(() => result.current(mkRun()).onPause());
    // Optimistic flip lands on the next microtask (onMutate awaits
    // cancelQueries first) — well before the 10s poll, and before the
    // mutation promise resolves.
    await waitFor(() => expect(listInCache()?.[0]?.paused).toBe(true));

    await act(async () => {
      resolve(mkEval({ paused: true }));
    });
    await waitFor(() =>
      expect(result.current(mkRun()).pausedExpanderOpen).toBe(true),
    );
    expect(listInCache()?.[0]?.paused).toBe(true);
  });

  test("pause failure reverts the optimistic flip + surfaces inline error", async () => {
    vi.mocked(pauseRun).mockRejectedValue(new Error("pause boom"));
    const { result } = renderHook(() => useTransport(false), { wrapper });

    await act(async () => {
      result.current(mkRun()).onPause();
    });
    // Revert restores the pre-mutation snapshot (mkRun has no `paused`).
    await waitFor(() => expect(result.current(mkRun()).error).toBe("pause boom"));
    expect(listInCache()?.[0]?.paused).toBeUndefined();
    expect(result.current(mkRun()).pausedExpanderOpen).toBe(false);
  });

  test("flatten flips flatten_requested + shows pending; run stays paused", async () => {
    vi.mocked(flattenRun).mockResolvedValue(
      mkEval({ paused: true, flatten_requested: true }),
    );
    const { result } = renderHook(() => useTransport(false), { wrapper });

    await act(async () => {
      result.current(mkRun()).onFlatten();
    });
    await waitFor(() =>
      expect(result.current(mkRun()).flattenPending).toBe(true),
    );
    expect(listInCache()?.[0]?.flatten_requested).toBe(true);
    // Run is NOT stopped by a flatten.
    expect(listInCache()?.[0]?.status).toBe("running");
  });

  test("keep open dismisses the paused expander without firing flatten", async () => {
    vi.mocked(pauseRun).mockResolvedValue(mkEval({ paused: true }));
    const { result } = renderHook(() => useTransport(false), { wrapper });

    await act(async () => {
      result.current(mkRun()).onPause();
    });
    await waitFor(() =>
      expect(result.current(mkRun()).pausedExpanderOpen).toBe(true),
    );
    act(() => result.current(mkRun()).onKeepOpen());
    await waitFor(() =>
      expect(result.current(mkRun()).pausedExpanderOpen).toBe(false),
    );
    expect(flattenRun).not.toHaveBeenCalled();
  });

  test("resume flips paused:false and clears expander", async () => {
    qc.setQueryData(agentRunKeys.list(), [mkRun({ paused: true })]);
    vi.mocked(resumeRun).mockResolvedValue(mkEval({ paused: false }));
    const { result } = renderHook(() => useTransport(false), { wrapper });

    act(() => result.current(mkRun({ paused: true })).onResume());
    await waitFor(() => expect(listInCache()?.[0]?.paused).toBe(false));
    await waitFor(() => expect(resumeRun).toHaveBeenCalledWith("run_1"));
  });

  test("stop confirm cancels the run (optimistic cancelled status)", async () => {
    vi.mocked(cancelRun).mockResolvedValue(mkEval({ status: "cancelled" }));
    const { result } = renderHook(() => useTransport(false), { wrapper });

    // onStop only opens the confirm; onStopConfirm fires the mutation.
    act(() => result.current(mkRun()).onStop());
    expect(result.current(mkRun()).stopConfirmOpen).toBe(true);

    act(() => result.current(mkRun()).onStopConfirm());
    await waitFor(() => expect(listInCache()?.[0]?.status).toBe("cancelled"));
    await waitFor(() => expect(cancelRun).toHaveBeenCalledWith("run_1"));
  });

  test("walletDisabled yields no-op handlers (no mutation fires)", () => {
    const { result } = renderHook(() => useTransport(true), { wrapper });
    act(() => result.current(mkRun()).onPause());
    expect(pauseRun).not.toHaveBeenCalled();
  });
});
