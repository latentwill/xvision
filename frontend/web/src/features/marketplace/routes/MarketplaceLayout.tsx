// src/features/marketplace/routes/MarketplaceLayout.tsx
import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import {
  FixtureMarketplaceData,
  type MarketplaceData,
} from "@/features/marketplace/data/MarketplaceData";
import { chooseMarketplaceData } from "@/features/marketplace/data/ApiMarketplaceData";
import { SubgraphMarketplaceData } from "@/features/marketplace/data/SubgraphMarketplaceData";
import {
  createSubgraphClient,
  subgraphUrl,
} from "@/features/marketplace/data/subgraph/client";
import { TestnetBanner } from "@/features/marketplace/components/TestnetBadge";

// Data-source selection, layered (merge of the subgraph track #908 and the
// dashboard-indexer track #912):
//   1. VITE_MARKETPLACE_SUBGRAPH_URL set → SubgraphMarketplaceData (Goldsky,
//      runbook §3.2 / C7) — explicit build-time opt-in wins.
//   2. Otherwise default to fixtures (renders immediately, no flash) and run
//      one /api/marketplace/status probe on mount; when the dashboard indexer
//      is active, upgrade to ApiMarketplaceData. `chooseMarketplaceData` never
//      rejects — probe failure (indexer off, jsdom, network down) resolves to
//      the same fixture instance, so the setState is a no-op.
const fixtureClient = new FixtureMarketplaceData();

function subgraphClient(): MarketplaceData | null {
  const url = subgraphUrl();
  if (!url) return null;
  return new SubgraphMarketplaceData({ client: createSubgraphClient(url) });
}

export function MarketplaceLayout() {
  const [client, setClient] = useState<MarketplaceData>(
    () => subgraphClient() ?? fixtureClient,
  );

  useEffect(() => {
    if (subgraphUrl()) return; // subgraph opt-in wins; no probe
    let cancelled = false;
    void chooseMarketplaceData(fixtureClient).then((c) => {
      if (!cancelled) setClient(c);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative space-y-4">
        <TestnetBanner />
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
