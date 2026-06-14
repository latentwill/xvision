// src/features/marketplace/routes/LineageRoute.tsx
//
// /marketplace/lineage/:name — the strategy inspector.
// App-native styling (matches the strategies / eval detail pages). Single
// full-width column. No popups, no right-side fourth column. The gen-art
// thumbnail inline-expands an "Artifact & provenance" accordion via
// ?inspect=art; the receipts drawer expands via ?receipts=open. Performance is
// a first-class full-width ChartFrame citizen with on-chain trade markers.
// Data: useQuery from @tanstack/react-query + useMarketplaceData() seam.
import { useState } from "react";
import { Link, useParams, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useMutation } from "@tanstack/react-query";
import { apiFetch } from "@/api/client";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import {
  isOnChainListingId,
  requirementsFromManifest,
  useBundleManifest,
} from "@/features/marketplace/data/bundle";
import { RequirementChip } from "@/features/marketplace/components/RequirementChip";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { faucetUsdc } from "@/features/marketplace/lib/chain";
import { InsufficientUsdcError } from "@/features/marketplace/lib/purchaseErrors";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { TxChip } from "@/features/marketplace/components/TxChip";
import { relativeTime } from "@/features/marketplace/lib/time";
import { humanize } from "./browse/ListingEntry";
import { IngredientBanner } from "./IngredientBanner";
import { PerformanceSection } from "./PerformanceSection";
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

function BuyerCard({
  humans,
  agents,
  paidToCreatorUsd,
  platformFeeBps,
  creator,
  isDemo,
}: {
  humans: number;
  agents: number;
  paidToCreatorUsd: number;
  platformFeeBps: number;
  creator: Creator;
  /** Fixture/demo client — adoption + earnings figures are illustrative. */
  isDemo: boolean;
}) {
  // Honest-data discipline (spec §0.5): adoption counts and the paid-to-creator
  // figure are fixture data on the demo client and unbacked (0) on the real
  // client today. Show them only when they are real (>0) or explicitly marked
  // DEMO — never a fabricated value masquerading as a real on-chain stat.
  const hasBuyers = humans + agents > 0;
  const hasPaid = paidToCreatorUsd > 0;
  if (!isDemo && !hasBuyers && !hasPaid) {
    // Real listing with no adoption record yet — dignified empty caption.
    return (
      <p className="font-mono text-[11px] text-text-3 mt-3" data-testid="buyers-empty">
        No buyers yet · be the first to acquire.
      </p>
    );
  }
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
      <div className="min-w-0">
        <p className="font-mono text-[11.5px] text-text flex items-center gap-1.5 flex-wrap">
          {humans} humans + {agents} agents
          {isDemo && (
            <span
              data-testid="buyers-demo-marker"
              className="font-mono text-[8.5px] tracking-[0.12em] uppercase bg-surface-elev text-text-3 border border-border rounded-[2px] px-1 py-0.5"
            >
              Demo
            </span>
          )}
        </p>
        {hasPaid && (
          <p className="font-mono text-[10px] text-text-3">
            ${paidToCreatorUsd.toLocaleString()} paid to {creator.handle ?? creator.address.slice(0, 8)} ·{" "}
            {platformFeeBps / 100}% platform fee
          </p>
        )}
      </div>
    </div>
  );
}

