// src/features/marketplace/routes/MarketplaceLayout.tsx
import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import {
  FixtureMarketplaceData,
  type MarketplaceData,
} from "@/features/marketplace/data/MarketplaceData";
import { chooseMarketplaceData } from "@/features/marketplace/data/ApiMarketplaceData";
import { TestnetBanner } from "@/features/marketplace/components/TestnetBadge";

// Default to fixtures (renders immediately, no flash); one status probe on
// mount upgrades to the real indexer-backed client when it's active.
// `chooseMarketplaceData` never rejects — if the probe fails (indexer off,
// jsdom, network down) it resolves to the same fixture instance, so the
// setState is a no-op.
const fixtureClient = new FixtureMarketplaceData();

export function MarketplaceLayout() {
  const [client, setClient] = useState<MarketplaceData>(fixtureClient);

  useEffect(() => {
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
