// MemorySurface.test.tsx — targeted tests for the FlywheelPanel sub-component
// inside MemorySurface.
//
// Focus: the "Stage Pattern" button precondition guard introduced in
// bead xvision-5jzr: when the flywheel status shows fewer than 2 observations
// the button must be disabled and the raw backend error string must never
// reach the user.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { FlywheelPanel } from "./MemorySurface";
import * as flywheelApi from "@/api/flywheel";
import * as memoryApi from "@/api/memory";

vi.mock("@/api/memory", async () => {
  const actual = await vi.importActual<typeof import("@/api/memory")>(
    "@/api/memory",
  );
  return {
    ...actual,
    listMemory: vi.fn(),
    createPattern: vi.fn(),
    createOperatorAttestation: vi.fn(),
    activatePattern: vi.fn(),
    demotePattern: vi.fn(),
    deleteMemoryItem: vi.fn(),
    forgetMemory: vi.fn(),
  };
});

vi.mock("@/api/flywheel", async () => {
  const actual = await vi.importActual<typeof import("@/api/flywheel")>(
    "@/api/flywheel",
  );
  return {
    ...actual,
    getFlywheelStatus: vi.fn(),
    getFlywheelVelocity: vi.fn(),
    getFlywheelLineage: vi.fn(),
    listAutoOptimizerRuns: vi.fn(),
    runAutoOptimizer: vi.fn(),
    getAutoOptimizerRun: vi.fn(),
    gateAutoOptimizerRun: vi.fn(),
    gateOptimization: vi.fn(),
    promoteAutoOptimizerRun: vi.fn(),
    demoteAutoOptimizerRun: vi.fn(),
    optimizeMemoryDemos: vi.fn(),
  };
});

function makeFlywheelStatusWith(observations: number): flywheelApi.FlywheelStatus {
  return {
    namespace: "global",
    observations,
    active_patterns: 0,
    staged_patterns: 0,
    forgotten_patterns: 0,
    autooptimizer_runs: 0,
    latest_autooptimizer_run_id: null,
    latest_autooptimizer_created_at: null,
  };
}

function renderPanel(observations: number) {
  vi.mocked(flywheelApi.getFlywheelStatus).mockResolvedValue(
    makeFlywheelStatusWith(observations),
  );
  vi.mocked(flywheelApi.getFlywheelVelocity).mockResolvedValue({
    namespace: "global",
    days: 7,
    since: "2026-05-18T00:00:00Z",
    observations_captured: 0,
    patterns_promoted: 0,
    patterns_demoted: 0,
    autooptimizer_runs: 0,
    optimized_child_agents: 0,
    average_lineage_depth: 0,
    latest_activity_at: null,
  });
  vi.mocked(flywheelApi.getFlywheelLineage).mockResolvedValue({
    namespace: "global",
    total: 0,
    items: [],
  });
  vi.mocked(flywheelApi.listAutoOptimizerRuns).mockResolvedValue({
    items: [],
    total: 0,
  });

  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <FlywheelPanel mode="workspace" />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(memoryApi.listMemory).mockResolvedValue({ items: [], total: 0 });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ── bead xvision-5jzr: Stage Pattern precondition guard ────────────────────

describe("FlywheelPanel — Stage Pattern button precondition (bead xvision-5jzr)", () => {
  it("disables Stage Pattern button when observations < 2 (0 observations)", async () => {
    renderPanel(0);

    // Wait for the button to become disabled after the status query resolves.
    // Before the query resolves, the button is enabled (loading state); after
    // the status loads with 0 observations, the guard kicks in.
    await waitFor(() => {
      const btn = screen.getByRole("button", { name: /Stage Pattern/i });
      expect(btn).toBeDisabled();
    });
  });

  it("disables Stage Pattern button when observations === 1", async () => {
    renderPanel(1);

    await waitFor(() => {
      const btn = screen.getByRole("button", { name: /Stage Pattern/i });
      expect(btn).toBeDisabled();
    });
  });

  it("provides a tooltip on the disabled button explaining the requirement", async () => {
    renderPanel(0);

    await waitFor(() => {
      const btn = screen.getByRole("button", { name: /Stage Pattern/i });
      expect(btn).toBeDisabled();
      expect(btn).toHaveAttribute("title");
      expect(btn.getAttribute("title")).toMatch(/2 observation/i);
    });
  });

  it("shows an always-visible precondition hint (not just a hover tooltip) when observations < 2", async () => {
    renderPanel(0);

    // The QA "not clickable" finding was an inertly-disabled button whose only
    // explanation was a hover-only `title`. A hint must be visible text so
    // touch/keyboard users understand the precondition without hovering.
    expect(
      await screen.findByText(/2 observations \(0 so far\)/i),
    ).toBeInTheDocument();
  });

  it("does NOT disable Stage Pattern button when observations >= 2", async () => {
    renderPanel(2);

    // Button should appear and remain enabled (no mutation in flight).
    // Use findByRole to wait for the button to exist, then confirm not disabled.
    const btn = await screen.findByRole("button", { name: /Stage Pattern/i });

    // Give the status query time to resolve — confirm button stays enabled.
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /Stage Pattern/i }),
      ).not.toBeDisabled();
    });

    expect(btn).not.toBeDisabled();
  });

  it("does not call runAutoOptimizer when button is disabled due to insufficient observations", async () => {
    const user = userEvent.setup();
    renderPanel(0);

    // Wait for the guard to engage after status loads.
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /Stage Pattern/i }),
      ).toBeDisabled();
    });

    const btn = screen.getByRole("button", { name: /Stage Pattern/i });

    // Attempt to click — should be a no-op because the button is disabled
    await user.click(btn);

    expect(flywheelApi.runAutoOptimizer).not.toHaveBeenCalled();
  });

  it("does not surface the raw backend error string when observations < 2", async () => {
    // Simulate the backend returning the raw validation error.
    // Even if the button guard were bypassed, the friendly message must appear.
    vi.mocked(flywheelApi.runAutoOptimizer).mockRejectedValue(
      new Error("not enough Observations for autooptimizer: found 0, need 2"),
    );

    renderPanel(0);

    // Wait for the guard to engage.
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /Stage Pattern/i }),
      ).toBeDisabled();
    });

    // The raw backend string should never appear in the document.
    expect(
      screen.queryByText(/not enough Observations for autooptimizer/i),
    ).not.toBeInTheDocument();
  });
});
