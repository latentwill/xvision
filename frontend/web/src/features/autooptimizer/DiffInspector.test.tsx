import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { apiFetch } from "@/api/client";
import { DiffInspector } from "./DiffInspector";

vi.mock("@/api/client", () => {
  return {
    apiFetch: vi.fn(),
  };
});

function renderDiff() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/autooptimizer/diff/childhash"]}>
        <Routes>
          <Route path="/autooptimizer/diff/:hash" element={<DiffInspector />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DiffInspector", () => {
  it("loads parent and experiment blobs and renders changed fields", async () => {
    vi.mocked(apiFetch).mockImplementation(async (path: string) => {
      if (path === "/api/autooptimizer/lineage/childhash") {
        return {
          bundle_hash: "childhash",
          parent_hash: "parenthash",
          gate_verdict: "passed",
          status: "active",
          cycle_id: "cycle-1",
          created_at: "2026-06-02T00:00:00Z",
          diversity_score: 0.4,
        };
      }
      if (path === "/api/autooptimizer/blob/parenthash") {
        return { manifest: { id: "parent" }, risk: { max_position_pct: 10 } };
      }
      return { manifest: { id: "child" }, risk: { max_position_pct: 6 } };
    });

    renderDiff();

    expect(await screen.findByText("risk.max_position_pct")).toBeInTheDocument();
    expect(screen.getByText("10")).toBeInTheDocument();
    expect(screen.getByText("6")).toBeInTheDocument();
  });
});
