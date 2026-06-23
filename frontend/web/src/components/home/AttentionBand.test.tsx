// frontend/web/src/components/home/AttentionBand.test.tsx

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import { AttentionBand } from "./AttentionBand";

vi.mock("@/api/agent-runs", () => ({
  agentRunKeys: { list: vi.fn(() => ["agent-runs"]) },
}));
vi.mock("@/api/agent-runs", () => ({
  agentRunKeys: { list: vi.fn(() => ["agent-runs"]) },
  listAgentRuns: vi.fn(() => Promise.resolve([])),
}));
vi.mock("@/api/live-deployments", () => ({
  deploymentKeys: { list: vi.fn(() => ["deployments"]) },
  listDeployments: vi.fn(() => Promise.resolve([])),
}));

describe("AttentionBand", () => {
  function renderBand(nagItems: any[] = []) {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <MemoryRouter>
        <QueryClientProvider client={client}>
          <AttentionBand nagItems={nagItems} deployments={[]} />
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

  it("renders nag items when present and hides the strip when clean", () => {
    const { unmount } = renderBand([
      {
        tone: "warn" as const,
        title: "1 provider missing API key",
        detail: "OpenAI → OPENAI_API_KEY",
        link: { to: "/settings/providers", label: "configure" },
      },
    ]);
    expect(screen.getByTestId("nag-strip")).toHaveTextContent(
      /1 provider missing api key/i,
    );
    unmount();

    renderBand([]);
    expect(screen.queryByTestId("nag-strip")).toBeNull();
  });
});
