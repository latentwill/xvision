import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  getAgentDiagnostics,
  getStrategyDiagnostics,
  type AgentDiagnostics,
  type StrategyDiagnostics,
} from "@/api/diagnostics";
import { AgentDiagnosticsView } from "./AgentDiagnosticsView";
import { StrategyReadinessPanel } from "./StrategyReadinessPanel";

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

function withQC(ui: React.ReactElement) {
  return (
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      {ui}
    </QueryClientProvider>
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function agentDiag(): AgentDiagnostics {
  return {
    agent_id: "ag1",
    agent_name: "Ready Trader",
    agent_ready: true,
    tool_names: ["ohlcv", "submit_decision"],
    slots: [
      {
        slot_name: "trader",
        model_bound: true,
        prompt_present: true,
        tools: [
          { name: "ohlcv", registered: true, description: "OHLCV history" },
          { name: "submit_decision", registered: true, description: "Submit decision" },
        ],
      },
    ],
  };
}

function strategyDiag(): StrategyDiagnostics {
  return {
    strategy_id: "st1",
    launchable: true,
    has_decision_path: true,
    unregistered_tools: [],
    per_agent: [
      {
        role: "trader",
        agent_id: "ag1",
        agent_name: "Ready Trader",
        agent_resolved: true,
        tools: [
          { name: "ohlcv", registered: true, description: "OHLCV history" },
          { name: "submit_decision", registered: true, description: "Submit decision" },
        ],
      },
    ],
  };
}

describe("tool diagnostics views", () => {
  it("renders agent tool readiness", async () => {
    vi.mocked(getAgentDiagnostics).mockResolvedValue(agentDiag());
    render(withQC(<AgentDiagnosticsView agentId="ag1" />));
    await waitFor(() => screen.getByText("Agent ready"));
    expect(screen.getByText("ohlcv")).toBeTruthy();
    expect(screen.getByText("submit_decision")).toBeTruthy();
  });

  it("renders strategy tool readiness", async () => {
    vi.mocked(getStrategyDiagnostics).mockResolvedValue(strategyDiag());
    render(withQC(<StrategyReadinessPanel strategyId="st1" />));
    await waitFor(() => screen.getByText("Ready to launch"));
    expect(screen.getByText("At least one slot can submit decisions.")).toBeTruthy();
    expect(screen.getByText("submit_decision")).toBeTruthy();
  });
});
