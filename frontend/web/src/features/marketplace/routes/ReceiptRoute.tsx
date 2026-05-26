// src/features/marketplace/routes/ReceiptRoute.tsx
// F6 — /marketplace/receipts/:tx — post-buy install + share surface.
// No modals. All panels are inline. Post targets open new tabs.
// Data fetched via useQuery per integration addendum §1.
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { Receipt } from "@/features/marketplace/data/types";
import { TxChip } from "@/features/marketplace/components/TxChip";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
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
  const rows: [string, React.ReactNode, "gold" | "mono" | "muted" | "link"][] = [
    ["strategy", <span key="strategy" className="text-gold">{listing.id}</span>, "gold"],
    ["version", listing.version, "mono"],
    ["creator", listing.creator.handle ?? listing.creator.address, "mono"],
    ["manifest", license.manifestHash, "mono"],
    ["bundle", `ipfs://${license.bundleCid}`, "link"],
    [
      "paid",
      `${license.pricePaidUsdc} USDC (5% fee · ${license.feeUsdc} USDC)`,
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
            You bought{" "}
            <span className="font-mono text-gold">{listing.id}</span>
          </h1>
          <div className="mt-1.5 font-mono text-[11.5px] text-text-3 flex flex-wrap items-center gap-x-2 gap-y-0.5">
            <span>
              <span className="text-gold">{license.pricePaidUsdc} USDC</span>{" "}
              paid
            </span>
            <span className="text-text-4">·</span>
            <span>
              license{" "}
              <span className="text-text-2">{license.tokenId}</span> minted
            </span>
            <span className="text-text-4">·</span>
            <span>
              {license.netToCreatorUsdc} USDC → {creatorLabel}
            </span>
            <span className="text-text-4">·</span>
            <TxChip hash={receipt.txHash} network={network} />
          </div>
        </div>

        <a
          href={`https://${network.includes("sepolia") ? "sepolia." : ""}mantlescan.xyz/tx/${receipt.txHash}`}
          target="_blank"
          rel="noreferrer"
          className="shrink-0 font-mono text-[12px] text-text-3 border border-border-strong rounded px-2.5 py-1.5 hover:text-text hover:border-border-strong/80 flex items-center gap-1.5"
        >
          View on Mantlescan
          <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
            <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </a>
      </div>

      {/* ── Body: 3-col ─────────────────────────────────────────────────────── */}
      <div
        className="flex-1 min-h-0 overflow-auto p-4"
        style={{ display: "grid", gridTemplateColumns: "320px 1fr 380px", gap: 14 }}
      >
        {/* Col 1 — License NFT */}
        <Panel
          title="License NFT"
          sub="non-transferable · ERC-1155 on Mantle"
        >
          <LicenseCard receipt={receipt} />
        </Panel>

        {/* Col 2 — Install steps */}
        <Panel
          title="Install in your XVN"
          sub={
            receipt.install.xvnDetected
              ? `detected at ${receipt.install.xvnEndpoint} · 4 steps · sealed bundle auto-decrypts`
              : "XVN not detected · install XVN first"
          }
          right={
            <button className="font-mono text-[12px] bg-gold text-black px-3 py-1.5 rounded hover:opacity-90 font-semibold">
              Install all
            </button>
          }
        >
          <InstallSteps receipt={receipt} />
        </Panel>

        {/* Col 3 — Share composer */}
        <Panel
          title="Share"
          sub="OG card pre-loaded · post to X / Farcaster / Discord"
        >
          <ShareComposer share={receipt.share} />
        </Panel>
      </div>
    </div>
  );
}
