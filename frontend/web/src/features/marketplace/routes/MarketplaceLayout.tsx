// src/features/marketplace/routes/MarketplaceLayout.tsx
import { useMemo } from "react";
import { Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import {
  FixtureMarketplaceData,
  type MarketplaceData,
} from "@/features/marketplace/data/MarketplaceData";
import { SubgraphMarketplaceData } from "@/features/marketplace/data/SubgraphMarketplaceData";
import {
  createSubgraphClient,
  subgraphUrl,
} from "@/features/marketplace/data/subgraph/client";
import { TestnetBanner } from "@/features/marketplace/components/TestnetBadge";

// Real client when VITE_MARKETPLACE_SUBGRAPH_URL points at the deployed Goldsky
// subgraph (runbook §3.2 / C7); the fixture otherwise. The subgraph impl backs
// the browse/detail reads from on-chain truth and delegates the off-chain /
// operator / write surface to a fixture fallback (see SubgraphMarketplaceData).
function makeClient(): MarketplaceData {
  const url = subgraphUrl();
  if (!url) return new FixtureMarketplaceData();
  return new SubgraphMarketplaceData({ client: createSubgraphClient(url) });
}

export function MarketplaceLayout() {
  const client = useMemo(makeClient, []);
  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative space-y-4">
        <TestnetBanner />
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
