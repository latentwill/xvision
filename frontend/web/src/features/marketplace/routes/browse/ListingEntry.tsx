// A marketplace browse row (replaces ListingCard's role). The WHOLE row is a
// <Link> to the inspector — no list-row tx (QA10/QA12). App-native styling:
// plain thumbnail with a border-border frame, Geist title, muted captions, and
// the existing accent for prices/CTAs. Honest data discipline (spec §0.5): no
// fabricated numbers; absent fields become muted captions, never a blank cell
// or a zero.
import { Link } from "react-router-dom";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { MiniSparkline } from "@/components/chart/v2/primitives/MiniSparkline";
import { isFreeListing } from "@/features/marketplace/data/pricing";
import type { ListingRow } from "@/features/marketplace/data/types";

// Known financial/asset acronyms that should render in all-caps when a slug is
// humanized (e.g. `btc-momentum-v3` → "BTC Momentum v3", not "Btc Momentum V3").
const ACRONYMS = new Set([
  "btc", "eth", "sol", "doge", "mnt", "avax", "usdc", "usdt", "ai", "ml",
  "dca", "rsi", "macd", "atr", "ema", "sma", "mr", "ls", "nft", "defi",
]);

/**
 * Humanize a listing id for display.
 *  - numeric ids → "Strategy #<id>"
 *  - slug ids    → Title Case with known acronyms upper-cased and version
 *    segments (`v3`, `v2.1`) kept lowercase: `btc-momentum-v3` → "BTC Momentum v3".
 * Mirrors the helper in sell/ListingPreviewCard so browse + mint preview match.
 */
export function humanize(id: string | number): string {
  const s = String(id);
  if (/^\d+$/.test(s)) return `Strategy #${s}`;
  return s
    .replace(/[-_]/g, " ")
    .split(" ")
    .map((word) => {
      if (!word) return word;
      const lower = word.toLowerCase();
      if (ACRONYMS.has(lower)) return word.toUpperCase();
      // Version segments stay lowercase: v3, v2.1, v10
      if (/^v\d+(\.\d+)?$/.test(lower)) return lower;
      return word.charAt(0).toUpperCase() + word.slice(1);
    })
    .join(" ");
}

interface ListingEntryProps {
  row: ListingRow;
  /** Stagger index (reserved; no per-row animation in the app-native row). */
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
// the dev-fixtures build — never the real-client path.
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

export function ListingEntry({ row, index: _index = 0, showSparkline = false }: ListingEntryProps) {
  const displayTitle = row.name ?? humanize(row.id);
  const isOpen = isFreeListing(row);
  const tierLabel = isOpen ? "Open edition" : "Sealed";
  const positive = row.return30dPct >= 0;
  // Honest performance: only show a number/sparkline when there is a real
  // return AND (in this build) we are on the demo/fixture client. Real on-chain
  // rows (return === 0, no equity) get the dignified pending caption.
  const hasRealReturn = showSparkline && row.return30dPct !== 0;

  return (
    <Link
      to={`/marketplace/lineage/${row.id}`}
      data-listing-entry={row.id}
      className="group grid items-start gap-5 py-4 px-4 sm:px-7 border-b border-border-soft hover:bg-surface-hover transition-colors"
      style={{ gridTemplateColumns: "80px 1fr auto" }}
    >
      {/* Zone A — THUMBNAIL */}
      <div className="rounded border border-border overflow-hidden">
        <GenArtPlaceholder seed={row.genArtSeed} size={78} />
      </div>

      {/* Zone B — DETAILS */}
      <div className="flex flex-col gap-1 min-w-0">
        {/* Line 1 — title */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span
            title={displayTitle}
            className="text-[14px] font-medium leading-[1.2] text-text line-clamp-2 break-words"
          >
            {displayTitle}
          </span>
          {row.verification === "verified" && <VerifiedBadge />}
          <span className="font-mono text-[11px] text-text-3 shrink-0">{row.version}</span>
        </div>

        {/* Line 2 — provenance caption (· separated, comfortable with absence) */}
        <p className="text-[12px] text-text-3 leading-[1.4] flex items-center gap-1.5 flex-wrap">
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
              <span>{row.model}</span>
            </>
          )}
          {row.style && (
            <>
              <span className="text-text-4">·</span>
              <span>{row.style}</span>
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
            <span className="font-mono text-[11.5px]">
              <span className="text-text-3">30d return </span>
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
          <p className="text-[12px] text-text-3 mt-0.5" data-perf-pending>
            Performance pending first live cycle
          </p>
        )}
      </div>

      {/* Zone C — ACQUISITION */}
      <div className="shrink-0 flex flex-col items-end justify-start gap-2 text-right">
        {isOpen ? (
          <span className="font-mono text-[10.5px] uppercase tracking-[0.06em] border border-border text-text-3 px-2 py-1 rounded">
            Open edition
          </span>
        ) : (
          <div className="flex flex-col items-end">
            <span className="font-mono text-[9px] uppercase tracking-[0.08em] text-text-3">
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
            "inline-flex items-center justify-center px-3 py-1.5 rounded text-[12px] font-medium",
            isOpen
              ? "bg-gold text-bg"
              : "border border-border text-text-2 group-hover:border-border-strong group-hover:text-text transition-colors",
          ].join(" ")}
        >
          {isOpen ? "Run free" : "Acquire"}
        </span>
      </div>
    </Link>
  );
}
