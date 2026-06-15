// src/features/marketplace/routes/CreatorRoute.tsx
// F3 — /marketplace/creator/:handleOrAddr
// Tasks fill this file: EarningsChart (T1), LineageForest (T2),
// helper components (T3), CreatorRoute page (T4).

import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { apiFetch } from "@/api/client";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { IndexedListing } from "@/features/marketplace/data/ApiMarketplaceData";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { relativeTime } from "@/features/marketplace/lib/time";
import { formatUsd } from "@/lib/format";
import type {
  AttestationActivity,
  CloneByEntry,
  ForestEdge,
  ForestNode,
  ListingRow,
  Verdict,
} from "@/features/marketplace/data/types";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function relativeDate(iso: string): string {
  const diffMs = Date.now() - new Date(iso).getTime();
  const days = Math.floor(diffMs / 86400000);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.floor(months / 12)}y ago`;
}

function truncAddr(addr: string): string {
  if (addr.length <= 10) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

// ─── EarningsChart ───────────────────────────────────────────────────────────

export function EarningsChart({
  data,
  width = 320,
  height = 110,
}: {
  data: number[];
  width?: number;
  height?: number;
}) {
  if (data.length < 2) return null;
  const padT = 4, padB = 4, padL = 0, padR = 0;
  const innerW = width - padL - padR;
  const innerH = height - padT - padB;
  const max = Math.max(...data) || 1;
  const xs = data.map((_, i) => padL + (i / (data.length - 1)) * innerW);
  const ys = data.map((v) => padT + innerH - (v / max) * innerH);
  const linePts = xs
    .map((x, i) => `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[i].toFixed(1)}`)
    .join(" ");
  const dFill =
    linePts +
    ` L ${xs[xs.length - 1].toFixed(1)} ${(padT + innerH).toFixed(1)}` +
    ` L ${xs[0].toFixed(1)} ${(padT + innerH).toFixed(1)} Z`;
  const gradId = `earn-fill-${data.length}`;
  return (
    <svg
      width="100%"
      viewBox={`0 0 ${width} ${height}`}
      aria-hidden="true"
      className="block"
    >
      <defs>
        <linearGradient id={gradId} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="#00E676" stopOpacity="0.30" />
          <stop offset="100%" stopColor="#00E676" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={dFill} fill={`url(#${gradId})`} />
      <path
        d={linePts}
        fill="none"
        stroke="var(--gold)"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
    </svg>
  );
}

// ─── LineageForest ────────────────────────────────────────────────────────────

