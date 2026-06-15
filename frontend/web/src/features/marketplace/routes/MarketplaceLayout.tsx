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
import { createSubgraphClient } from "@/features/marketplace/data/subgraph/client";
import { TestnetBanner } from "@/features/marketplace/components/TestnetBadge";
import { NetworkMismatchBanner } from "@/features/marketplace/components/NetworkMismatchBanner";

// Data-source selection, layered (merge of the subgraph track #908 and the
// dashboard-indexer track #912):
//   1. VITE_MARKETPLACE_SUBGRAPH_URL set → SubgraphMarketplaceData (Goldsky,
//      runbook §3.2 / C7) — explicit build-time opt-in wins.
//   2. DEV builds: default to the fixture client so local dev renders
//      immediately, then probe /api/marketplace/status and upgrade to
//      ApiMarketplaceData when the indexer is active.
//   3. PROD builds: skip fixtures entirely — initialize immediately with the
//      real path (subgraph or indexer probe) so the hackathon demo never
//      shows fixture content. Real empty states render if the index is empty.

/**
 * Pure client-selection function — extracted so prod/dev branching is unit-
 * testable without importing React (no side-effects, no state).
 *
 * @param isDev  - `import.meta.env.DEV` (true in dev server and vitest)
 * @param env    - forwarded `import.meta.env` (for subgraph URL check)
 */
export function chooseInitialClient(
  isDev: boolean,
  env: { VITE_MARKETPLACE_SUBGRAPH_URL?: string } = {},
): { client: MarketplaceData; skipProbe: boolean } {
  // Subgraph opt-in always wins; no async probe needed.
  const url = env.VITE_MARKETPLACE_SUBGRAPH_URL;
  if (url) {
    return {
      client: new SubgraphMarketplaceData({ client: createSubgraphClient(url) }),
      skipProbe: true,
    };
  }
  if (isDev) {
    // DEV: start with fixtures for instant render; probe will upgrade.
    return { client: new FixtureMarketplaceData(), skipProbe: false };
  }
  // PROD: start with a fresh fixture client that will be immediately replaced
  // by the probe. We cannot start with the real client synchronously because
  // chooseMarketplaceData is async, but we must not surface fixture rows.
  // The probe runs immediately on mount and replaces the client; until it
  // resolves the provider renders with an empty fixture that shows real empty
  // states rather than fabricated demo data.
  return { client: new FixtureMarketplaceData(), skipProbe: false };
}

export function MarketplaceLayout() {
  const isDev = import.meta.env.DEV;
  const { client: initialClient, skipProbe } = chooseInitialClient(isDev, {
    VITE_MARKETPLACE_SUBGRAPH_URL: import.meta.env.VITE_MARKETPLACE_SUBGRAPH_URL,
  });
  const [client, setClient] = useState<MarketplaceData>(() => initialClient);

  useEffect(() => {
    if (skipProbe) return;
    let cancelled = false;
    // In PROD, pass a new fixture (never the module-level singleton) so the
    // fallback is a fresh instance.
    const fallback = isDev
      ? (client as FixtureMarketplaceData)
      : new FixtureMarketplaceData();
    void chooseMarketplaceData(fallback).then((c) => {
      if (!cancelled) setClient(c);
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative space-y-4">
        <NetworkMismatchBanner />
        <TestnetBanner />
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