function WhatYouGetCards({ get, dont }: { get: string[]; dont: string[] }) {
  return (
    <div className="grid grid-cols-2 gap-4">
      <div className="rounded-md border border-border bg-surface-card p-4">
        <div className="text-[12px] font-medium text-text mb-0.5">What you get</div>
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
        <div className="text-[12px] font-medium text-text mb-0.5">What you don&apos;t get</div>
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
      <div className="text-[12px] font-medium text-text mb-3">Version history</div>
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
              <span className="font-mono text-[9.5px] text-text">{v.version}</span>
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
        <span className="text-[12px] font-medium text-text">Recent buyers</span>
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
        <span className="text-[12px] font-medium text-text">
          More from {creator.handle ?? creator.address.slice(0, 8)}
        </span>
        <button className="text-[11px] text-text-3 hover:text-text transition-colors">
          Profile
        </button>
      </div>
      <div>
        {rows.map((row, i) => (
          <button
            key={row.id}
            onClick={() => navigate(`/marketplace/lineage/${row.id}`)}
            className={[
              "w-full flex items-center gap-2.5 px-4 py-2.5 text-left hover:bg-surface-hover transition-colors",
              i < rows.length - 1 ? "border-b border-border-soft" : "",
            ].join(" ")}
          >
            <GenArtPlaceholder seed={row.genArtSeed} size={36} />
            <span className="font-mono text-[11px] text-text-2 flex-1">{row.name ?? row.id}</span>
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

// ── Strategy description (above the fold, inline "more" expand) ─────────────
// Renders detail.promise directly under the title/creator line. Clamped to 3
// lines with an inline "more" toggle when longer; no popup. When the promise is
// empty (sealed listings pre-purchase) the caller passes the honest fallback.
function DescriptionBlock({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  // Heuristic: anything past ~3 lines of body copy gets a "more" affordance.
  const isLong = text.length > 180;
  return (
    <p
      data-testid="strategy-description"
      className={[
        "text-[13.5px] text-text-2 leading-relaxed max-w-[640px]",
        !expanded && isLong ? "line-clamp-3" : "",
      ].join(" ")}
    >
      {text}
      {isLong && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="ml-1.5 align-baseline text-[12.5px] font-medium text-text-3 hover:text-text"
        >
          {expanded ? "less" : "more"}
        </button>
      )}
    </p>
  );
}

// ── Metric cell — one cell in the 5-up KPI strip ────────────────────────────
// Rebuilt so every value FITS (operator complaint): the old KpiCard hardcodes a
// 30px value + 100px min-height that clips inside a 5-up grid on narrow cards.
// Label and value sizing mirror the app's compact metric chrome; the value is
// tabular-nums + whitespace-nowrap so "+12.8%" and "—" never wrap or clip.
function MetricCell({
  label,
  value,
  intent = "default",
}: {
  label: string;
  value: string;
  intent?: "default" | "danger";
}) {
  return (
    <div className="border border-border rounded-card bg-surface-card p-3 min-w-0">
      <div className="text-[10px] uppercase tracking-[0.04em] text-text-3 whitespace-nowrap">
        {label}
      </div>
      <div
        className={[
          "mt-1 text-lg sm:text-xl font-semibold tabular-nums whitespace-nowrap",
          intent === "danger" ? "text-danger" : "text-text",
        ].join(" ")}
      >
        {value}
      </div>
    </div>
  );
}

// ── Eval attestations (on-chain, permissionless self-attestation) ───────────
// Wording: this section says "attested", never "verified" — v1 attestations
// are permissionless self-attestations (anyone, including the seller, can
// post one), so claiming "verified" would overstate the trust signal.
// Fetched directly from the attestations route rather than threading the data
// through `detail.onChain.attestations` — ApiMarketplaceData.getListing only
// hits the listing route and has no attestation fetch, so populating the
// OnChainReceipts shape there would mean a second fetch hidden inside the
// data seam. The section owns its own query instead.

/** Mirrors the backend `GET /api/marketplace/listings/:id/attestations` item. */
interface AttestationItem {
  attester: string;
  posted_at_unix: number;
  eval_result_uri: string;
  eval_result_hash: string;
  schema: string;
}

function truncMiddle(s: string): string {
  if (s.length <= 12) return s;
  return `${s.slice(0, 6)}…${s.slice(-4)}`;
}

function VerifiedEvalsSection({ listingId }: { listingId: string }) {
  const { data } = useQuery({
    queryKey: ["marketplace", "attestations", listingId],
    queryFn: () =>
      apiFetch<{ items: AttestationItem[] }>(
        `/api/marketplace/listings/${listingId}/attestations`,
      ),
    retry: false,
  });

  if (!data || data.items.length === 0) return null;

  return (
    <section
      data-testid="verified-evals"
      className="rounded-md border border-border bg-surface-card"
    >
      <div className="px-4 py-3 border-b border-border">
        <span className="text-[12px] font-medium text-text">
          Eval attestations
        </span>
        <span className="ml-2 font-mono text-[10px] text-text-3">
          on-chain eval attestations
        </span>
      </div>
      <div>
        {data.items.map((a, i) => (
          <div
            key={`${a.attester}-${a.posted_at_unix}-${i}`}
            className={[
              "flex items-center gap-3 px-4 py-2.5 flex-wrap",
              i < data.items.length - 1 ? "border-b border-border-soft" : "",
            ].join(" ")}
          >
            {/* v1: the registry carries no verdict field — every posted
                attestation renders as a plain "attested" chip (a
                self-attestation, not a third-party verification). */}
            <span className="font-mono text-[10px] px-1.5 py-0.5 border border-gold/40 rounded-[3px] text-gold">
              attested
            </span>
            <span className="font-mono text-[11.5px] text-text-2">
              {truncMiddle(a.attester)}
            </span>
            <span className="font-mono text-[10.5px] text-text-3 min-w-0 truncate">
              {a.eval_result_uri}
            </span>
            <span className="ml-auto font-mono text-[10.5px] text-text-3">
              {new Date(a.posted_at_unix * 1000).toLocaleDateString(undefined, {
                year: "numeric",
                month: "short",
                day: "numeric",
              })}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

// ────────────────────────────────────────────────────
// Main route component
// ────────────────────────────────────────────────────

export function LineageRoute() {
  const { name } = useParams<{ name: string }>();
  const mp = useMarketplaceData();
  const isDemo = mp.dataSource === "fixture";
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

  // Verified manifest enrichment — real (numeric) on-chain listings only.
  // Fixture listings never fetch; on any error this is null and the page
  // renders exactly as before.
  const manifest = useBundleManifest(detail?.id);
  const requirements = requirementsFromManifest(manifest);

  // Real purchase via the MarketplaceData seam: ApiMarketplaceData signs an
  // EIP-3009 TransferWithAuthorization and POSTs the gasless relay (falling
  // back to approve+buy when the relay 503s); the fixture client still
  // returns a fake TxRef for fixture slugs. Errors render inline below the
  // Acquire button (no popups); InsufficientUsdcError gets a faucet affordance.
  const buyMutation = useMutation({
    mutationFn: () => mp.purchaseIntent(detail!.id),
    onSuccess: (ref) => navigate(`/marketplace/receipts/${ref.txHash}`),
  });

  // Testnet affordance: mint the missing test USDC, then retry the purchase
  // (which re-runs the balance gate with the fresh balance).
  const faucetMutation = useMutation({
    mutationFn: (needed6: bigint) => faucetUsdc(needed6),
    onSuccess: () => buyMutation.mutate(),
  });

  // Free / clone path. Open-tier listings route through cloneIntent (QA12) —
  // a clone receipt, never a purchase.
  const cloneMutation = useMutation({
    mutationFn: () => mp.cloneIntent(detail!.id),
    onSuccess: (ref) => navigate(`/marketplace/receipts/${ref.txHash}`),
  });

  const isOpenTier = !!detail && detail.tier === "open";
  const canClone =
    !!detail &&
    (isOpenTier || (viewer?.ownedListingIds.includes(detail.id) ?? false));

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

  // Plate inspector accordion — deep-linkable via ?inspect=art.
  const inspectOpen = sp.get("inspect") === "art";
  const toggleInspect = () => {
    setSp(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (inspectOpen) next.delete("inspect");
        else next.set("inspect", "art");
        return next;
      },
      { replace: true },
    );
  };

  if (isLoading) {
    return <div className="px-6 py-8 text-[13px] text-text-3">Loading…</div>;
  }
  if (isError || !detail) {
    // App-native not-found — matches the other detail routes' 404 pattern.
    return (
      <div
        data-testid="lineage-not-found"
        className="px-6 py-12 text-center"
      >
        <div className="text-[24px] font-semibold text-text-3 mb-3">
          Strategy not found
        </div>
        <p className="m-0 mb-5 text-text-2 text-[13px]">
          No strategy with id <code className="font-mono text-text">{name}</code>.
        </p>
        <Link
          to="/marketplace"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          ← Back to marketplace
        </Link>
      </div>
    );
  }

  const provenanceTx =
    (
      detail.onChain.anchors.find((anchor) => anchor.kind === "mint" && anchor.tx)
        ?.tx ??
      detail.onChain.anchors.find((anchor) => anchor.tx)?.tx ??
      detail.onChain.tradesMeta.anchorTx
    ) ||
    undefined;

  // Title fallback chain: verified manifest display name → the listing's own
  // name → a humanized form of the id. Never the raw tech slug —
  // `humanize('btc-momentum-v3')` yields "Btc Momentum V3" (QA fix).
  const title = manifest?.display_name || detail.name || humanize(detail.id);

  // R3: surface the strategy description above the fold. The fixture carries
  // `promise`; R4 wires this to the real manifest `plain_summary` on the API
  // path. When empty (sealed listings pre-purchase) we show an honest line
  // rather than a blank gap.
  const description = detail.promise?.trim() ?? "";
  const platformFeePct = detail.platformFeeBps / 100;
  const netToCreator =
    detail.priceUsdc != null
      ? Math.round(detail.priceUsdc * (1 - detail.platformFeeBps / 10_000) * 100) / 100
      : null;

  return (
    <div data-testid="lineage-page">
      {/* ===== BACK LINK + PROVENANCE EYEBROW ===== */}
      <div className="px-6 pt-6">
        <Link
          to="/marketplace"
          data-testid="lineage-back"
          className="inline-flex items-center gap-1.5 text-[12px] text-text-2 hover:text-text mb-3"
        >
          ← Back to marketplace
        </Link>
        <div className="font-mono text-[11px] tracking-[0.14em] uppercase text-text-3">
          Marketplace · {detail.onChain.nft.network.toUpperCase()}
        </div>
      </div>

      {/* ===== HERO (two zones: thumbnail + info/price) ===== */}
      <section
        data-testid="lineage-hero"
        className="grid gap-6 p-6 border-b border-border"
        style={{ gridTemplateColumns: "360px 1fr" }}
      >
        {/* Zone A: gen-art thumbnail — clickable, inline-expands the inspector */}
        <div className="flex flex-col gap-2">
          <button
            type="button"
            data-testid="plate-inspect-toggle"
            onClick={toggleInspect}
            aria-pressed={inspectOpen}
            className="block rounded-card border border-border hover:border-border-strong transition-colors overflow-hidden text-left"
          >
            <GenArtPlaceholder
              seed={detail.genArtSeed}
              size={340}
              className="block"
            />
          </button>
          <div className="flex items-center justify-end">
            <span className="font-mono text-[10px] tracking-[0.1em] text-text-3 uppercase inline-flex items-center gap-1.5">
              {inspectOpen ? "Hide artifact" : "Artifact & provenance"}
              <svg
                width="9"
                height="9"
                viewBox="0 0 10 10"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
                aria-hidden="true"
                className={[
                  "transition-transform",
                  inspectOpen ? "rotate-180" : "",
                ].join(" ")}
              >
                <path d="M2.5 3.5L5 6l2.5-2.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </span>
          </div>
        </div>

        {/* Zone B: title, description, metrics, purchase block */}
        <div data-testid="lineage-info-stack" className="flex flex-col gap-3 min-w-0">
          {/* Title row */}
          <div className="flex items-center flex-wrap gap-2">
            <h1
              className="text-[30px] font-medium tracking-tight leading-tight text-text"
              title={title}
            >
              {title}
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

          {/* Creator line — real listings link the seller address to the
              creator page; fixture listings keep the plain text. */}
          <div className="font-mono text-[11.5px] text-text-3 flex items-center gap-1.5">
            <span>{detail.creator.handle ?? detail.creator.address.slice(0, 10)}</span>
            <span>·</span>
            {isOnChainListingId(detail.id) ? (
              <Link
                data-testid="creator-link"
                to={`/marketplace/creator/${detail.creator.address}`}
                className="text-info hover:underline"
              >
                {detail.creator.address.slice(0, 6)}…{detail.creator.address.slice(-4)}
              </Link>
            ) : (
              <span>{detail.creator.address.slice(0, 6)}…{detail.creator.address.slice(-4)}</span>
            )}
            {detail.model && (
              <>
                <span>·</span>
                <span>{detail.model}</span>
              </>
            )}
          </div>

          {/* Description — above the fold, directly under the creator line. */}
          {description ? (
            <DescriptionBlock text={description} />
          ) : (
            <p
              data-testid="strategy-description-sealed"
              className="text-[13.5px] text-text-3 leading-relaxed max-w-[640px]"
            >
              Sealed strategy — contents verified on-chain, revealed after
              purchase.
            </p>
          )}

          {/* Metric strip — every value FITS: tabular-nums + whitespace-nowrap,
              "—" for absent values (never 0).
              Provenance: these figures come from XVN backtest/eval (indicative).
              Live Degen Arena (on-chain) PnL is shown in the PerformanceSection
              provenance banner below when available. */}
          <div className="flex items-center gap-2 pt-1 pb-0.5">
            <span
              data-testid="metric-strip-provenance"
              className="font-mono text-[9px] tracking-[0.12em] uppercase text-text-3 whitespace-nowrap"
            >
              Indicative · XVN backtest
            </span>
          </div>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(104px,1fr))] gap-2">
            <MetricCell
              label="30D Return"
              value={
                detail.metrics.return30dPct === 0
                  ? "—"
                  : `${detail.metrics.return30dPct > 0 ? "+" : ""}${detail.metrics.return30dPct}%`
              }
            />
            <MetricCell
              label="Sharpe"
              value={detail.metrics.sharpe === 0 ? "—" : String(detail.metrics.sharpe)}
            />
            <MetricCell
              label="Win rate"
              value={detail.metrics.winRatePct === 0 ? "—" : `${detail.metrics.winRatePct}%`}
            />
            <MetricCell
              label="Max DD"
              value={detail.metrics.maxDrawdownPct === 0 ? "—" : `${detail.metrics.maxDrawdownPct}%`}
              intent="danger"
            />
            <MetricCell
              label="Avg dur"
              value={detail.metrics.avgDurationDays === 0 ? "—" : `${detail.metrics.avgDurationDays}d`}
            />
          </div>

          {/* Buyer card */}
          <BuyerCard
            humans={detail.buyers.humans}
            agents={detail.buyers.agents}
            paidToCreatorUsd={detail.paidToCreatorUsd}
            platformFeeBps={detail.platformFeeBps}
            creator={detail.creator}
            isDemo={isDemo}
          />

          {/* Purchase block — folded inline into Zone B's right edge (no third column) */}
          <div
            data-testid="lineage-purchase-col"
            className="mt-1 rounded-md border border-gold-soft bg-gradient-to-b from-gold/[0.06] to-gold/[0.02] p-4 max-w-[420px]"
          >
            <div className="font-mono text-[9px] tracking-[0.2em] text-text-3 uppercase mb-1">
              Price
            </div>
            <div className="flex items-baseline gap-2">
              <span className="font-mono text-[24px] font-semibold text-text leading-none tabular-nums">
                {detail.priceUsdc === null ? "OPEN EDITION" : `${detail.priceUsdc} USDC`}
              </span>
            </div>
            {/* Fee on a SEPARATE muted line — never parenthesized into the price (QA15). */}
            {detail.priceUsdc !== null && netToCreator !== null && (
              <div
                data-testid="fee-line"
                className="font-mono text-[10.5px] text-text-3 mt-1.5"
              >
                Platform fee {platformFeePct}% · creator receives {netToCreator} USDC
              </div>
            )}
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              perpetual license · one-time
            </div>

            {/* Primary CTA. Open tier = Run free (cloneIntent); paid = Acquire
                (purchaseIntent) gated on wallet connection. */}
            {isOpenTier ? (
              <button
                data-testid="run-free-btn"
                onClick={() => cloneMutation.mutate()}
                disabled={cloneMutation.isPending}
                className="mt-3 w-full py-2.5 rounded-[3px] bg-gold text-[#001A0A] text-[13.5px] font-bold tracking-[0.01em] disabled:opacity-60 hover:opacity-90 transition-opacity motion-safe:active:scale-[0.96]"
              >
                {cloneMutation.isPending ? "Activating…" : "Run free"}
              </button>
            ) : (
              <button
                data-testid="buy-btn"
                onClick={() =>
                  walletAddress
                    ? buyMutation.mutate()
                    : navigate("/settings/wallet")
                }
                disabled={buyMutation.isPending}
                className="mt-3 w-full py-2.5 rounded-[3px] bg-gold text-[#001A0A] text-[13.5px] font-bold tracking-[0.01em] disabled:opacity-60 hover:opacity-90 transition-opacity motion-safe:active:scale-[0.96]"
              >
                {buyMutation.isPending
                  ? "Acquiring…"
                  : walletAddress
                    ? "Acquire"
                    : "Connect wallet to acquire"}
              </button>
            )}

            {/* Inline purchase error (no popups). Faucet affordance when the
                failure is an insufficient test-USDC balance. */}
            {buyMutation.isError && !buyMutation.isPending && (
              <div
                data-testid="buy-error"
                className="mt-2 rounded border border-danger/40 bg-danger/5 px-2.5 py-2"
              >
                <p className="font-mono text-[10.5px] text-danger leading-snug">
                  {buyMutation.error instanceof Error
                    ? buyMutation.error.message
                    : "Purchase failed."}
                </p>
                {buyMutation.error instanceof InsufficientUsdcError && (
                  <button
                    data-testid="faucet-btn"
                    onClick={() =>
                      faucetMutation.mutate(
                        (buyMutation.error as InsufficientUsdcError).neededUsdc6,
                      )
                    }
                    disabled={faucetMutation.isPending}
                    className="mt-1.5 px-2 py-1 rounded border border-gold/60 bg-gold/10 font-mono text-[10.5px] text-gold hover:bg-gold/20 transition-colors disabled:opacity-60"
                  >
                    {faucetMutation.isPending
                      ? "Minting test USDC…"
                      : "Get test USDC"}
                  </button>
                )}
                {faucetMutation.isError && (
                  <p className="mt-1 font-mono text-[10px] text-danger leading-snug">
                    Faucet failed:{" "}
                    {faucetMutation.error instanceof Error
                      ? faucetMutation.error.message
                      : "unknown error"}
                  </p>
                )}
              </div>
            )}

            <div className="mt-2 font-mono text-[10px] text-text-3 leading-snug">
              Mantle Sepolia testnet — pays with test USDC.
            </div>

            {/* Clone to edit — the editor handoff (kept). The Share button is removed (QA3). */}
            <button
              onClick={() => cloneMutation.mutate()}
              disabled={!canClone || cloneMutation.isPending}
              className="mt-2 w-full py-2 rounded border border-border text-[12px] font-medium text-text-2 hover:text-text hover:border-border-strong transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Clone to edit
            </button>
          </div>
        </div>
      </section>

      {/* ===== ARTIFACT & PROVENANCE INSPECTOR (inline accordion, ?inspect=art) ===== */}
      {inspectOpen && (
        <section
          data-testid="inspect-art"
          className="mx-6 mt-6 rounded-md border border-border bg-surface-card p-4"
        >
          <div className="text-[12px] font-medium text-text mb-3">Artifact &amp; provenance</div>
          <div className="space-y-0">
            {(
              [
                ["token_id", detail.onChain.nft.tokenId],
                ["lineage_id", detail.onChain.nft.lineageId],
                ["manifest_hash", detail.onChain.nft.manifestHash],
                ["contract", detail.onChain.nft.contract],
                ["born_at", detail.onChain.nft.bornAt],
              ] as [string, string][]
            ).map(([key, val], i, arr) => (
              <div
                key={key}
                className={[
                  "grid gap-2.5 py-1.5",
                  i < arr.length - 1 ? "border-b border-border-soft" : "",
                ].join(" ")}
                style={{ gridTemplateColumns: "130px 1fr" }}
              >
                <span className="font-mono text-[9.5px] tracking-[0.14em] text-text-3 uppercase">
                  {key}
                </span>
                <span className="font-mono text-[11px] break-all text-text">{val}</span>
              </div>
            ))}
          </div>
          {provenanceTx && (
            <div className="mt-3">
              <TxChip
                hash={provenanceTx}
                network={detail.onChain.nft.network}
                label="View on explorer"
              />
            </div>
          )}
        </section>
      )}

      {/* ===== MANIFEST ENRICHMENT (real listings; renders nothing when the
            bundle route 404s/errors or fields are absent) ===== */}
      {manifest?.plain_summary && (
        <section
          data-testid="about-strategy"
          className="mx-6 mt-6 rounded-md border border-border bg-surface-card p-4"
        >
          <div className="text-[12px] font-medium text-text mb-1.5">
            About this strategy
          </div>
          <p className="text-[13px] leading-[1.5] text-text-2 whitespace-pre-wrap max-w-[640px]">
            {manifest.plain_summary}
          </p>
        </section>
      )}
      {requirements.length > 0 && (
        <section data-testid="requirements-row" className="mx-6 mt-6">
          <div className="font-mono text-[9px] tracking-[0.2em] uppercase text-text-3 mb-2">
            Requirements
          </div>
          <div className="flex flex-wrap gap-1.5">
            {requirements.map((r) => (
              <RequirementChip key={`${r.kind}:${r.name}`} requirement={r} />
            ))}
          </div>
          <p className="mt-1.5 font-mono text-[10.5px] text-text-3">
            you&apos;ll need these to run the strategy after purchase
          </p>
        </section>
      )}

      {/* ===== INGREDIENT BANNER ===== */}
      <IngredientBanner ingredients={detail.ingredients} />

      {/* ===== BELOW THE FOLD — single full-width column ===== */}
      <div className="p-6 space-y-6">
        {/* PERFORMANCE — first-class citizen, full-width, on-chain markers */}
        <PerformanceSection
          curve={detail.equityCurve}
          trades={detail.onChain.trades}
          // Live Degen Arena PnL is authoritative once there's real on-chain
          // trading; hidden (null) until the indexer reports trades, so an
          // un-traded listing doesn't show a misleading $0.00 live figure.
          liveDegenPnlUsd={
            detail.onChain.tradesMeta.totalOnChain > 0
              ? detail.onChain.tradesMeta.netPnlUsd
              : null
          }
        />

        {/* EVAL ATTESTATIONS (inline, only for attested on-chain listings) */}
        {/^\d+$/.test(detail.id) && detail.verification === "verified" && (
          <VerifiedEvalsSection listingId={detail.id} />
        )}

        <WhatYouGetCards get={detail.whatYouGet} dont={detail.whatYouDont} />
        <VariantMiniTree variants={detail.variants} clonesOfYours={detail.clonesOfYours} />

        {/* RECENT BUYERS + MORE FROM CREATOR — inline grid-cols-2, full-width
            (NOT a 380px right sidebar). */}
        <div className="grid gap-6" style={{ gridTemplateColumns: "1fr 1fr" }}>
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
