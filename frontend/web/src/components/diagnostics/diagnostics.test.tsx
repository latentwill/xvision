// Tests for the Phase 4.5 strategy-agent diagnostics + mint UI surfaces.
//
// Asserts:
//  - pure api/diagnostics helpers (isBlocker / remediationFor / statusLabel);
//  - CapabilityBadges renders trader/filter as live, critic/intern/router muted;
//  - AgentDiagnosticsView renders the readiness verdict + per-slot statuses
//    with remediation for blockers (and flips to "Not ready" on a blocker);
//  - StrategyReadinessPanel surfaces the launchable verdict + unmet blockers
//    BEFORE launch, with remediation copy;
//  - MintLineagePanel surfaces a typed MintRefusal (mint_missing_eval_proof
//    style) and the attested decision on success;
//  - none of these surfaces render a dialog/modal role (no-popup rule).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  render,
  screen,
  fireEvent,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  isBlocker,
  remediationFor,
  statusLabel,
  getAgentDiagnostics,
  getStrategyDiagnostics,
  type AgentDiagnostics,
  type StrategyDiagnostics,
} from "@/api/diagnostics";
import { mintOptimization } from "@/api/optimizations";
import { ApiError } from "@/api/client";
import { CapabilityBadges } from "./CapabilityBadges";
import { AgentDiagnosticsView } from "./AgentDiagnosticsView";
import { StrategyReadinessPanel } from "./StrategyReadinessPanel";
import { MintLineagePanel } from "./MintLineagePanel";

vi.mock("@/api/diagnostics", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/diagnostics")>(
      "@/api/diagnostics",
    );
  return {
    ...actual,
    getAgentDiagnostics: vi.fn(),
    getStrategyDiagnostics: vi.fn(),
  };
});

vi.mock("@/api/optimizations", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/optimizations")>(
      "@/api/optimizations",
    );
  return {
    ...actual,
    mintOptimization: vi.fn(),
  };
});

