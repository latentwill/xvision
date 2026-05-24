/**
 * MarketContextCard — BTC market context block on the B4 hero
 * dashboard. 2×2 stats grid (Price / Funding / Open Interest /
 * Liq 24h) + a regime chip row.
 *
 * B4 ships a literal-data version (caller passes a `data` payload
 * shaped to the props). A follow-up will wire this to a real backend
 * endpoint when one exists.
 */
import type { ReactElement } from "react";

export interface MarketContextData {
  price: number;
  fundingPct: number;
  openInterestUsd: number;
  liq24hUsd: number;
}

export interface RegimeWeight {
  label: string;
  pct: number;
}

export interface MarketContextCardProps {
  data: MarketContextData;
  regimes: RegimeWeight[];
}

function fmtUsd(n: number): string {
  if (Math.abs(n) >= 1_000_000_000)
    return `$${(n / 1_000_000_000).toFixed(2)}B`;
  if (Math.abs(n) >= 1_000_000) return `$${(n / 1_000_000).toFixed(1)}M`;
  if (Math.abs(n) >= 1_000) return `$${(n / 1_000).toFixed(1)}K`;
  return `$${n.toFixed(0)}`;
}

function fmtPct(n: number): string {
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(3)}%`;
}

export function MarketContextCard({
  data,
  regimes,
}: MarketContextCardProps): ReactElement {
  const fundingClass = data.fundingPct >= 0 ? "text-gold" : "text-danger";
  return (
    <div className="p-4 flex flex-col gap-4">
      <header className="caps">Market Context · BTC</header>

      <div className="grid grid-cols-2 gap-3">
        <Stat
          label="Price"
          value={data.price.toLocaleString(undefined, { maximumFractionDigits: 2 })}
          large
        />
        <Stat
          label="Funding"
          value={fmtPct(data.fundingPct)}
          className={fundingClass}
        />
        <Stat
          label="Open Interest"
          value={fmtUsd(data.openInterestUsd)}
        />
        <Stat
          label="Liq 24h"
          value={fmtUsd(data.liq24hUsd)}
          className="text-amber"
        />
      </div>

      <div className="flex flex-wrap gap-1.5">
        {regimes.map((r) => (
          <span
            key={r.label}
            className="caps inline-flex items-center px-1.5 py-0.5 rounded border border-border-soft text-text-3"
          >
            {r.label} · {r.pct.toFixed(0)}%
          </span>
        ))}
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  large = false,
  className = "",
}: {
  label: string;
  value: string;
  large?: boolean;
  className?: string;
}): ReactElement {
  return (
    <div>
      <div className="caps">{label}</div>
      <div
        className={[
          large ? "text-[24px]" : "text-[16px]",
          "leading-tight tabular-nums text-text",
          className,
        ].join(" ")}
        style={
          large
            ? { fontFamily: 'Geist, sans-serif' }
            : { fontFamily: 'Geist Mono, ui-monospace, monospace' }
        }
      >
        {value}
      </div>
    </div>
  );
}
