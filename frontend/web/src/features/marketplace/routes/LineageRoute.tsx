// src/features/marketplace/routes/LineageRoute.tsx
//
// /marketplace/lineage/:name — the viral identity page.
// No popups. Receipts drawer inline-expands via ?receipts=open.
// Data: useQuery from @tanstack/react-query + useMarketplaceData() seam.
// Per addendum §1: queryKey ["marketplace", "listing", name] / ["marketplace", "viewer"]
import { useParams, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useMutation } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { relativeTime } from "@/features/marketplace/lib/time";
import { IngredientBanner } from "./IngredientBanner";
import { EquityPanel } from "./EquityPanel";
import { ReceiptsDrawer } from "./ReceiptsDrawer";
import type {
  Creator,
  ListingRow,
  RecentBuyer,
  Variant,
} from "@/features/marketplace/data/types";

// ────────────────────────────────────────────────────
// Inline sub-components (simple enough to colocate)
// ────────────────────────────────────────────────────

function MetricCell({
  label,
  value,
  tone = "default",
}: {
  label: string;
  value: string | number;
  tone?: "default" | "warn";
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-mono text-[9px] tracking-[0.2em] text-text-3 uppercase">{label}</span>
      <span
        className={[
          "font-mono text-[18px] font-semibold leading-none",
          tone === "warn" ? "text-warn" : "text-foreground",
        ].join(" ")}
      >
        {value}
      </span>
    </div>
  );
}

function BuyerCard({
  humans,
  agents,
  paidToCreatorUsd,
  platformFeeBps,
  creator,
}: {
  humans: number;
  agents: number;
  paidToCreatorUsd: number;
  platformFeeBps: number;
  creator: Creator;
}) {
  return (
    <div className="flex items-center gap-2.5 p-3 rounded-md border border-border bg-surface-elev mt-3">
      {/* Avatar stack: 5 colored circles + AgentIcon circle */}
      <div className="flex -space-x-1.5">
        {[150, 210, 265, 45, 330].map((hue, i) => (
          <span
            key={i}
            className="w-6 h-6 rounded-full border border-bg flex-shrink-0"
            style={{ background: `hsl(${hue} 60% 35%)` }}
          />
        ))}
        <span className="w-6 h-6 rounded-full border border-bg flex-shrink-0 flex items-center justify-center bg-surface-elev border-gold-soft">
          <AgentIcon size={10} className="text-gold" />
        </span>
      </div>
      <div>
        <p className="font-mono text-[11.5px] text-foreground">
          {humans} humans + {agents} agents
        </p>
        <p className="font-mono text-[10px] text-text-3">
          ${paidToCreatorUsd.toLocaleString()} paid to {creator.handle ?? creator.address.slice(0, 8)} ·{" "}
          {platformFeeBps / 100}% platform fee
        </p>
      </div>
    </div>
  );
}

