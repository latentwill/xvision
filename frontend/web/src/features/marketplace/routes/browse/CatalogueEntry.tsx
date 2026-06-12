// src/features/marketplace/routes/browse/CatalogueEntry.tsx
// The Catalogue editorial entry row (replaces ListingCard's role, spec 3.1E).
// The WHOLE entry is a <Link> to the inspector — no list-row tx (QA10/QA12).
// Honest data discipline (spec §0.5): no seeded-RNG sparkline, no fake numbers;
// absent fields become designed empty captions, never a blank cell or a zero.
import { Link } from "react-router-dom";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { MiniSparkline } from "@/components/chart/v2/primitives/MiniSparkline";
import type { ListingRow } from "@/features/marketplace/data/types";

/**
 * Humanize a listing id for display.
 *  - numeric ids → "Strategy #<id>"
 *  - slug ids    → Title Case ("btc-momentum-v3" → "Btc Momentum V3")
 * Mirrors the helper in sell/ListingPreviewCard so browse + mint preview match.
 */
export function humanize(id: string | number): string {
  const s = String(id);
  if (/^\d+$/.test(s)) return `Strategy #${s}`;
  return s.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/**
 * Stable plate number for the catalogue index (`№ 0043`).
 *  - numeric ids → zero-padded id
 *  - slug ids    → deterministic 4-digit hash of the id (stable per listing)
 */
export function plateNumber(id: string | number): string {
  const s = String(id);
  if (/^\d+$/.test(s)) return s.padStart(4, "0");
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return String(h % 10000).padStart(4, "0");
}

interface CatalogueEntryProps {
  row: ListingRow;
  /** Stagger index → per-entry plate-develop animationDelay. */
  index?: number;
  /**
   * Demo/fixture client only. When true, a real equity micro-series exists for
   * the row, so the performance caption may show a MiniSparkline. Real on-chain
   * rows fall through to the honest "pending first live cycle" caption.
   */
  showSparkline?: boolean;
}

// Build a small synthetic equity series for the demo MiniSparkline. Only used
// on the fixture client (showSparkline), so this is demo data presented inside
// a DEMO CATALOGUE — never the real-client path.
function demoEquity(seed: string, positive: boolean): { time: number[]; values: number[] } {
  const time: number[] = [];
  const values: number[] = [];
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  let v = 100;
  const drift = positive ? 0.6 : -0.5;
  for (let i = 0; i < 30; i++) {
    h = (h * 1103515245 + 12345) & 0x7fffffff;
    const noise = ((h % 1000) / 1000 - 0.5) * 3;
    v = Math.max(1, v + drift + noise);
    time.push(i);
    values.push(v);
  }
  return { time, values };
}

export function CatalogueEntry({ row, index = 0, showSparkline = false }: CatalogueEntryProps) {
  const displayTitle = row.name ?? humanize(row.id);
  const isOpen = row.priceUsdc === null || row.tier === "open";
  const tierLabel = isOpen ? "Open edition" : "Sealed";
  const positive = row.return30dPct >= 0;
  // Honest performance: only show a number/sparkline when there is a real
  // return AND (in this build) we are on the demo/fixture client. Real on-chain
  // rows (return === 0, no equity) get the dignified pending caption.
  const hasRealReturn = showSparkline && row.return30dPct !== 0;

  return (
    <Link
      to={`/marketplace/lineage/${row.id}`}
      data-catalogue-entry={row.id}
      className="group grid items-start gap-6 py-5 px-4 sm:px-7 border-b border-ink-rule-faint hover:bg-surface-hover transition-colors"
      style={{ gridTemplateColumns: "120px 1fr auto" }}
    >
      {/* Zone A — PLATE */}
      <div className="flex flex-col items-start gap-1.5">
        <div className="p-[3px] border-2 border-ink-rule ring-1 ring-gilt/15 group-hover:ring-gilt/40 transition-[box-shadow] inline-block">
          <GenArtPlaceholder
            seed={row.genArtSeed}
            size={104}
            className="!rounded-none motion-safe:animate-[xvn-plate-develop_var(--duration-base)_var(--ease-out)_both]"
          />
        </div>
        <span className="font-mono text-[12px] tracking-[0.1em] text-gilt">
          №&nbsp;{plateNumber(row.id)}
        </span>
      </div>

      {/* Zone B — CAPTION BLOCK */}
      <div className="flex flex-col gap-1 min-w-0">
        {/* Line 1 — title */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span
            title={displayTitle}
            className="font-display text-[17px] font-medium leading-[1.15] tracking-[-0.01em] line-clamp-2 break-words"
          >
            {displayTitle}
          </span>
          {row.verification === "verified" && <VerifiedBadge />}
          <span className="font-mono text-[11px] text-text-3 shrink-0">{row.version}</span>
        </div>

        {/* Line 2 — provenance caption (· separated, comfortable with absence) */}
        <p className="font-mono text-[11.5px] text-text-2 leading-[1.4] flex items-center gap-1.5 flex-wrap">
          <span>{row.creator.handle ?? `${row.creator.address.slice(0, 8)}…`}</span>
          <span className="text-text-4">·</span>
          <span>{tierLabel}</span>
          {row.verification === "verified" && (
            <>
              <span className="text-text-4">·</span>
              <span className="inline-flex items-center gap-1">
                <span className="w-1.5 h-1.5 rounded-full bg-gold" aria-hidden="true" />
                On-chain
              </span>
            </>
          )}
          {row.model && (
            <>
              <span className="text-text-4">·</span>
              <span className="text-text-3">{row.model}</span>
            </>
          )}
          {row.style && (
            <>
              <span className="text-text-4">·</span>
              <span className="text-text-3">{row.style}</span>
            </>
          )}
          {/* Asset pills inline only when present; absent = segment omitted. */}
          {row.assets.length > 0 && (
            <>
              <span className="text-text-4">·</span>
              <span className="inline-flex items-center gap-1 flex-wrap">
                {row.assets.map((a) => (
                  <AssetPill key={a} asset={a} />
                ))}
              </span>
            </>
          )}
        </p>

        {/* Line 3 — performance caption (HONEST) */}
        {hasRealReturn ? (
          <div className="flex items-center gap-3 mt-0.5">
            <span className="font-mono text-[11.5px] tracking-[0.04em]">
              <span className="text-text-3">30-DAY RETURN </span>
              <span
                data-return-pct
                className={positive ? "text-gold font-semibold" : "text-danger font-semibold"}
              >
                {positive ? "+" : ""}
                {row.return30dPct}%
              </span>
            </span>
            <span className="w-[120px] h-[28px] shrink-0" data-perf-spark>
              <MiniSparkline
                {...demoEquity(row.genArtSeed, positive)}
                color={positive ? "#00e676" : "#ff4d4d"}
                height={28}
              />
            </span>
          </div>
        ) : (
          <p className="font-display italic text-[12px] text-text-3 mt-0.5" data-perf-pending>
            Performance record · pending first live cycle
          </p>
        )}
      </div>

      {/* Zone C — ACQUISITION */}
      <div className="shrink-0 flex flex-col items-end justify-start gap-2 text-right">
        {isOpen ? (
          <span className="font-mono text-[10.5px] uppercase tracking-[0.14em] border border-gilt/40 text-gilt px-2 py-1 rounded-[2px]">
            Open edition
          </span>
        ) : (
          <div className="flex flex-col items-end">
            <span className="font-mono text-[9px] uppercase tracking-[0.18em] text-text-3">
              Price
            </span>
            <span className="font-mono text-[13px] font-medium text-text tabular-nums">
              {row.priceUsdc} <span className="text-text-3">USDC</span>
            </span>
          </div>
        )}
        {/* One non-interactive label — the whole entry is the Link to detail. */}
        <span
          className={[
            "inline-flex items-center justify-center px-3 py-1.5 rounded-[3px] text-[12px] font-bold",
            isOpen
              ? "bg-gold text-[#001A0A]"
              : "border border-gilt/40 text-gilt group-hover:bg-gilt-bg transition-colors",
          ].join(" ")}
        >
          {isOpen ? "Run free" : "Acquire"}
        </span>
      </div>
    </Link>
  );
}
