// frontend/web/src/routes/agent-runs-detail.test.tsx
import { describe, expect, test } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

  test("inspector selection falls back to the first filtered span", async () => {
    renderAt("/agent-runs/run_abc1234");
    await waitFor(() => expect(screen.getByText(/Improve BTC/)).toBeInTheDocument());
    await screen.findByTestId("span-row-s1");

    await userEvent.click(screen.getByRole("button", { name: /^MODEL$/i }));

    expect(screen.queryByText("s1")).not.toBeInTheDocument();
    expect(await screen.findByText("s3")).toBeInTheDocument();
  });

  test("renders the retention badge with the run's retention_mode", async () => {
    renderAt("/agent-runs/run_abc1234");
    const badge = await screen.findByTestId("retention-badge");
    expect(badge).toHaveTextContent(/hash_only/);
  });

  test("does not render the full_debug banner for hash_only runs", async () => {
    renderAt("/agent-runs/run_abc1234");
    await screen.findByTestId("retention-badge");
    expect(screen.queryByTestId("retention-banner")).not.toBeInTheDocument();
  });

  test("renders the full_debug banner when retention_mode is full_debug", async () => {
    renderAt("/agent-runs/run_debug42");
    const banner = await screen.findByTestId("retention-banner");
    expect(banner).toHaveTextContent(
      /Recorded under full_debug retention — prompts and tool payloads stored on disk\./,
    );
    const badge = await screen.findByTestId("retention-badge");
    expect(badge).toHaveTextContent(/full_debug/);
  });
});
