// src/features/marketplace/routes/ReceiptRoute.tsx
// F6 — /marketplace/receipts/:tx — post-buy install + share surface.
// No modals. All panels are inline. Post targets open new tabs.
// Data fetched via useQuery per integration addendum §1.
// Layout: 2-column (320px license | 1fr install). Share collapsed inline below install.
import { useState } from "react";
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { Receipt } from "@/features/marketplace/data/types";
import { TxChip } from "@/features/marketplace/components/TxChip";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { humanize } from "./browse/ListingEntry";
import { InstallSteps } from "./InstallSteps";
import { ShareComposer } from "./ShareComposer";

// ── card wrapper matching F0 design tokens ──────────────────────────────────
function Panel({
  title,
  sub,
  right,
  children,
}: {
  title: string;
  sub?: React.ReactNode;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-md border border-border bg-surface-card flex flex-col overflow-hidden">
      <div className="flex items-start justify-between gap-3 px-4 py-3 border-b border-border">
        <div className="min-w-0">
          <div className="text-[13.5px] font-semibold text-text leading-tight">{title}</div>
          {sub ? (
            <div className="mt-0.5 font-mono text-[11px] text-text-3 leading-snug">{sub}</div>
          ) : null}
        </div>
        {right ? <div className="shrink-0">{right}</div> : null}
      </div>
      {children}
    </div>
  );
}