export function LineageForest({
  nodes,
  edges,
}: {
  nodes: ForestNode[];
  edges: ForestEdge[];
}) {
  const navigate = useNavigate();
  const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));
  const offsetX = 100, offsetY = 30;

  // Derive row labels: unique strategy names (excluding clone markers) + y of first node
  const rowLabels: { label: string; y: number }[] = [];
  const seen = new Set<string>();
  for (const n of nodes) {
    if (n.strategy === "clone-by" || n.strategy === "clone-from") continue;
    if (n.external) continue;
    if (!seen.has(n.strategy)) {
      seen.add(n.strategy);
      rowLabels.push({ label: n.strategy.toUpperCase(), y: n.y });
    }
  }

  return (
    <div className="relative overflow-x-auto" style={{ padding: "18px 18px 22px" }}>
      <svg
        width="100%"
        height={300}
        viewBox="-100 0 580 320"
        aria-label="Lineage forest"
        className="block"
      >
        {/* Row labels */}
        {rowLabels.map(({ label, y }) => (
          <text
            key={label}
            x="-50"
            y={y + offsetY + 4}
            fontFamily="'Geist Mono', monospace"
            fontSize="9"
            fill="var(--text-3, #5F6670)"
            letterSpacing="0.18em"
          >
            {label}
          </text>
        ))}

        {/* Edges */}
        {edges.map((e, i) => {
          const na = byId[e.from];
          const nb = byId[e.to];
          if (!na || !nb) return null;
          const isClone = e.kind === "clone";
          const x1 = na.x + offsetX, y1 = na.y + offsetY;
          const x2 = nb.x + offsetX, y2 = nb.y + offsetY;
          const dx = (x2 - x1) * 0.5;
          const path =
            y1 === y2
              ? `M ${x1 + 22} ${y1} L ${x2 - 22} ${y2}`
              : `M ${x1 + 22} ${y1} C ${x1 + dx + 22} ${y1}, ${x2 - dx - 22} ${y2}, ${x2 - 22} ${y2}`;
          return (
            <path
              key={i}
              d={path}
              fill="none"
              stroke={isClone ? "var(--info, #5FA8FF)" : "var(--border-strong, #2A2A2A)"}
              strokeWidth={isClone ? 1.2 : 1.4}
              strokeDasharray={isClone ? "3 3" : undefined}
              opacity={isClone ? 0.7 : 0.9}
            />
          );
        })}
      </svg>

      {/* Node tiles — absolutely positioned over SVG */}
      {nodes.map((n) => {
        const isHead = !!n.current;
        const isExternal = !!n.external;
        const leftPct = ((n.x + offsetX + 100) / 580) * 100;
        const topPx = 18 + n.y + offsetY;
        const isCloneMarker = n.strategy === "clone-by" || n.strategy === "clone-from";
        const isClickable = !n.more;
        const handleClick = isClickable
          ? () =>
              navigate(
                `/marketplace/lineage/${isCloneMarker ? n.id : n.strategy}`,
              )
          : undefined;

        return (
          <div
            key={n.id}
            className="absolute flex flex-col items-center gap-1"
            style={{
              left: `${leftPct}%`,
              top: topPx,
              transform: "translate(-50%, -50%)",
              cursor: isClickable ? "pointer" : "default",
            }}
            onClick={handleClick}
            role={isClickable ? "button" : undefined}
            tabIndex={isClickable ? 0 : undefined}
            onKeyDown={
              isClickable
                ? (e) => {
                    if (e.key === "Enter" || e.key === " ") handleClick!();
                  }
                : undefined
            }
            aria-label={isClickable ? `View lineage: ${n.label}` : undefined}
          >
            {n.more ? (
              <div
                className="flex items-center justify-center font-mono text-[10.5px]"
                style={{
                  width: 36,
                  height: 36,
                  borderRadius: 4,
                  border: "1px dashed var(--info, #5FA8FF)",
                  color: "var(--info, #5FA8FF)",
                }}
              >
                {n.label}
              </div>
            ) : (
              <GenArtPlaceholder
                seed={n.genArtSeed ?? n.id}
                size={isExternal ? 32 : 38}
                className={
                  isHead
                    ? "border-2 border-gold"
                    : isExternal
                      ? "border border-dashed border-info/70 opacity-80"
                      : "border border-border"
                }
              />
            )}
            <span
              className="font-mono whitespace-nowrap text-[9.5px]"
              style={{
                color: isHead
                  ? "var(--gold)"
                  : isExternal
                    ? "var(--info, #5FA8FF)"
                    : "var(--text-2, #9CA3AF)",
              }}
            >
              {n.label}
            </span>
          </div>
        );
      })}
    </div>
  );
}

// ─── CreatorStat ──────────────────────────────────────────────────────────────

function CreatorStat({
  label,
  value,
  tone = "text",
  sub,
}: {
  label: string;
  value: string | number;
  tone?: "text" | "gold";
  sub?: React.ReactNode;
}) {
  return (
    <div className="py-4 pr-4 border-r border-border last:border-r-0">
      <div className="font-mono text-[9px] tracking-[0.2em] uppercase text-text-3 mb-1.5">
        {label}
      </div>
      <div
        className={`font-mono text-2xl font-semibold leading-none ${
          tone === "gold" ? "text-gold" : "text-text"
        }`}
      >
        {value}
      </div>
      {sub && (
        <div className="font-mono text-[10.5px] mt-1 text-text-3">{sub}</div>
      )}
    </div>
  );
}

// ─── CreatorStrategyCard ──────────────────────────────────────────────────────

