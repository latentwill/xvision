// frontend/web/src/components/home/AttentionBand.test.tsx

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import { AttentionBand } from "./AttentionBand";

vi.mock("@/api/agent-runs", () => ({
  agentRunKeys: { list: vi.fn(() => ["agent-runs"]) },
  listAgentRuns: vi.fn(() => Promise.resolve([])),
}));
vi.mock("@/api/live-deployments", () => ({
  deploymentKeys: { list: vi.fn(() => ["deployments"]) },
  listDeployments: vi.fn(() => Promise.resolve([])),
}));

describe("AttentionBand", () => {
  function renderBand() {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <MemoryRouter>
        <QueryClientProvider client={client}>
          <AttentionBand deployments={[]} nagItems={[]} />
        </QueryClientProvider>
      </MemoryRouter>,
    );
  }

  it("renders the live summary strip", async () => {
    renderBand();
    expect(
      await screen.findByTestId("live-summary-strip"),
    ).toBeInTheDocument();
  });
});
