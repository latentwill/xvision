// useMarketplaceNetwork — runtime network for cosmetic surfaces.
//
// The testnet badge/banner and the mint-step copy must reflect the network the
// BACKEND is actually on (from /api/marketplace/status), not the build-time
// VITE_MARKETPLACE_NETWORK — otherwise a prebuilt bundle deployed against a
// different chain shows the wrong "testnet/mainnet" label.
//
// Plain useState/useEffect (NOT useQuery) so it needs no QueryClient and works
// in every render context: it renders the build-time default immediately, then
// updates once the backend status resolves. Uses the LENIENT resolver, so a
// status outage just keeps the build-time default.
import { useEffect, useState } from "react";
import { getActiveNetworkConfigOrDefault, isMainnetNetwork } from "./chain";

export function useMarketplaceNetwork(): { isMainnet: boolean } {
  const [isMainnet, setIsMainnet] = useState<boolean>(() => isMainnetNetwork());
  useEffect(() => {
    let cancelled = false;
    void getActiveNetworkConfigOrDefault().then((net) => {
      if (!cancelled) setIsMainnet(net.chain.id === 5000);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return { isMainnet };
}