function makeQC() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function withQC(ui: React.ReactElement) {
  return <QueryClientProvider client={makeQC()}>{ui}</QueryClientProvider>;
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ── pure helpers ──────────────────────────────────────────────────────────

describe("api/diagnostics helpers", () => {
  it("isBlocker is true for the four hard blockers only", () => {
    expect(isBlocker({ kind: "missing_prompt" })).toBe(true);
    expect(isBlocker({ kind: "missing_model_binding" })).toBe(true);
    expect(isBlocker({ kind: "missing_tool", tool: "ohlcv" })).toBe(true);
    expect(isBlocker({ kind: "unsupported" })).toBe(true);
    expect(isBlocker({ kind: "ready" })).toBe(false);
    expect(isBlocker({ kind: "optimizable" })).toBe(false);
    expect(isBlocker({ kind: "optional" })).toBe(false);
  });

  it("remediationFor names the missing tool", () => {
    expect(
      remediationFor({ kind: "missing_tool", tool: "ohlcv" }, "trader"),
    ).toContain("ohlcv");
    expect(remediationFor({ kind: "ready" }, "trader")).toBe("");
  });

  it("statusLabel maps the missing_tool variant", () => {
    expect(statusLabel({ kind: "missing_tool", tool: "x" })).toBe(
      "Missing tool",
    );
  });
});

// ── CapabilityBadges ────────────────────────────────────────────────────────

describe("CapabilityBadges", () => {
  it("renders trader as a live (gold) badge and router as muted", () => {
    render(<CapabilityBadges capabilities={["trader", "router"]} />);
    expect(screen.getByTestId("cap-badge-trader")).toBeTruthy();
    expect(screen.getByTestId("cap-badge-router")).toBeTruthy();
    // Trader is supported at runtime; router is not.
    expect(screen.getByTestId("cap-badge-trader").getAttribute("title")).toMatch(
      /supported/,
    );
    expect(screen.getByTestId("cap-badge-router").getAttribute("title")).toMatch(
      /no runtime handler/,
    );
  });

  it("shows an empty hint when there are no capabilities", () => {
    render(<CapabilityBadges capabilities={[]} />);
    expect(screen.getByTestId("cap-badges-empty")).toBeTruthy();
  });
});

// ── AgentDiagnosticsView ────────────────────────────────────────────────────

function readyAgentDiag(): AgentDiagnostics {
  return {
    agent_id: "ag1",
    agent_name: "Ready Trader",
    slots: [
      {
        slot_name: "trader",
        model_bound: true,
        prompt_present: true,
        declared: ["trader"],
        capabilities: [
          {
            capability: "trader",
            status: { kind: "optimizable" },
            required_tools: ["ohlcv"],
            optimizable: true,
          },
        ],
      },
    ],
    declared_capabilities: ["trader"],
    optimizable_capabilities: ["trader"],
    agent_ready: true,
  };
}

function blockedAgentDiag(): AgentDiagnostics {
  return {
    agent_id: "ag2",
    agent_name: "Promptless",
    slots: [
      {
        slot_name: "trader",
        model_bound: true,
        prompt_present: false,
        declared: ["trader"],
        capabilities: [
          {
            capability: "trader",
            status: { kind: "missing_prompt" },
            required_tools: ["ohlcv"],
            optimizable: true,
          },
        ],
      },
    ],
    declared_capabilities: ["trader"],
    optimizable_capabilities: ["trader"],
    agent_ready: false,
  };
}

describe("AgentDiagnosticsView", () => {
  it("renders the ready verdict and optimizable hint", async () => {
    vi.mocked(getAgentDiagnostics).mockResolvedValue(readyAgentDiag());
    render(withQC(<AgentDiagnosticsView agentId="ag1" />));
    await waitFor(() => screen.getByTestId("agent-ready"));
    expect(screen.getByTestId("slot-diag-trader")).toBeTruthy();
    expect(screen.getByTestId("cap-status-optimizable")).toBeTruthy();
  });

  it("flips to not-ready and shows remediation for a blocker", async () => {
    vi.mocked(getAgentDiagnostics).mockResolvedValue(blockedAgentDiag());
    render(withQC(<AgentDiagnosticsView agentId="ag2" />));
    await waitFor(() => screen.getByTestId("agent-not-ready"));
    expect(screen.getByTestId("cap-status-missing_prompt")).toBeTruthy();
    // Remediation copy for a missing prompt is rendered inline.
    expect(screen.getByText(/Add a system prompt/i)).toBeTruthy();
  });

  it("does not render a dialog/modal (no-popup rule)", async () => {
    vi.mocked(getAgentDiagnostics).mockResolvedValue(readyAgentDiag());
    const { container } = render(withQC(<AgentDiagnosticsView agentId="ag1" />));
    await waitFor(() => screen.getByTestId("agent-ready"));
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });
});

// ── StrategyReadinessPanel ──────────────────────────────────────────────────

function launchableStrategy(): StrategyDiagnostics {
  return {
    strategy_id: "st1",
    per_agent: [
      {
        role: "trader",
        agent_id: "ag1",
        agent_name: "Trader",
        agent_resolved: true,
        declared: ["trader"],
        required: "trader",
        capabilities: [
          {
            capability: "trader",
            status: { kind: "optimizable" },
            required: true,
            required_tools: ["ohlcv"],
            optimizable: true,
          },
        ],
      },
    ],
    required_capabilities: ["trader"],
    required_unmet: [],
    optimizable: ["trader"],
    launchable: true,
  };
}

function blockedStrategy(): StrategyDiagnostics {
  return {
    strategy_id: "st2",
    per_agent: [
      {
        role: "trader",
        agent_id: "ag1",
        agent_name: "Trader",
        agent_resolved: true,
        declared: ["trader"],
        required: "trader",
        capabilities: [
          {
            capability: "trader",
            status: { kind: "missing_tool", tool: "ohlcv" },
            required: true,
            required_tools: ["ohlcv"],
            optimizable: true,
          },
        ],
      },
    ],
    required_capabilities: ["trader"],
    required_unmet: [
      {
        role: "trader",
        agent_id: "ag1",
        capability: "trader",
        status: { kind: "missing_tool", tool: "ohlcv" },
      },
    ],
    optimizable: [],
    launchable: false,
  };
}

describe("StrategyReadinessPanel", () => {
  it("shows the launchable verdict when ready", async () => {
    vi.mocked(getStrategyDiagnostics).mockResolvedValue(launchableStrategy());
    render(withQC(<StrategyReadinessPanel strategyId="st1" />));
    await waitFor(() => screen.getByTestId("strategy-launchable"));
    expect(screen.getByTestId("agent-readiness-trader")).toBeTruthy();
  });

  it("surfaces the unmet blocker + remediation BEFORE launch", async () => {
    vi.mocked(getStrategyDiagnostics).mockResolvedValue(blockedStrategy());
    render(withQC(<StrategyReadinessPanel strategyId="st2" />));
    await waitFor(() => screen.getByTestId("strategy-not-launchable"));
    expect(screen.getByTestId("strategy-unmet")).toBeTruthy();
    expect(screen.getByTestId("unmet-trader")).toBeTruthy();
    // The missing-tool remediation names the tool to grant. It appears in
    // both the unmet-summary list and the per-agent card, so assert ≥1.
    expect(screen.getAllByText(/Grant the "ohlcv" tool/i).length).toBeGreaterThan(0);
  });
});

// ── MintLineagePanel ────────────────────────────────────────────────────────

describe("MintLineagePanel", () => {
  it("surfaces a typed mint refusal with remediation", async () => {
    vi.mocked(mintOptimization).mockRejectedValue(
      new ApiError(422, "mint_missing_eval_proof", "no eval proof"),
    );
    render(
      withQC(
        <MintLineagePanel
          runId="run1"
          capability="trader"
          childAgentId="child1"
        />,
      ),
    );
    fireEvent.change(screen.getByTestId("mint-eval-run-id"), {
      target: { value: "ev1" },
    });
    fireEvent.click(screen.getByTestId("mint-button"));
    await waitFor(() => screen.getByTestId("mint-refusal"));
    expect(screen.getByText(/mint_missing_eval_proof/)).toBeTruthy();
    expect(screen.getByText(/Provide an eval run id/i)).toBeTruthy();
  });

  it("shows the attested decision on a successful mint", async () => {
    vi.mocked(mintOptimization).mockResolvedValue({
      decision: {
        child_agent_id: "child1",
        capability: "trader",
        eval_run_id: "ev1",
        overfit_waived: false,
        holdout_snapshot_id: "snap1",
      },
    });
    render(
      withQC(
        <MintLineagePanel
          runId="run1"
          capability="trader"
          childAgentId="child1"
        />,
      ),
    );
    fireEvent.change(screen.getByTestId("mint-eval-run-id"), {
      target: { value: "ev1" },
    });
    fireEvent.click(screen.getByTestId("mint-button"));
    await waitFor(() => screen.getByTestId("mint-decision"));
    expect(screen.getByText(/Provenance attested/i)).toBeTruthy();
    expect(mintOptimization).toHaveBeenCalledWith("run1", {
      childAgentId: "child1",
      evalRunId: "ev1",
      evalMetric: "sharpe",
      metricsPresent: [
        "forward_return_agreement",
        "sharpe",
        "max_drawdown",
        "profit_factor",
        "calibration",
        "action_validity",
        "selectivity",
        "net_of_cost",
      ],
    });
  });

  it("surfaces non-API mint failures", async () => {
    vi.mocked(mintOptimization).mockRejectedValue(new Error("network down"));
    render(
      withQC(
        <MintLineagePanel
          runId="run1"
          capability="trader"
          childAgentId="child1"
        />,
      ),
    );
    fireEvent.change(screen.getByTestId("mint-eval-run-id"), {
      target: { value: "ev1" },
    });
    fireEvent.click(screen.getByTestId("mint-button"));
    await waitFor(() => screen.getByTestId("mint-generic-error"));
    expect(screen.getByText(/network down/i)).toBeTruthy();
  });

  it("does not render a dialog/modal (no-popup rule)", () => {
    const { container } = render(
      withQC(
        <MintLineagePanel
          runId="run1"
          capability="trader"
          childAgentId="child1"
        />,
      ),
    );
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });
});