function CreatorStrategyCard({
  strategy,
}: {
  strategy: ListingRow & { status: "live" | "archived" };
}) {
  const pos = strategy.return30dPct >= 0;
  return (
    <Link
      to={`/marketplace/lineage/${strategy.id}`}
      aria-label={strategy.id}
      className="block border border-border rounded-[5px] overflow-hidden bg-surface-card hover:border-border-strong transition-colors"
    >
      <div className="p-[10px_12px] flex items-center gap-2.5">
        <GenArtPlaceholder seed={strategy.genArtSeed} size={46} />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="font-mono text-[12px] text-text font-semibold truncate">
              {strategy.id}
            </span>
            <span className="font-mono text-[10px] text-text-3">{strategy.version}</span>
          </div>
          <div className="flex gap-1 mt-1 flex-wrap">
            {strategy.assets.map((a) => (
              <AssetPill key={a} asset={a} />
            ))}
            {strategy.verification === "verified" && <VerifiedBadge />}
          </div>
        </div>
      </div>
      <div className="p-[10px_12px] border-t border-border grid grid-cols-3 gap-2">
        <div>
          <div className="font-mono text-[8.5px] tracking-[0.16em] uppercase text-text-3">
            30D
          </div>
          <div
            className={`font-mono text-[13px] font-semibold mt-0.5 ${
              pos ? "text-gold" : "text-danger"
            }`}
          >
            {pos ? "+" : ""}
            {strategy.return30dPct}%
          </div>
        </div>
        <div>
          <div className="font-mono text-[8.5px] tracking-[0.16em] uppercase text-text-3">
            BUYERS
          </div>
          <div className="flex items-center gap-1 mt-0.5">
            <span className="font-mono text-[12px] text-text">
              {strategy.buyers.humans}
            </span>
            <span className="inline-flex items-center gap-0.5 font-mono text-[10.5px] text-gold">
              <AgentIcon size={8} />
              {strategy.buyers.agents}
            </span>
          </div>
        </div>
        <div>
          <div className="font-mono text-[8.5px] tracking-[0.16em] uppercase text-text-3">
            CLONES
          </div>
          <div
            className={`font-mono text-[12px] mt-0.5 ${
              strategy.clones > 0 ? "text-text" : "text-text-3"
            }`}
          >
            {strategy.clones > 0 ? strategy.clones : "—"}
          </div>
        </div>
      </div>
    </Link>
  );
}

// ─── VerdictPill ──────────────────────────────────────────────────────────────

const VERDICT_TONE: Record<Verdict, string> = {
  endorse: "border-gold text-gold",
  question: "border-warn text-warn",
  reject: "border-danger text-danger",
};
const VERDICT_DOT: Record<Verdict, string> = {
  endorse: "bg-gold",
  question: "bg-warn",
  reject: "bg-danger",
};

function VerdictPill({ verdict }: { verdict: Verdict }) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-1.5 py-0.5 border rounded-[3px] min-w-[80px] ${VERDICT_TONE[verdict]}`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${VERDICT_DOT[verdict]}`} />
      <span className="font-mono text-[9.5px] tracking-[0.14em] font-semibold uppercase">
        {verdict}
      </span>
    </span>
  );
}

// ─── ReputationFeedRow ────────────────────────────────────────────────────────

function ReputationFeedRow({ item }: { item: AttestationActivity }) {
  const isIssued = item.direction === "issued";
  const relTime = relativeTime(item.at);
  return (
    <div className="flex items-center gap-2.5 px-4 py-2.5 border-b border-border last:border-b-0">
      <VerdictPill verdict={item.verdict} />
      <span
        className={`font-mono text-[9px] tracking-[0.18em] uppercase ${
          isIssued ? "text-info" : "text-text-3"
        }`}
      >
        {item.direction}
      </span>
      <span className="font-mono text-[11.5px] text-text-2 flex-1 min-w-0 truncate">
        {isIssued ? `→ ${item.on}` : `${item.attester} → ${item.on}`}
      </span>
      <span className="font-mono text-[10.5px] text-text-3 ml-auto shrink-0">
        {relTime}
      </span>
    </div>
  );
}

// ─── CloneByRow ───────────────────────────────────────────────────────────────