function WhatYouGetCards({ get, dont }: { get: string[]; dont: string[] }) {
  return (
    <div className="grid grid-cols-2 gap-4">
      <div className="rounded-md border border-border bg-surface-card p-4">
        <div className="text-[12px] font-medium text-foreground mb-0.5">What you get</div>
        <div className="font-mono text-[10.5px] text-text-3 mb-2">
          Tier 2 sealed bundle · decrypts after purchase
        </div>
        <ul className="flex flex-col gap-1">
          {get.map((item) => (
            <li key={item} className="flex items-center gap-1.5 text-[12.5px] text-text-2">
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" className="text-gold flex-shrink-0">
                <path
                  d="M2 5l2 2 4-4"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              {item}
            </li>
          ))}
        </ul>
      </div>
      <div className="rounded-md border border-border bg-surface-card p-4">
        <div className="text-[12px] font-medium text-foreground mb-0.5">What you don&apos;t get</div>
        <div className="font-mono text-[10.5px] text-text-3 mb-2">Tier 3 — never bundled</div>
        <ul className="flex flex-col gap-1">
          {dont.map((item) => (
            <li key={item} className="flex items-center gap-1.5 text-[12.5px] text-text-3">
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" className="text-text-3 flex-shrink-0">
                <path d="M3 3l4 4M7 3l-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
              {item}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function VariantMiniTree({
  variants,
  clonesOfYours,
}: {
  variants: Variant[];
  clonesOfYours?: { count: number; upstreamEarningsUsd: number };
}) {
  return (
    <div className="rounded-md border border-border bg-surface-card p-4">
      <div className="text-[12px] font-medium text-foreground mb-3">Version history</div>
      <div className="flex items-center gap-0">
        {variants.map((v, i) => (
          <div key={v.version} className="flex items-center">
            <div
              className={[
                "flex flex-col items-center gap-1.5 p-2 rounded-md border",
                v.current ? "border-2 border-gold" : "border-border",
              ].join(" ")}
            >
              <GenArtPlaceholder seed={v.genArtSeed} size={56} />
              <span className="font-mono text-[9.5px] text-foreground">{v.version}</span>
              <span className="font-mono text-[9px] text-text-3">{v.sharpe} sharpe</span>
            </div>
            {/* Connector between variants */}
            {i < variants.length - 1 && (
              <div className="flex items-center mx-1">
                <div className="w-6 h-px bg-border" />
                <div className="w-1.5 h-1.5 rounded-full bg-border-strong" />
              </div>
            )}
          </div>
        ))}

        {/* Clones-of-yours teaser */}
        {clonesOfYours && (
          <div className="flex items-center ml-3">
            <div className="w-6 h-px bg-border" />
            <div className="ml-2 flex flex-col gap-0.5">
              <span className="font-mono text-[8px] tracking-[0.18em] text-text-3 uppercase">
                Clones of yours
              </span>
              <span className="font-mono text-[22px] font-semibold text-gold leading-none">
                {clonesOfYours.count}
              </span>
              <span className="font-mono text-[9px] text-text-3">
                upstream of ${clonesOfYours.upstreamEarningsUsd.toLocaleString()}
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function RecentBuyersList({ buyers }: { buyers: RecentBuyer[] }) {
  return (
    <div className="rounded-md border border-border bg-surface-card">
      <div className="px-4 py-3 border-b border-border">
        <span className="text-[12px] font-medium text-foreground">Recent buyers</span>
      </div>
      <div>
        {buyers.map((b, i) => {
          const isAgent = b.payerKind === "agent";
          const outcomeColor = b.outcome.startsWith("+")
            ? "text-gold"
            : b.outcome.startsWith("-")
              ? "text-danger"
              : "text-info";
          return (
            <div
              key={i}
              className={[
                "flex items-center gap-2.5 px-4 py-2.5",
                i < buyers.length - 1 ? "border-b border-border-soft" : "",
              ].join(" ")}
            >
              <span
                className={[
                  "w-6 h-6 flex items-center justify-center border flex-shrink-0",
                  isAgent
                    ? "rounded-[3px] border-gold-soft bg-gold/[0.10]"
                    : "rounded-full border-border-strong bg-surface-elev",
                ].join(" ")}
              >
                {isAgent && <AgentIcon size={10} className="text-gold" />}
              </span>
              <span className="font-mono text-[11.5px] text-text-2 flex-1">{b.label}</span>
              <span className={`font-mono text-[11.5px] font-medium ${outcomeColor}`}>
                {b.outcome}
              </span>
              <span className="font-mono text-[10.5px] text-text-3 min-w-[60px] text-right">
                {relativeTime(b.at)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function MoreFromCreatorCard({
  rows,
  creator,
}: {
  rows: ListingRow[];
  creator: Creator;
}) {
  const navigate = useNavigate();
  return (
    <div className="rounded-md border border-border bg-surface-card">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="text-[12px] font-medium text-foreground">
          More from {creator.handle ?? creator.address.slice(0, 8)}
        </span>
        <button className="text-[11px] text-text-3 hover:text-foreground transition-colors">
          Profile
        </button>
      </div>
      <div>
        {rows.map((row, i) => (
          <button
            key={row.id}
            onClick={() => navigate(`/marketplace/lineage/${row.id}`)}
            className={[
              "w-full flex items-center gap-2.5 px-4 py-2.5 text-left hover:bg-white/[0.02] transition-colors",
              i < rows.length - 1 ? "border-b border-border-soft" : "",
            ].join(" ")}
          >
            <GenArtPlaceholder seed={row.genArtSeed} size={36} />
            <span className="font-mono text-[11px] text-text-2 flex-1">{row.id}</span>
            <span className="font-mono text-[10.5px] text-text-3">
              {row.buyers.humans + row.buyers.agents} acqd
            </span>
            <span className="font-mono text-[12px] font-semibold text-gold ml-2">
              {row.return30dPct > 0 ? "+" : ""}
              {row.return30dPct}%
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

// ────────────────────────────────────────────────────
// Main route component
// ────────────────────────────────────────────────────

export function LineageRoute() {
  const { name } = useParams<{ name: string }>();
  const mp = useMarketplaceData();
  const navigate = useNavigate();
  const [sp, setSp] = useSearchParams();
  const { address: walletAddress } = useWallet();

  const {
    data: detail,
    isLoading,
    isError,
  } = useQuery({
    queryKey: ["marketplace", "listing", name],
    queryFn: () => mp.getListing(name!),
    enabled: !!name,
  });

  const { data: viewer } = useQuery({
    queryKey: ["marketplace", "viewer"],
    queryFn: () => mp.getViewer(),
  });

  const buyMutation = useMutation({
    mutationFn: () => mp.purchaseIntent(detail!.id),
    onSuccess: (ref) => navigate(`/marketplace/receipts/${ref.txHash}`),
  });

  const cloneMutation = useMutation({
    mutationFn: () => mp.cloneIntent(detail!.id),
    onSuccess: (ref) => navigate(`/marketplace/receipts/${ref.txHash}`),
  });

  const canClone =
    !!detail &&
    (detail.tier === "open" || (viewer?.ownedListingIds.includes(detail.id) ?? false));

  const receiptsOpen = sp.get("receipts") === "open";
  const toggleReceipts = () => {
    setSp(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (receiptsOpen) next.delete("receipts");
        else next.set("receipts", "open");
        return next;
      },
      { replace: true },
    );
  };

  if (isLoading) {
    return <div className="px-6 py-8 text-[13px] text-text-3">Loading…</div>;
  }
  if (isError || !detail) {
    return (
      <div className="px-6 py-8 text-[13px] text-danger">Strategy not found.</div>
    );
  }

  return (
    <div data-testid="lineage-page">
      {/* ===== HERO (above-the-fold) ===== */}
      <section
        data-testid="lineage-hero"
        className="grid gap-6 p-6 border-b border-border"
        style={{ gridTemplateColumns: "320px 1fr 250px" }}
      >
        {/* Col 1: gen-art + NFT stamp */}
        <div className="relative">
          <GenArtPlaceholder
            seed={detail.genArtSeed}
            size={320}
            className="rounded-lg border border-border"
          />
          <span className="absolute bottom-2 left-2 px-2 py-0.5 rounded bg-black/70 backdrop-blur-sm font-mono text-[10px] tracking-[0.14em] text-foreground uppercase">
            NFT {detail.onChain.nft.tokenId}
          </span>
        </div>

        {/* Col 2: title + metrics + buyer card */}
        <div data-testid="lineage-info-stack" className="flex flex-col gap-3 min-w-0">
          {/* Title row */}
          <div className="flex items-center flex-wrap gap-2">
            <h1 className="font-mono text-[30px] font-semibold tracking-tight leading-none">
              {detail.id}
            </h1>
            <span className="font-mono text-[11px] text-text-3 px-1.5 py-0.5 rounded border border-border-strong">
              {detail.version}
            </span>
            {detail.verification === "verified" && (
              <VerifiedBadge data-testid="verified-badge" />
            )}
            {detail.acceptsX402 && <X402Badge data-testid="x402-badge" />}
            {detail.assets.map((a) => (
              <AssetPill key={a} asset={a} />
            ))}
          </div>

          {/* Creator line */}
          <div className="font-mono text-[11.5px] text-text-3 flex items-center gap-1.5">
            <span>{detail.creator.handle ?? detail.creator.address.slice(0, 10)}</span>
            <span>·</span>
            <span>{detail.creator.address.slice(0, 6)}…{detail.creator.address.slice(-4)}</span>
            <span>·</span>
            <span>{detail.model}</span>
          </div>

          {/* Promise */}
          <p className="text-[14.5px] leading-[1.45] max-w-[480px]">{detail.promise}</p>

          {/* Metrics grid */}
          <div
            className="grid gap-[18px] items-end pt-1.5"
            style={{ gridTemplateColumns: "auto 1fr 1fr 1fr 1fr" }}
          >
            {/* 30D RETURN — big gold number */}
            <div className="flex flex-col gap-0.5">
              <span className="font-mono text-[9px] tracking-[0.2em] text-text-3 uppercase">
                30D Return
              </span>
              <span className="font-mono text-[42px] font-semibold text-gold leading-none">
                {detail.metrics.return30dPct > 0 ? "+" : ""}
                {detail.metrics.return30dPct}%
              </span>
            </div>
            <MetricCell label="Sharpe" value={detail.metrics.sharpe} />
            <MetricCell
              label="Win rate"
              value={`${detail.metrics.winRatePct}%`}
            />
            <MetricCell
              label="Max DD"
              value={`${detail.metrics.maxDrawdownPct}%`}
              tone="warn"
            />
            <MetricCell
              label="Avg dur"
              value={`${detail.metrics.avgDurationDays}d`}
            />
          </div>

          {/* Buyer card */}
          <BuyerCard
            humans={detail.buyers.humans}
            agents={detail.buyers.agents}
            paidToCreatorUsd={detail.paidToCreatorUsd}
            platformFeeBps={detail.platformFeeBps}
            creator={detail.creator}
          />
        </div>

        {/* Col 3: purchase column */}
        <div data-testid="lineage-purchase-col" className="flex flex-col gap-3">
          {/* Price card with gold-tinted bg */}
          <div className="rounded-md border border-gold-soft bg-gradient-to-b from-gold/[0.06] to-gold/[0.02] p-4">
            <div className="font-mono text-[9px] tracking-[0.2em] text-text-3 uppercase mb-1">
              Price
            </div>
            <div className="flex items-baseline gap-2">
              <span className="font-mono text-[24px] font-semibold text-foreground leading-none">
                {detail.priceUsdc === null ? "FREE" : `${detail.priceUsdc} USDC`}
              </span>
              <span className="font-mono text-[9px] tracking-[0.14em] text-warn uppercase px-1.5 py-0.5 rounded border border-warn/30 bg-warn/5">
                TESTNET
              </span>
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              perpetual license · one-time
            </div>

            {/* Buy button — gated on wallet connection */}
            <button
              data-testid="buy-btn"
              onClick={() =>
                walletAddress
                  ? buyMutation.mutate()
                  : navigate("/settings/wallet")
              }
              disabled={buyMutation.isPending}
              className="mt-3 w-full py-2.5 rounded bg-gold text-[#001A0A] text-[13.5px] font-bold tracking-[0.01em] disabled:opacity-60 hover:opacity-90 transition-opacity active:scale-[0.96]"
            >
              {buyMutation.isPending
                ? "Buying…"
                : walletAddress
                  ? "Buy"
                  : "Connect wallet to buy"}
            </button>
          </div>

          {/* Clone / Share row */}
          <div className="flex gap-2">
            <button
              onClick={() => cloneMutation.mutate()}
              disabled={!canClone || cloneMutation.isPending}
              className="flex-1 py-2 rounded border border-border text-[12px] font-medium text-text-2 hover:text-foreground hover:border-border-strong transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Clone to edit
            </button>
            {/* TODO(F7-share): Share composer route is F7 */}
            <button
              disabled
              className="flex-1 py-2 rounded border border-border text-[12px] font-medium text-text-3 disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Share
            </button>
          </div>
        </div>
      </section>

      {/* ===== INGREDIENT BANNER ===== */}
      <IngredientBanner ingredients={detail.ingredients} />

      {/* ===== BELOW THE FOLD (2-col) ===== */}
      <div className="grid gap-6 p-6" style={{ gridTemplateColumns: "1fr 380px" }}>
        {/* LEFT */}
        <div className="flex flex-col gap-5">
          <EquityPanel curve={detail.equityCurve} />
          <WhatYouGetCards get={detail.whatYouGet} dont={detail.whatYouDont} />
          <VariantMiniTree variants={detail.variants} clonesOfYours={detail.clonesOfYours} />
        </div>
        {/* RIGHT */}
        <div className="flex flex-col gap-5">
          <RecentBuyersList buyers={detail.recentBuyers} />
          <MoreFromCreatorCard rows={detail.creatorOther} creator={detail.creator} />
        </div>
      </div>

      {/* ===== RECEIPTS DRAWER (inline expand, NO modal/sheet/popover) ===== */}
      <ReceiptsDrawer
        onChain={detail.onChain}
        open={receiptsOpen}
        onToggle={toggleReceipts}
      />
    </div>
  );
}
