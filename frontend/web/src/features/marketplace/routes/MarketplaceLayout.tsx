// src/features/marketplace/routes/MarketplaceLayout.tsx
import { Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";

// Phase F mounts the fixture client. Phase 6 swaps this one line.
const client = new FixtureMarketplaceData();

export function MarketplaceLayout() {
  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative">
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
