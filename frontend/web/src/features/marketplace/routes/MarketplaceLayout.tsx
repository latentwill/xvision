// src/features/marketplace/routes/MarketplaceLayout.tsx
import { Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { TestnetBanner } from "@/features/marketplace/components/TestnetBadge";

// Phase F mounts the fixture client. Phase 6 / C7 swaps this one line for a
// real on-chain MarketplaceData impl once contracts are deployed.
const client = new FixtureMarketplaceData();

export function MarketplaceLayout() {
  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative space-y-4">
        <TestnetBanner />
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
