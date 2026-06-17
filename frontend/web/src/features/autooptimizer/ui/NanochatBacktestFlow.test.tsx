import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { createElement, type ReactNode } from "react";
import { NanochatBacktestFlow } from "./NanochatBacktestFlow";
import type { RunDetail } from "@/api/types.gen";

afterEach(() => cleanup());

const APPROVE_CHECKPOINT = vi.fn().mockResolvedValue({ model_id: "mod-1", live_approved: true });

// The real API function is `startRun` (not `startEvalRun` as the plan draft named it).
// We mock it at the module level so the component can be swapped to a spy.
vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>("@/api/eval");
  return {
    ...actual,
    startRun: vi.fn().mockResolvedValue({ summary: { id: "eval-run-1" } } as RunDetail),
  };
});

vi.mock("@/api/nanochat", async () => {
  const actual = await vi.importActual<typeof import("@/api/nanochat")>("@/api/nanochat");
  return {
    ...actual,
    useApproveCheckpoint: vi.fn(() => ({
      mutateAsync: APPROVE_CHECKPOINT,
      isPending: false,
    })),
  };
});

function makeWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) =>
    createElement(
      QueryClientProvider,
      { client: qc },
      createElement(MemoryRouter, null, children),
    );
}

const DEFAULT_PROPS = {
  strategyId: "strat-1",
  checkpointModelId: "mod-1",
  onApproved: vi.fn(),
};

describe("NanochatBacktestFlow", () => {
  it("renders a 'Run backtest' button (operator-triggered, not auto-fired)", () => {
    render(<NanochatBacktestFlow {...DEFAULT_PROPS} />, { wrapper: makeWrapper() });
    expect(screen.getByRole("button", { name: /run backtest/i })).toBeInTheDocument();
  });

  it("does NOT auto-fire backtest on mount", async () => {
    const evalApi = await import("@/api/eval");
    render(<NanochatBacktestFlow {...DEFAULT_PROPS} />, { wrapper: makeWrapper() });
    // Wait a tick to confirm no auto-fire
    await new Promise((r) => setTimeout(r, 50));
    expect(vi.mocked(evalApi.startRun)).not.toHaveBeenCalled();
  });

  it("clicking Run backtest launches two eval runs (with/without checkpoint slot)", async () => {
    const evalApi = await import("@/api/eval");
    vi.mocked(evalApi.startRun).mockResolvedValue({ summary: { id: "run-x" } } as RunDetail);

    render(<NanochatBacktestFlow {...DEFAULT_PROPS} />, { wrapper: makeWrapper() });
    fireEvent.click(screen.getByRole("button", { name: /run backtest/i }));

    await waitFor(() => {
      expect(vi.mocked(evalApi.startRun)).toHaveBeenCalledTimes(2);
    });
    // First call includes the checkpoint; second does not
    const [call1, call2] = vi.mocked(evalApi.startRun).mock.calls;
    expect(JSON.stringify(call1[0])).toContain("mod-1"); // with checkpoint
    expect(JSON.stringify(call2[0])).not.toContain("mod-1"); // without checkpoint (baseline)
  });

  it("operator confirm flow calls approve then invokes onApproved", async () => {
    const evalApi = await import("@/api/eval");
    vi.mocked(evalApi.startRun).mockResolvedValue({ summary: { id: "run-y" } } as RunDetail);
    const onApproved = vi.fn();

    render(
      <NanochatBacktestFlow {...DEFAULT_PROPS} onApproved={onApproved} />,
      { wrapper: makeWrapper() },
    );

    fireEvent.click(screen.getByRole("button", { name: /run backtest/i }));

    // Wait for the runs to start and confirm button to appear
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /confirm.*approve/i })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /confirm.*approve/i }));

    await waitFor(() => {
      expect(APPROVE_CHECKPOINT).toHaveBeenCalledWith("mod-1");
      expect(onApproved).toHaveBeenCalled();
    });
  });

  it("does not import Dialog, Sheet, or Popover", async () => {
    const src = await fetch(
      new URL("./NanochatBacktestFlow.tsx", import.meta.url).href,
    ).then((r) => r.text()).catch(() => "");
    for (const name of ["Dialog", "Sheet", "Popover"]) {
      expect(src).not.toMatch(new RegExp(`import[^;]+${name}`));
    }
  });
});
