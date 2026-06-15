import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceLayout, chooseInitialClient } from "./MarketplaceLayout";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { SubgraphMarketplaceData } from "@/features/marketplace/data/SubgraphMarketplaceData";

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllEnvs();
});

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/marketplace" element={<MarketplaceLayout />}>
            <Route index element={<div>browse-content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("MarketplaceLayout", () => {
  it("renders marketplace content unconditionally", async () => {
    renderAt("/marketplace");
    expect(await screen.findByText("browse-content")).toBeInTheDocument();
  });

  it("renders the runtime-network testnet banner", async () => {
    renderAt("/marketplace");
    await screen.findByText("browse-content");
    expect(screen.getByText(/Mantle Sepolia testnet/i)).toBeInTheDocument();
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// chooseInitialClient — pure-function unit tests that cover the prod branch
// without needing React, by testing the exported helper directly.
// ──────────────────────────────────────────────────────────────────────────────

describe("chooseInitialClient", () => {
  it("DEV=true, no subgraph URL → fixture client (instant render), probe not skipped", () => {
    const { client, skipProbe } = chooseInitialClient(true, {});
    expect(client).toBeInstanceOf(FixtureMarketplaceData);
    expect(skipProbe).toBe(false);
  });

  it("DEV=false (prod), no subgraph URL → fixture client (real empty state), probe not skipped", () => {
    const { client, skipProbe } = chooseInitialClient(false, {});
    // Prod still starts with a fixture (probe replaces it immediately); the
    // important assertion is that skipProbe=false so the probe DOES run.
    expect(client).toBeInstanceOf(FixtureMarketplaceData);
    expect(skipProbe).toBe(false);
  });

  it("subgraph URL set → SubgraphMarketplaceData, probe skipped (subgraph wins)", () => {
    const { client, skipProbe } = chooseInitialClient(false, {
      VITE_MARKETPLACE_SUBGRAPH_URL: "https://api.goldsky.com/api/public/project_xxx/subgraphs/marketplace/v1/gn",
    });
    expect(client).toBeInstanceOf(SubgraphMarketplaceData);
    expect(skipProbe).toBe(true);
  });

  it("subgraph URL set in DEV → SubgraphMarketplaceData (explicit opt-in wins over dev-fixture path)", () => {
    const { client, skipProbe } = chooseInitialClient(true, {
      VITE_MARKETPLACE_SUBGRAPH_URL: "https://example.com/subgraph",
    });
    expect(client).toBeInstanceOf(SubgraphMarketplaceData);
    expect(skipProbe).toBe(true);
  });

  it("PROD: dataSource of the initial client is 'fixture' (never 'api' or 'subgraph' on boot)", () => {
    const { client } = chooseInitialClient(false, {});
    expect(client.dataSource).toBe("fixture");
  });
});
