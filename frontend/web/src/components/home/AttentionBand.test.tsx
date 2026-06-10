import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { AttentionBand } from "./AttentionBand";

vi.mock("@/api/agent-runs", () => ({
  agentRunKeys: {
    all: ["agent-runs"],
    list: (p?: unknown) => ["agent-runs", "list", p ?? {}],
  },
  listAgentRuns: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/api/eval", () => ({
  evalKeys: { runs: (p?: unknown) => ["eval", "runs", p ?? {}] },
  listRuns: vi.fn().mockResolvedValue([]),
  cancelRun: vi.fn(),
}));

vi.mock("@/api/eval-review", () => ({
  listCriticalFindings: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/features/autooptimizer/api", () => ({
  useOptimizerStatus: vi.fn(() => undefined),
  usePauseCycle: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  useResumeCycle: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
}));

function strategy(over: Partial<StrategyListItem>): StrategyListItem {
  return {
    agent_id: "strat-1",
    display_name: "Strategy One",
    template: "trend_follower",
    decision_cadence_minutes: 60,
    ...over,
  };
}

function renderBand(
  strategies: StrategyListItem[] = [],
  runs: RunSummary[] = [],
  nagItems: Parameters<typeof AttentionBand>[0]["nagItems"] = [],
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <AttentionBand runs={runs} strategies={strategies} nagItems={nagItems} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("AttentionBand", () => {
  it("renders the live summary and critical findings rows inside one card", async () => {
    renderBand();
    expect(await screen.findByTestId("live-summary-strip")).toBeInTheDocument();
    expect(screen.getByTestId("critical-findings-row")).toBeInTheDocument();
    expect(screen.getByTestId("attention-band")).toBeInTheDocument();
  });

  it("phrases awaiting-first-eval as a routed next action, segmented by origin", () => {
    renderBand([
      strategy({ agent_id: "u1" }),
      strategy({ agent_id: "u2" }),
      strategy({ agent_id: "o1", origin: "optimizer" }),
    ]);
    const action = screen.getByTestId("awaiting-eval-action");
    expect(action).toHaveTextContent(
      /evaluate 2 user strategies awaiting first eval/i,
    );
    expect(action).toHaveTextContent(
      /1 optimizer-generated \(evaluated in lineage\)/i,
    );
    const link = screen.getByRole("link", {
      name: /evaluate 2 user strategies/i,
    });
    expect(link).toHaveAttribute("href", "/eval-runs");
  });

  it("omits the awaiting action when every user strategy is evaluated", () => {
    renderBand([strategy({ agent_id: "done", evaluated: true })]);
    expect(screen.queryByTestId("awaiting-eval-action")).toBeNull();
  });

  it("renders nag items when present and hides the strip when clean", () => {
    const { unmount } = renderBand([], [], [
      {
        tone: "warn",
        title: "1 provider missing API key",
        detail: "OpenAI → OPENAI_API_KEY",
        link: { to: "/settings/providers", label: "configure" },
      },
    ]);
    expect(screen.getByTestId("nag-strip")).toHaveTextContent(
      /1 provider missing api key/i,
    );
    unmount();

    renderBand([], [], []);
    expect(screen.queryByTestId("nag-strip")).toBeNull();
  });
});
