// src/features/marketplace/routes/WalletRoute.tsx
// Task 5 — /marketplace/wallet: owned strategies, licenses held, and listing
// management for the connected wallet. Single full-width column (chat-rail
// rule), inline two-step revoke confirm (no-popup rule).
import { useState } from "react";
import { Link } from "react-router-dom";
import { OwnerListingCard } from "@/features/marketplace/components/OwnerListingCard";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { ApiError, apiFetch } from "@/api/client";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { faucetUsdc, usdcBalance } from "@/features/marketplace/lib/chain";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import type { IndexedListing } from "@/features/marketplace/data/ApiMarketplaceData";

// ── Backend shapes (GET /api/marketplace/wallet/:address) ────────────────────

interface WalletStrategy {
  token_id: string;
  agent_id: string;
  name: string;
  gen_art_seed: string;
  listed: boolean;
  listing_id: number | null;
}

interface WalletLicense {
  listing_id: number;
  agent_id: string;
  name: string;
  gen_art_seed: string;
  balance: number;
}

interface WalletData {
  address: string;
  strategies: WalletStrategy[];
  licenses: WalletLicense[];
  listings: IndexedListing[];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function truncAddr(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

function SectionHeading({ title, sub }: { title: string; sub?: string }) {
  return (
    <div className="px-4 py-3 border-b border-border">
      <div className="font-mono text-[13px] font-semibold text-text">
        {title}
      </div>
      {sub && (
        <div className="font-mono text-[10.5px] text-text-3 mt-0.5">{sub}</div>
      )}
    </div>
  );
}

function EmptyLine({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-4 py-6 text-center font-mono text-[12px] text-text-3">
      {children}
    </div>
  );
}

function formatUsdc6(amount6: bigint): string {
  return (Number(amount6) / 1e6).toFixed(2);
}

// ── USDC balance line + testnet faucet (inline, connected only) ──────────────

function UsdcLine({ address }: { address: string }) {
  const queryClient = useQueryClient();
  const balanceQuery = useQuery({
    queryKey: ["marketplace", "usdc-balance", address],
    queryFn: () => usdcBalance(address as `0x${string}`),
    retry: false,
  });
  const faucet = useMutation({
    mutationFn: () => faucetUsdc(10_000_000n), // 10 test USDC per click
    onSuccess: () =>
      void queryClient.invalidateQueries({
        queryKey: ["marketplace", "usdc-balance", address],
      }),
  });

  return (
    <span className="flex items-center gap-2 flex-wrap">
      <span data-testid="usdc-balance" className="font-mono text-[11px] text-text-2">
        {balanceQuery.isLoading
          ? "USDC …"
          : balanceQuery.isError
            ? "USDC unavailable"
            : `${formatUsdc6(balanceQuery.data!)} USDC`}
      </span>
      <button
        type="button"
        data-testid="wallet-faucet-btn"
        onClick={() => faucet.mutate()}
        disabled={faucet.isPending}
        className="font-mono text-[10.5px] px-2 py-0.5 border border-gold/40 text-gold rounded-[3px] hover:bg-gold/10 transition-colors disabled:opacity-60"
      >
        {faucet.isPending ? "Minting…" : "Get test USDC"}
      </button>
      {faucet.isError && (
        <span className="font-mono text-[10.5px] text-danger">
          Faucet failed:{" "}
          {faucet.error instanceof Error ? faucet.error.message : "unknown error"}
        </span>
      )}
    </span>
  );
}

// ── Wallet strip ──────────────────────────────────────────────────────────────

function WalletStrip() {
  const { address, connecting, connect, disconnect } = useWallet();
  const [error, setError] = useState<string | null>(null);

  async function handleConnect() {
    setError(null);
    try {
      await connect();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Connection failed");
    }
  }

  return (
    <div className="border border-border rounded-[5px] px-4 py-3 flex items-center gap-3 flex-wrap">
      {address ? (
        <>
          <span className="font-mono text-[13px] text-text font-semibold">
            {truncAddr(address)}
          </span>
          <span className="font-mono text-[10.5px] text-text-3">
            connected · Ethereum
          </span>
          <UsdcLine address={address} />
          <button
            type="button"
            onClick={disconnect}
            className="ml-auto font-mono text-[11px] text-text-3 hover:text-text underline underline-offset-2 transition-colors"
          >
            Disconnect
          </button>
        </>
      ) : (
        <>
          <button
            type="button"
            onClick={handleConnect}
            disabled={connecting}
            className="font-mono text-[12px] px-3 py-1.5 border border-gold/60 bg-gold/10 text-gold rounded-[4px] hover:bg-gold/20 transition-colors disabled:opacity-60"
          >
            {connecting ? "Connecting…" : "Connect Wallet"}
          </button>
          <span className="font-mono text-[11px] text-text-3">
            Connect a wallet to see the strategies you own, licenses you hold,
            and your listings.
          </span>
          {error && (
            <span className="font-mono text-[11px] text-danger">{error}</span>
          )}
        </>
      )}
    </div>
  );
}

// ── Strategy / license cards ──────────────────────────────────────────────────

function StrategyCard({ s }: { s: WalletStrategy }) {
  return (
    <div className="border border-border rounded-[5px] bg-surface-card p-3 flex items-center gap-3">
      <GenArtPlaceholder seed={s.gen_art_seed} size={96} />
      <div className="min-w-0 flex-1">
        <div className="font-mono text-[12.5px] text-text font-semibold truncate">
          {s.name || s.agent_id.slice(0, 10)}
        </div>
        <div className="flex items-center gap-1.5 mt-1.5 flex-wrap">
          <span className="font-mono text-[10px] px-1.5 py-0.5 border border-border rounded-[3px] text-text-2">
            #{s.token_id}
          </span>
          {s.listed && s.listing_id != null && (
            <Link
              to={`/marketplace/lineage/${s.listing_id}`}
              className="font-mono text-[10px] px-1.5 py-0.5 border border-gold/40 rounded-[3px] text-gold hover:bg-gold/10 transition-colors"
            >
              listed
            </Link>
          )}
        </div>
      </div>
    </div>
  );
}

function LicenseCard({ l }: { l: WalletLicense }) {
  return (
    <div className="border border-border rounded-[5px] bg-surface-card p-3 flex items-center gap-3">
      <GenArtPlaceholder seed={l.gen_art_seed} size={96} />
      <div className="min-w-0 flex-1">
        <div className="font-mono text-[12.5px] text-text font-semibold truncate">
          {l.name || l.agent_id.slice(0, 10)}
        </div>
        <div className="flex items-center gap-1.5 mt-1.5 flex-wrap">
          <span className="font-mono text-[10px] px-1.5 py-0.5 border border-border rounded-[3px] text-text-2">
            ×{l.balance}
          </span>
          <Link
            to={`/marketplace/lineage/${l.listing_id}`}
            className="font-mono text-[10px] px-1.5 py-0.5 border border-border rounded-[3px] text-text-3 hover:text-text-2 transition-colors"
          >
            view listing
          </Link>
        </div>
      </div>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export function WalletRoute() {
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

  const data = walletQuery.data;

  return (
    <div className="space-y-5" style={{ padding: "18px 28px 28px" }}>
      <h1 className="m-0 text-[20px] font-semibold tracking-[-0.02em]">
        Wallet
      </h1>

      <WalletStrip />

      {!address ? null : indexerOffline ? (
        <div className="border border-warn/40 bg-warn/5 rounded-[5px] px-4 py-3 font-mono text-[12px] text-warn">
          marketplace indexer offline — set XVN_RPC_URL / XVN_LISTING_REGISTRY
          / XVN_IDENTITY_REGISTRY on the server
        </div>
      ) : walletQuery.isError ? (
        <div className="border border-danger/40 bg-danger/5 rounded-[5px] px-4 py-3 font-mono text-[12px] text-danger">
          Failed to load wallet data: {walletQuery.error.message}
        </div>
      ) : walletQuery.isLoading || !data ? (
        <div className="px-1 py-4 font-mono text-[12px] text-text-3">
          Loading wallet…
        </div>
      ) : (
        <>
          {/* Strategies you own */}
          <div className="border border-border rounded-[5px] overflow-hidden">
            <SectionHeading
              title="Strategies you own"
              sub={`${data.strategies.length} agent NFT${data.strategies.length === 1 ? "" : "s"} held by this wallet`}
            />
            {data.strategies.length === 0 ? (
              <EmptyLine>No strategies owned by this wallet.</EmptyLine>
            ) : (
              <div className="p-3 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                {data.strategies.map((s) => (
                  <StrategyCard key={s.token_id} s={s} />
                ))}
              </div>
            )}
          </div>

          {/* Licenses you hold */}
          <div className="border border-border rounded-[5px] overflow-hidden">
            <SectionHeading
              title="Licenses you hold"
              sub="ERC-1155 license balances"
            />
            {data.licenses.length === 0 ? (
              <EmptyLine>No licenses held.</EmptyLine>
            ) : (
              <div className="p-3 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                {data.licenses.map((l) => (
                  <LicenseCard key={l.listing_id} l={l} />
                ))}
              </div>
            )}
          </div>

          {/* Your listings */}
          <div className="border border-border rounded-[5px] overflow-hidden">
            <SectionHeading
              title="Your listings"
              sub="listings published from this wallet"
            />
            {data.listings.length === 0 ? (
              <EmptyLine>No listings published from this wallet.</EmptyLine>
            ) : (
              <>
                {data.listings.map((l) => (
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
                <div className="px-4 py-2 border-t border-border">
                  <Link
                    to="/marketplace/mine"
                    className="font-mono text-[11px] text-text-3 hover:text-text underline underline-offset-2 transition-colors"
                  >
                    Manage all →
                  </Link>
                </div>
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}