function CloneByRow({ item, isLast }: { item: CloneByEntry; isLast: boolean }) {
  const relTime = new Date(item.at).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
  });
  const initial = item.handle.startsWith("@")
    ? item.handle[1].toUpperCase()
    : "?";
  return (
    <div
      className={`flex items-center gap-2.5 px-4 py-2.5 ${isLast ? "" : "border-b border-border"}`}
    >
      <div
        className={`w-6 h-6 rounded-full flex items-center justify-center font-mono text-[9.5px] text-text-3 shrink-0 ${
          item.more
            ? "border border-dashed border-border-strong bg-transparent"
            : "border border-border-strong bg-surface-elev"
        }`}
      >
        {item.more ? "…" : initial}
      </div>
      <div className="flex-1 min-w-0">
        <span className="font-mono text-[11.5px] text-text">{item.handle}</span>
        {!item.more && (
          <>
            <span className="font-mono text-[11px] text-text-3 mx-1.5">cloned</span>
            <span className="font-mono text-[11px] text-text-2">{item.from}</span>
            <span className="font-mono text-[11px] text-text-3 mx-1.5">→</span>
            <span className="font-mono text-[11px] text-text-2">{item.made}</span>
          </>
        )}
      </div>
      <span className="font-mono text-[11.5px] text-gold min-w-[60px] text-right">
        ${item.earnedUsd.toLocaleString()}
      </span>
      <span className="font-mono text-[10.5px] text-text-3 min-w-[54px] text-right">
        {relTime}
      </span>
    </div>
  );
}

// ─── On-chain creator page (0x… address params) ──────────────────────────────

/** Mirrors `WalletView` in marketplace_read.rs (only the fields used here). */
interface WalletViewOut {
  address: string;
  strategies: unknown[];
  licenses: unknown[];
  listings: IndexedListing[];
}

function OnChainCreatorListingCard({ listing }: { listing: IndexedListing }) {
  // Part A (.7): route via agent_id (ULID) when available for stable deep-links.
  const routingId = listing.agent_id || listing.listing_id;
  return (
    <Link
      to={`/marketplace/lineage/${routingId}`}
      data-testid="onchain-creator-listing"
      className="block border border-border rounded-[5px] overflow-hidden bg-surface-card hover:border-border-strong transition-colors"
    >
      <div className="p-[10px_12px] flex items-center gap-2.5">
        <GenArtPlaceholder seed={listing.gen_art_seed} size={46} />
        <div className="flex-1 min-w-0">
          <div className="font-mono text-[12px] text-text font-semibold truncate">
            {listing.name || `Listing #${listing.listing_id}`}
          </div>
          <div className="font-mono text-[11px] text-gold mt-0.5">
            {listing.price_usdc > 0 ? `${listing.price_usdc} USDC` : "FREE"}
          </div>
        </div>
      </div>
    </Link>
  );
}

