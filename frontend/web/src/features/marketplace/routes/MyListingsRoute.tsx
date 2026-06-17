// src/features/marketplace/routes/MyListingsRoute.tsx
// /marketplace/mine — all listings the connected viewer created.
// Single full-width column (chat-rail rule). No popups.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, apiFetch } from "@/api/client";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { OwnerListingCard } from "@/features/marketplace/components/OwnerListingCard";
import type { IndexedListing } from "@/features/marketplace/data/ApiMarketplaceData";

interface WalletData {
  address: string;
  listings: IndexedListing[];
}

export function MyListingsRoute() {
  const { address } = useWallet();
  const queryClient = useQueryClient();

  const walletQuery = useQuery<WalletData, Error>({
    queryKey: ["marketplace", "wallet", address],
    queryFn: () =>
      apiFetch<WalletData>(`/api/marketplace/wallet/${address}`),
    enabled: !!address,
    retry: false,
  });

  const indexerOffline =
    walletQuery.error instanceof ApiError &&
    walletQuery.error.status === 503;

  const listings = walletQuery.data?.listings ?? [];

  return (
    <div className="space-y-5" style={{ padding: "18px 28px 28px" }}>
      <h1 className="m-0 text-[20px] font-semibold tracking-[-0.02em]">
        My Listings
      </h1>

      {!address ? (
        <div className="px-1 py-4 font-mono text-[12px] text-text-3">
          Connect a wallet to see your listings.
        </div>
      ) : indexerOffline ? (
        <div className="border border-warn/40 bg-warn/5 rounded-[5px] px-4 py-3 font-mono text-[12px] text-warn">
          marketplace indexer offline — set XVN_RPC_URL / XVN_LISTING_REGISTRY on the server
        </div>
      ) : walletQuery.isError ? (
        <div className="border border-danger/40 bg-danger/5 rounded-[5px] px-4 py-3 font-mono text-[12px] text-danger">
          Failed to load listings: {walletQuery.error.message}
        </div>
      ) : walletQuery.isLoading ? (
        <div className="px-1 py-4 font-mono text-[12px] text-text-3">
          Loading listings…
        </div>
      ) : listings.length === 0 ? (
        <div className="border border-border rounded-[5px] px-4 py-8 text-center font-mono text-[12px] text-text-3">
          No listings published from this wallet.
        </div>
      ) : (
        <div className="border border-border rounded-[5px] overflow-hidden">
          {listings.map((l) => (
            <OwnerListingCard
              key={l.listing_id}
              listing={l}
              onChanged={() =>
                void queryClient.invalidateQueries({
                  queryKey: ["marketplace", "wallet", address],
                })
              }
            />
          ))}
        </div>
      )}
    </div>
  );
}
