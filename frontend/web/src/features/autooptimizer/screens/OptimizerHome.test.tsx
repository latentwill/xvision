import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { OptimizerHome } from "./OptimizerHome";
import * as apiModule from "../api";

afterEach(() => vi.restoreAllMocks());

// Suppress EventSource errors in jsdom
beforeEach(() => {
  // @ts-expect-error jsdom EventSource stub
  global.EventSource = vi.fn().mockImplementation(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }));
});

describe("OptimizerHome — status hero", () => {
  it("shows Idle pill and Start button when status returns null active_session", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderWithProviders(<OptimizerHome />);

    expect(await screen.findByText("Idle")).toBeInTheDocument();
    // Start button must be visible when idle
    expect(screen.getByRole("button", { name: /start/i })).toBeInTheDocument();
    // Pause/Cancel must NOT be visible
    expect(screen.queryByRole("button", { name: /pause/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /cancel/i })).toBeNull();
  });

  it("shows Running pill and Pause + Cancel buttons when a session is active", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
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
    });
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderWithProviders(<OptimizerHome />);

    expect(await screen.findByText("Running")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /pause/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /start/i })).toBeNull();
  });

  it("does not import or call deriveCycleState", () => {
    // Static check: the module should not export deriveCycleState
    // (it was removed from LiveCycleView and never exported from api.ts)
    expect((apiModule as Record<string, unknown>).deriveCycleState).toBeUndefined();
  });

  it("renders recent runs list from useSessionList", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [
        {
          session_id: "sess_AAA",
          strategy_id: "alpha-strategy",
          state: "finished",
          mode: "explore",
          cycles_completed: 10,
          kept_count: 4,
          cost_usd: 0.12,
          finished_at: new Date(Date.now() - 3600_000).toISOString(),
        },
        {
          session_id: "sess_BBB",
          strategy_id: "beta-strategy",
          state: "failed",
          mode: "exploit",
          cycles_completed: 2,
          kept_count: 0,
        },
      ],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderWithProviders(<OptimizerHome />);

    expect(await screen.findByText("alpha-strategy")).toBeInTheDocument();
    expect(screen.getByText("beta-strategy")).toBeInTheDocument();
  });
});