function OnChainCreatorRoute({ address }: { address: string }) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["marketplace", "wallet", address],
    queryFn: () => apiFetch<WalletViewOut>(`/api/marketplace/wallet/${address}`),
    retry: false,
  });

  if (isLoading) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">Loading creator…</div>
    );
  }
  if (isError || !data) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">Creator not found.</div>
    );
  }

  const listings = data.listings.filter((l) => !l.revoked);

  return (
    <div
      data-testid="onchain-creator-page"
      className="flex flex-col overflow-y-auto"
    >
      {/* Address header */}
      <div
        className="border-b border-border flex items-center gap-[22px]"
        style={{ padding: "22px 28px 18px 44px" }}
      >
        <GenArtPlaceholder seed={address} size={96} />
        <div className="min-w-0">
          <div className="font-mono text-[18px] font-semibold text-text">
            {truncAddr(address)}
          </div>
          <div className="flex items-center gap-2 mt-1.5 flex-wrap">
            <a
              href={`https://explorer.mantle.xyz/address/${address}`}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-[10px] text-info hover:underline"
              title="View on Mantlescan"
            >
              ↗ Mantlescan
            </a>
            <span className="font-mono text-[11px] text-text-3">
              {listings.length} listing{listings.length !== 1 ? "s" : ""} on chain
            </span>
          </div>
        </div>
      </div>

      {/* Listings grid */}
      <div style={{ padding: "18px 28px 28px" }}>
        <div className="border border-border rounded-[5px] overflow-hidden">
          <div className="px-4 py-3 border-b border-border">
            <div className="font-mono text-[13px] font-semibold text-text">
              Listings
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              indexed from the on-chain marketplace
            </div>
          </div>
          <div className="p-3 grid grid-cols-3 gap-3">
            {listings.map((l) => (
              <OnChainCreatorListingCard key={l.listing_id} listing={l} />
            ))}
            {listings.length === 0 && (
              <div className="col-span-3 py-6 text-center font-mono text-[12px] text-text-3">
                No listings.
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── CreatorRoute (page) ──────────────────────────────────────────────────────

export function CreatorRoute() {
  const { handleOrAddr = "" } = useParams<{ handleOrAddr: string }>();
  // Real wallet addresses get the indexer-backed page; handles and fixture
  // slugs keep the fixture profile path untouched.
  if (/^0x[0-9a-fA-F]{40}$/.test(handleOrAddr)) {
    return <OnChainCreatorRoute address={handleOrAddr} />;
  }
  return <FixtureCreatorRoute handleOrAddr={handleOrAddr} />;
}

function FixtureCreatorRoute({ handleOrAddr }: { handleOrAddr: string }) {
  const mp = useMarketplaceData();

  const { data: profile, isLoading, isError } = useQuery({
    queryKey: ["marketplace", "creator", handleOrAddr],
    queryFn: () => mp.getCreator(handleOrAddr),
    retry: false,
  });

  const [strategyTab, setStrategyTab] = useState<"all" | "live" | "archived">("all");
  const [repTab, setRepTab] = useState<"all" | "received" | "issued">("all");

  if (isLoading) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">Loading creator…</div>
    );
  }
  if (isError || !profile) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">Creator not found.</div>
    );
  }

  const { creator, joinedAt, reputation, notableTag, counters, strategies,
    earningsWeekly, earningsSummary, forest, reputationFeed, clonedBy } = profile;

  const handle = creator.handle ?? truncAddr(creator.address);

  const visibleStrategies =
    strategyTab === "all"
      ? strategies
      : strategies.filter((s) => s.status === strategyTab);

  const visibleRep =
    repTab === "all"
      ? reputationFeed
      : reputationFeed.filter((r) => r.direction === repTab);

  const lineageCount = new Set(
    forest.nodes
      .filter((n) => n.strategy !== "clone-by" && n.strategy !== "clone-from" && !n.external)
      .map((n) => n.strategy),
  ).size;

  const profileUrl =
    typeof window !== "undefined"
      ? `${window.location.origin}/marketplace/creator/${handleOrAddr}`
      : `/marketplace/creator/${handleOrAddr}`;

  return (
    <div className="flex flex-col overflow-y-auto">
      {/* ── HERO ── */}
      <div
        className="border-b border-border"
        style={{ padding: "22px 28px 18px 44px" }}
      >
        <div
          className="grid items-center gap-[22px]"
          style={{ gridTemplateColumns: "96px 1fr 280px" }}
        >
          {/* Identicon */}
          <GenArtPlaceholder seed={creator.address} size={96} />

          {/* Identity column */}
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-mono text-[18px] font-semibold text-text">
                {handle}
              </span>
              {creator.ens && (
                <span className="font-mono text-[11px] px-1.5 py-0.5 border border-border rounded-[3px] text-text-2">
                  {creator.ens}
                </span>
              )}
              {notableTag && (
                <span className="font-mono text-[10px] px-1.5 py-0.5 border border-gold/40 rounded-[3px] text-gold">
                  {notableTag}
                </span>
              )}
            </div>
            <div className="flex items-center gap-2 mt-1.5 flex-wrap">
              <span className="font-mono text-[11px] text-text-3">
                {truncAddr(creator.address)}
              </span>
              <a
                href={`https://explorer.mantle.xyz/address/${creator.address}`}
                target="_blank"
                rel="noopener noreferrer"
                className="font-mono text-[10px] text-info hover:underline"
                title="View on Mantlescan"
              >
                ↗ Mantlescan
              </a>
            </div>
            <div className="flex items-center gap-3 mt-1.5">
              <span className="font-mono text-[11px] text-text-3">
                Joined {relativeDate(joinedAt)}
              </span>
              <span className="font-mono text-[11px] text-gold">
                Rep {reputation.toFixed(1)}
              </span>
            </div>
          </div>

          {/* Action column */}
          <div className="flex flex-col gap-2">
            <button
              disabled
              title="Follow is a deferred affordance — on-chain follow registry not yet wired"
              aria-label="Follow (coming soon)"
              className="w-full font-mono text-[12px] px-3 py-2 border border-border rounded-[4px] text-text-3 opacity-50 cursor-not-allowed"
            >
              Follow {handle}
            </button>
            <div className="flex gap-2">
              <a
                href={profileUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex-1 font-mono text-[11px] px-2 py-1.5 border border-border rounded-[4px] text-text-2 hover:border-border-strong transition-colors text-center"
              >
                Share
              </a>
              <button
                disabled
                title="Tip is a deferred affordance — on-chain tip relay not yet wired"
                aria-label="Tip (coming soon)"
                className="flex-1 font-mono text-[11px] px-2 py-1.5 border border-border rounded-[4px] text-text-3 opacity-50 cursor-not-allowed"
              >
                Tip
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* ── COUNTERS ── */}
      <div
        className="border-b border-border"
        style={{ padding: "0 28px 0 44px" }}
      >
        <div className="grid" style={{ gridTemplateColumns: "repeat(6, 1fr)" }}>
          <CreatorStat label="Strategies" value={counters.strategies} />
          <CreatorStat
            label="Lifetime earned"
            value={formatUsd(counters.lifetimeEarnedUsd)}
            tone="gold"
          />
          <CreatorStat
            label="Total buyers"
            value={counters.totalBuyers.humans}
            sub={
              <span className="inline-flex items-center gap-0.5">
                <AgentIcon size={9} />
                +{counters.totalBuyers.agents} agents
              </span>
            }
          />
          <CreatorStat
            label="Clones spawned"
            value={counters.clonesSpawned}
            sub={`upstream of ${formatUsd(counters.clonesUpstreamUsd)}`}
          />
          <CreatorStat
            label="Attestations"
            value={counters.attestationsIssued}
            sub="issued"
          />
          <CreatorStat
            label="Member since"
            value={relativeDate(joinedAt)}
          />
        </div>
      </div>

      {/* ── STRATEGIES + EARNINGS ── */}
      <div
        className="grid gap-6"
        style={{ padding: "18px 28px 0", gridTemplateColumns: "1fr 380px" }}
      >
        {/* Strategies card */}
        <div className="border border-border rounded-[5px] overflow-hidden">
          <div className="px-4 py-3 border-b border-border">
            <div className="font-mono text-[13px] font-semibold text-text">
              Strategies
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              {counters.strategies} on chain · sorted by buyers
            </div>
            {/* Tab row */}
            <div className="flex gap-1 mt-2.5">
              {(["all", "live", "archived"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setStrategyTab(tab)}
                  className={`font-mono text-[10.5px] px-2.5 py-1 rounded-[3px] capitalize transition-colors ${
                    strategyTab === tab
                      ? "bg-surface-elev text-text border border-border"
                      : "text-text-3 hover:text-text-2"
                  }`}
                >
                  {tab === "all" ? "All" : tab.charAt(0).toUpperCase() + tab.slice(1)}
                </button>
              ))}
            </div>
          </div>
          <div className="p-3 grid grid-cols-3 gap-3">
            {visibleStrategies.map((s) => (
              <CreatorStrategyCard key={s.id} strategy={s} />
            ))}
            {visibleStrategies.length === 0 && (
              <div className="col-span-3 py-6 text-center font-mono text-[12px] text-text-3">
                No strategies.
              </div>
            )}
          </div>
        </div>

        {/* Earnings card */}
        <div className="border border-border rounded-[5px] overflow-hidden">
          <div className="px-4 py-3 border-b border-border">
            <div className="font-mono text-[13px] font-semibold text-text">
              Earnings · weekly
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              USDC paid to wallet · 5% platform fee deducted
            </div>
          </div>
          <div className="px-4 pt-3 pb-2">
            <EarningsChart data={earningsWeekly} />
            <div className="flex justify-between mt-1">
              <span className="font-mono text-[10.5px] text-text-3">
                {earningsWeekly.length} weeks ago
              </span>
              <span className="font-mono text-[10.5px] text-text-3">today</span>
            </div>
          </div>
          <div className="px-4 py-2.5 border-t border-border">
            <span className="font-mono text-[11px] text-gold">
              +{formatUsd(earningsSummary.last7dUsd)} last 7d ·{" "}
              +{formatUsd(earningsSummary.last30dUsd)} last 30d
            </span>
          </div>
        </div>
      </div>

      {/* ── LINEAGE FOREST ── */}
      <div style={{ padding: "18px 28px 0" }}>
        <div className="border border-border rounded-[5px] overflow-hidden">
          <div className="px-4 py-3 border-b border-border flex items-start justify-between">
            <div>
              <div className="font-mono text-[13px] font-semibold text-text">
                Lineage forest
              </div>
              <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
                {lineageCount} lineage{lineageCount !== 1 ? "s" : ""} tracked
              </div>
            </div>
            {/* Legend */}
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-1.5">
                <div
                  className="w-2 h-2 rounded-sm"
                  style={{ border: "2px solid var(--gold)" }}
                />
                <span className="font-mono text-[9.5px] text-text-3">HEAD</span>
              </div>
              <div className="flex items-center gap-1.5">
                <div
                  className="w-2 h-2 rounded-sm border border-border"
                />
                <span className="font-mono text-[9.5px] text-text-3">HISTORY</span>
              </div>
              <div className="flex items-center gap-1.5">
                <div
                  className="w-2 h-2 rounded-sm"
                  style={{ border: "1px dashed var(--info, #5FA8FF)" }}
                />
                <span className="font-mono text-[9.5px] text-text-3">CLONE</span>
              </div>
            </div>
          </div>
          <LineageForest nodes={forest.nodes} edges={forest.edges} />
        </div>
      </div>

      {/* ── ATTESTATIONS + CLONED-BY ── */}
      <div
        className="grid gap-[18px]"
        style={{ padding: "18px 28px 28px", gridTemplateColumns: "1fr 1fr" }}
      >
        {/* Reputation card */}
        <div className="border border-border rounded-[5px] overflow-hidden">
          <div className="px-4 py-3 border-b border-border">
            <div className="font-mono text-[13px] font-semibold text-text">
              Reputation
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              {reputationFeed.filter((r) => r.direction === "issued").length} issued ·{" "}
              {reputationFeed.filter((r) => r.direction === "received").length} received ·{" "}
              {reputationFeed.filter((r) => r.verdict === "question").length} questions ·{" "}
              {reputationFeed.filter((r) => r.verdict === "reject").length} rejects
            </div>
            {/* Filter tabs */}
            <div className="flex gap-1 mt-2.5">
              {(["all", "received", "issued"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setRepTab(tab)}
                  className={`font-mono text-[10.5px] px-2.5 py-1 rounded-[3px] capitalize transition-colors ${
                    repTab === tab
                      ? "bg-surface-elev text-text border border-border"
                      : "text-text-3 hover:text-text-2"
                  }`}
                >
                  {tab === "all" ? "All" : tab.charAt(0).toUpperCase() + tab.slice(1)}
                </button>
              ))}
            </div>
          </div>
          <div>
            {visibleRep.map((item, i) => (
              <ReputationFeedRow key={i} item={item} />
            ))}
            {visibleRep.length === 0 && (
              <div className="px-4 py-6 text-center font-mono text-[12px] text-text-3">
                No activity.
              </div>
            )}
          </div>
        </div>

        {/* Cloned-by card */}
        <div className="border border-border rounded-[5px] overflow-hidden">
          <div className="px-4 py-3 border-b border-border">
            <div className="font-mono text-[13px] font-semibold text-text">
              Cloned by · downstream
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mt-0.5">
              {clonedBy.filter((c) => !c.more).length} clones of {handle}'s work ·
              upstream of {formatUsd(counters.clonesUpstreamUsd)} earnings
            </div>
          </div>
          <div>
            {clonedBy.map((item, i) => (
              <CloneByRow
                key={i}
                item={item}
                isLast={i === clonedBy.length - 1}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
