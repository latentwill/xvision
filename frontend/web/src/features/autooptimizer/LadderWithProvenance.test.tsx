import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { apiFetch } from "@/api/client";
import { LadderWithProvenance } from "./LadderWithProvenance";

vi.mock("@/api/client", () => {
  return {
    apiFetch: vi.fn(),
  };
});

function renderView() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <LadderWithProvenance />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LadderWithProvenance", () => {
  it("groups recent experiments by lineage writer attribution", async () => {
    vi.mocked(apiFetch).mockImplementation(async (path: string) => {
      if (path === "/api/autooptimizer/ladder") {
        return [
          {
            provider: "anthropic",
            model: "model-a",
            prompt_version: "v1",
            proposals: 1,
            accepted: 0,
            rejected_overfit: 1,
            avg_delta_sharpe: -0.1,
          },
          {
            provider: "openai",
            model: "model-b",
            prompt_version: "v2",
            proposals: 1,
            accepted: 1,
            rejected_overfit: 0,
            avg_delta_sharpe: 0.4,
          },
        ];
      }
      return [
        {
          bundle_hash: "hashbbbbbbbb",
          parent_hash: null,
          gate_verdict: "passed",
          status: "active",
          cycle_id: "cycle-2",
          created_at: "2026-06-02T00:00:00Z",
          diversity_score: 0.5,
          writer_provider: "openai",
          writer_model: "model-b",
          writer_prompt_version: "v2",
        },
      ];
    });

    renderView();

    const provenance = await screen.findByTestId("writer-group-openai-model-b-v2");
    expect(within(provenance).getByText("hashbbbb")).toBeInTheDocument();
    expect(within(provenance).getByText(/openai/i)).toBeInTheDocument();
  });
});
