import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { apiFetch } from "@/api/client";
import { GenealogyTree } from "./GenealogyTree";

vi.mock("@/api/client", () => {
  return {
    apiFetch: vi.fn(),
  };
});

function renderTree() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <GenealogyTree />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("GenealogyTree", () => {
  it("renders cycle-grouped parent child structure", async () => {
    vi.mocked(apiFetch).mockResolvedValue([
      {
        bundle_hash: "rootaaaaaaaa",
        parent_hash: null,
        gate_verdict: "passed",
        status: "active",
        cycle_id: "cycle-1",
        created_at: "2026-06-02T01:00:00Z",
        diversity_score: 0.1,
      },
      {
        bundle_hash: "childbbbbbbbb",
        parent_hash: "rootaaaaaaaa",
        gate_verdict: "rejected:overfit",
        status: "rejected",
        cycle_id: "cycle-1",
        created_at: "2026-06-02T02:00:00Z",
        diversity_score: 0.4,
      },
    ]);

    renderTree();

    expect(await screen.findByText("rootaaaa")).toBeInTheDocument();
    expect(screen.getByText("childbbb")).toBeInTheDocument();
    expect(screen.getByText("1 child")).toBeInTheDocument();
    expect(screen.getByText("Rejected")).toBeInTheDocument();
  });
});