// ── license metadata stack ───────────────────────────────────────────────────
function LicenseCard({ receipt }: { receipt: Receipt }) {
  const { license, listing } = receipt;
  // Display name, not the raw URL slug — the licence reads as an editorial
  // record, not a database key (spec §3.3).
  const displayName =
    (listing as Receipt["listing"] & { name?: string }).name ?? humanize(listing.id);
  const rows: [string, React.ReactNode, "gold" | "mono" | "muted" | "link"][] = [
    ["strategy", <span key="strategy" className="text-gold">{displayName}</span>, "gold"],
    ["version", listing.version, "mono"],
    ["creator", listing.creator.handle ?? listing.creator.address, "mono"],
    ["manifest", license.manifestHash, "mono"],
    ["bundle", `ipfs://${license.bundleCid}`, "link"],
    [
      "paid",
      `${license.pricePaidUsdc} USDC`,
      "muted",
    ],
    ["minted", license.mintedAt, "mono"],
  ];
  return (
    <div className="p-4 flex flex-col gap-4">
      <div className="relative">
        <GenArtPlaceholder seed={listing.genArtSeed} size={290} className="w-full !rounded-[5px]" />
        {/* LICENSE #N overlay — top-left */}
        <div className="absolute top-2 left-2 px-2 py-0.5 rounded-sm bg-black/75 backdrop-blur">
          <span className="font-mono text-[10px] font-semibold tracking-[0.14em] text-text">
            LICENSE {license.tokenId}
          </span>
        </div>
        {/* OWNED · YOU overlay — bottom-right */}
        <div className="absolute bottom-2 right-2 px-2 py-0.5 rounded-sm bg-gold/10 border border-gold-soft">
          <span className="font-mono text-[9.5px] font-semibold tracking-[0.14em] text-gold">
            OWNED · YOU
          </span>
        </div>
      </div>

      <div className="flex flex-col gap-1.5">
        {rows.map(([label, value, tone]) => (
          <div key={label} className="grid gap-2" style={{ gridTemplateColumns: "72px 1fr" }}>
            <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-text-3 leading-relaxed">
              {label}
            </span>
            <span
              className={[
                "font-mono text-[11px] break-all leading-relaxed",
                tone === "gold" ? "text-gold" : "",
                tone === "link" ? "text-info underline decoration-dotted underline-offset-2" : "",
                tone === "muted" ? "text-text-3" : "",
                tone === "mono" ? "text-text-2" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              {value}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── page ─────────────────────────────────────────────────────────────────────
export function ReceiptRoute() {
  const { tx = "" } = useParams<{ tx: string }>();
  const mp = useMarketplaceData();
  const [shareExpanded, setShareExpanded] = useState(false);

  const { data: receipt, isLoading, error } = useQuery({
    queryKey: ["marketplace", "receipt", tx],
    queryFn: () => mp.getReceipt(tx),
  });

  if (error) {
    return (
      <div className="px-7 py-8 text-[13px] text-danger font-mono">
        Receipt not found: {String(error)}
      </div>
    );
  }

  if (isLoading || !receipt) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">
        Loading receipt…
      </div>
    );
  }

  const { listing, license, network } = receipt;
  const creatorLabel = listing.creator.handle ?? listing.creator.address;
  // Display name, not the raw URL slug — this is the emotional payoff of the buy
  // flow and must read as an editorial title, not a database key (spec §3.3).
  const displayName =
    (listing as Receipt["listing"] & { name?: string }).name ?? humanize(listing.id);
  // Header branches on whether a price was paid (spec 3.3, QA12)
  const isPaid = license.pricePaidUsdc > 0;
  const headerTitle = isPaid
    ? `Acquired ${displayName}`
    : `Activated ${displayName} — added to your strategies`;

  return (
    <div className="flex flex-col min-h-0">
      {/* ── Success header strip ────────────────────────────────────────────── */}
      <div
        className="flex items-center gap-4 px-7 py-4 border-b border-border"
        style={{
          background: "linear-gradient(90deg, rgba(0,230,118,0.10), rgba(0,230,118,0.02))",
        }}
      >
        {/* 44px green check circle */}
        <div className="w-11 h-11 rounded-full flex items-center justify-center shrink-0 bg-gold/[0.18] border border-gold">
          <svg
            width="22"
            height="22"
            viewBox="0 0 22 22"
            fill="none"
            stroke="var(--gold)"
            strokeWidth="2.4"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M4 11l5 5 9-11" />
          </svg>
        </div>

        <div className="flex-1 min-w-0">
          <h1 className="text-2xl font-semibold tracking-tight leading-tight">
            {headerTitle}
          </h1>
          <div className="mt-1.5 font-mono text-[11.5px] text-text-3 flex flex-wrap items-center gap-x-2 gap-y-0.5">
            {isPaid && (
              <>
                <span>
                  <span className="text-gold">{license.pricePaidUsdc} USDC</span>{" "}
                  paid
                </span>
                <span className="text-text-4">·</span>
              </>
            )}
            <span>
              license{" "}
              <span className="text-text-2">{license.tokenId}</span> minted
            </span>
            <span className="text-text-4">·</span>
            <span>
              {license.netToCreatorUsdc} USDC → {creatorLabel}
            </span>
            <span className="text-text-4">·</span>
            {/* Single explorer link via TxChip (QA16 — fixes hand-built mantlescan href) */}
            <TxChip hash={receipt.txHash} network={network} label="View on explorer" />
          </div>
        </div>
      </div>

      {/* ── Body: 2-col (spec 3.4, QA13 layout rule — no 380px third column) ─── */}
      <div
        className="flex-1 min-h-0 overflow-auto p-4"
        style={{ display: "grid", gridTemplateColumns: "320px 1fr", gap: 14 }}
      >
        {/* Col 1 — License NFT */}
        <Panel
          title="License NFT"
          sub="non-transferable · ERC-1155 on Mantle"
        >
          <LicenseCard receipt={receipt} />
        </Panel>

        {/* Col 2 — Install steps + collapsed share accordion */}
        <div className="flex flex-col gap-3.5">
          <Panel
            title="Install in your XVN"
            sub="license-gated import · sealed bundles decrypt with your wallet"
            right={
              <button className="font-mono text-[12px] bg-gold text-black px-3 py-1.5 rounded hover:opacity-90 font-semibold motion-safe:active:scale-[0.96]">
                Install all
              </button>
            }
          >
            <InstallSteps receipt={receipt} />
          </Panel>

          {/* Collapsed share accordion — default state is a ~56px strip (QA13) */}
          <div className="rounded-md border border-border bg-surface-card overflow-hidden">
            <div className="flex items-center justify-between gap-3 px-4 py-3">
              <div className="flex items-center gap-3">
                <span className="text-[13px] font-semibold text-text">Share this acquisition</span>
                <button
                  onClick={() => {
                    try {
                      navigator.clipboard?.writeText(
                        `https://${receipt.share.ogCard.url}`
                      );
                    } catch {
                      // clipboard not available
                    }
                  }}
                  className="font-mono text-[11px] text-text-3 border border-border-strong rounded px-2 py-1 hover:text-text"
                >
                  Copy link
                </button>
              </div>
              <button
                onClick={() => setShareExpanded((v) => !v)}
                className="font-mono text-[11px] text-text-3 border border-border-strong rounded px-2 py-1 hover:text-text shrink-0"
              >
                {shareExpanded ? "Collapse" : "Customize post"}
              </button>
            </div>
            {shareExpanded && (
              <div className="border-t border-border">
                {/* initialExpanded=true: the full composer is visible once the outer accordion opens */}
                <ShareComposer share={receipt.share} initialExpanded={true} />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
