import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { OptimizerHome } from "./OptimizerHome";
import * as apiModule from "../api";

// Mock uPlot — charts render inside OptimizerHome after P3 additions
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

// Default flywheel mock — disabled unless overridden per test
const defaultFlywheelMock = () =>
  vi.spyOn(apiModule, "useFlywheel").mockReturnValue({
    data: { enabled: false },
    isLoading: false,
    isError: false,
  } as unknown as ReturnType<typeof apiModule.useFlywheel>);

// Default stats mock — empty rows
const defaultStatsMock = () =>
  vi.spyOn(apiModule, "useOptimizerStats").mockReturnValue({
    data: [],
    isLoading: false,
    isError: false,
  } as unknown as ReturnType<typeof apiModule.useOptimizerStats>);

afterEach(() => vi.restoreAllMocks());

// Suppress EventSource errors in jsdom
beforeEach(() => {
  // @ts-expect-error jsdom EventSource stub
  global.EventSource = vi.fn().mockImplementation(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }));
  defaultFlywheelMock();
  defaultStatsMock();
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

describe("OptimizerHome — FlywheelStrip integration", () => {
  const idleStatus = {
    active_session: null,
    last_event_seq: 0,
  };

  it("renders FlywheelStrip when useFlywheel returns enabled=true", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(idleStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    vi.spyOn(apiModule, "useFlywheel").mockReturnValue({
      data: {
        enabled: true,
        cohort_count: 5,
        threshold: 20,
        compiled_pattern_count: 1,
        latest_optimization_run_id: "run_XYZ",
        last_prompt_compile: null,
      },
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useFlywheel>);

    renderWithProviders(<OptimizerHome />);

    expect(await screen.findByText(/Observations toward next prompt compile/i)).toBeInTheDocument();
  });

  it("does not render FlywheelStrip when useFlywheel returns enabled=false", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(idleStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    // useFlywheel already mocked to disabled in beforeEach

    renderWithProviders(<OptimizerHome />);

    // Give async rendering a tick
    await screen.findByText("Idle");
    expect(screen.queryByText(/Observations toward next prompt compile/i)).toBeNull();
  });
});

describe("OptimizerHome — outcome mix toggle (P3-W3)", () => {
  const idleStatus = {
    active_session: null,
    last_event_seq: 0,
  };

  function setupIdleMocks() {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(idleStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
  }

  it("renders the 'Show outcome mix' toggle button", async () => {
    setupIdleMocks();
    renderWithProviders(<OptimizerHome />);
    expect(
      await screen.findByRole("button", { name: /show outcome mix/i }),
    ).toBeInTheDocument();
  });

  it("OutcomeStackedChart is hidden by default", async () => {
    setupIdleMocks();
    renderWithProviders(<OptimizerHome />);
    await screen.findByRole("button", { name: /show outcome mix/i });
    // The stacked chart data-attribute should not be visible initially
    const chart = document.querySelector("[data-chart='outcome-stacked']");
    expect(chart).toBeNull();
  });

  it("clicking 'Show outcome mix' reveals OutcomeStackedChart", async () => {
    setupIdleMocks();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    const btn = await screen.findByRole("button", { name: /show outcome mix/i });
    await user.click(btn);
    // After click the chart container should be present
    expect(document.querySelector("[data-chart='outcome-stacked']")).toBeInTheDocument();
  });

  it("clicking toggle again hides OutcomeStackedChart", async () => {
    setupIdleMocks();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    const btn = await screen.findByRole("button", { name: /show outcome mix/i });
    await user.click(btn);
    expect(document.querySelector("[data-chart='outcome-stacked']")).toBeInTheDocument();
    await user.click(btn);
    expect(document.querySelector("[data-chart='outcome-stacked']")).toBeNull();
  });
});
