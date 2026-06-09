// src/features/marketplace/routes/MarketplaceLayout.tsx
import { Navigate, Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { useMarketplaceOptIn } from "@/features/marketplace/lib/optin";
import { TestnetBanner } from "@/features/marketplace/components/TestnetBadge";

// Phase F mounts the fixture client. Phase 6 / C7 swaps this one line for a
// real on-chain MarketplaceData impl once contracts are deployed.
const client = new FixtureMarketplaceData();

export function MarketplaceLayout() {
  const { enabled } = useMarketplaceOptIn();

  // C8 gate: marketplace is opt-in (default OFF, Settings → Marketplace).
  // When off, every /marketplace/* deep-link redirects cleanly to the opt-in
  // tab rather than erroring or rendering the fixture surface.
  if (!enabled) {
    return <Navigate to="/settings/marketplace" replace />;
  }

  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative space-y-4">
        <TestnetBanner />
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
