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
  test("loads the run and renders the waterfall timeline + inspector", async () => {
    renderAt("/agent-runs/run_abc1234");
    await waitFor(() => expect(screen.getByText(/Improve BTC/)).toBeInTheDocument());
    const rows = screen.getAllByTestId(/^span-row-/);
    expect(rows.length).toBeGreaterThan(0);
    // Redundant rail-tree column was removed.
    expect(screen.queryAllByTestId(/^rail-node-/)).toHaveLength(0);
    // Each row pairs with a positioned waterfall bar.
    const bars = screen.getAllByTestId(/^span-waterfall-bar-/);
    expect(bars.length).toBe(rows.length);
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

  test("does not render the loud full_debug banner for hash_only runs", async () => {
    renderAt("/agent-runs/run_abc1234");
    await screen.findByTestId("retention-badge");
    expect(screen.queryByTestId("retention-banner")).not.toBeInTheDocument();
  });

  test("never renders the loud full_debug banner — badge is the only surface", async () => {
    // qa-ui-polish-round2 #10: the role="alert" Card was removed. The
    // retention mode is communicated by the minimal Pill above, and
    // Settings → Retention is the canonical control. Confirm the banner
    // does NOT render even for a full_debug run.
    renderAt("/agent-runs/run_debug42");
    const badge = await screen.findByTestId("retention-badge");
    expect(badge).toHaveTextContent(/full_debug/);
    expect(screen.queryByTestId("retention-banner")).not.toBeInTheDocument();
  });
});
