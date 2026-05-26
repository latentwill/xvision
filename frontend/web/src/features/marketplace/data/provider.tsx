// src/features/marketplace/data/provider.tsx
import { createContext, useContext, type ReactNode } from "react";
import type { MarketplaceData } from "./MarketplaceData";

const Ctx = createContext<MarketplaceData | null>(null);

export function MarketplaceDataProvider({
  client,
  children,
}: {
  client: MarketplaceData;
  children: ReactNode;
}) {
  return <Ctx.Provider value={client}>{children}</Ctx.Provider>;
}

export function useMarketplaceData(): MarketplaceData {
  const mp = useContext(Ctx);
  if (!mp) throw new Error("useMarketplaceData must be used within a MarketplaceDataProvider");
  return mp;
}
