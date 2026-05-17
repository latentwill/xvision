// frontend/web/src/routes/agent-runs-detail.test.tsx
import { describe, expect, test } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { AgentRunDetailRoute } from "./agent-runs-detail";

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/agent-runs/:runId" element={<AgentRunDetailRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("AgentRunDetailRoute", () => {
  test("loads the run and renders rail-tree + timeline + inspector", async () => {
    renderAt("/agent-runs/run_abc1234");
    await waitFor(() => expect(screen.getByText(/Improve BTC/)).toBeInTheDocument());
    expect(screen.getAllByTestId(/^rail-node-/).length).toBeGreaterThan(0);
    expect(screen.getAllByTestId(/^span-row-/).length).toBeGreaterThan(0);
  });

  test("renders an error state for unknown id", async () => {
    renderAt("/agent-runs/missing");
    await waitFor(() => expect(screen.getByText(/not found/i)).toBeInTheDocument());
  });
});
