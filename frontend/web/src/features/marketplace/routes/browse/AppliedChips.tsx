// src/features/marketplace/routes/browse/AppliedChips.tsx
import { RemovableChip } from "@/features/marketplace/components/RemovableChip";
import { defaultFilterState } from "@/features/marketplace/data/filter";
import type { FilterState } from "@/features/marketplace/data/types";

interface AppliedChipsProps {
  filter: FilterState;
  setFilter: (patch: Partial<FilterState>) => void;
  matchCount: number;
}

function hasActiveFilters(f: FilterState): boolean {
  return (
    f.assets.length > 0 ||
    f.models.length > 0 ||
    f.styles.length > 0 ||
    f.tier.length > 0 ||
    f.trust.verifiedOnly ||
    f.trust.acceptsAgents ||
    f.trust.auditedOnly ||
    f.minBuyers > 0 ||
    f.priceUsdc.from !== 0 ||
    f.priceUsdc.to !== 500
  );
}

export function AppliedChips({ filter, setFilter, matchCount }: AppliedChipsProps) {
  if (!hasActiveFilters(filter)) return null;

  const chips: { label: string; onRemove: () => void }[] = [];

  for (const asset of filter.assets) {
    chips.push({
      label: `Asset: ${asset}`,
      onRemove: () => setFilter({ assets: filter.assets.filter((a) => a !== asset) }),
    });
  }
  for (const model of filter.models) {
    chips.push({
      label: `Model: ${model}`,
      onRemove: () => setFilter({ models: filter.models.filter((m) => m !== model) }),
    });
  }
  for (const style of filter.styles) {
    chips.push({
      label: `Style: ${style}`,
      onRemove: () => setFilter({ styles: filter.styles.filter((s) => s !== style) }),
    });
  }
  for (const t of filter.tier) {
    chips.push({
      label: `Tier: ${t}`,
      onRemove: () => setFilter({ tier: filter.tier.filter((x) => x !== t) }),
    });
  }
  if (filter.trust.verifiedOnly) {
    chips.push({
      label: "Verified only",
      onRemove: () => setFilter({ trust: { ...filter.trust, verifiedOnly: false } }),
    });
  }
  if (filter.trust.acceptsAgents) {
    chips.push({
      label: "Accepts agents",
      onRemove: () => setFilter({ trust: { ...filter.trust, acceptsAgents: false } }),
    });
  }
  if (filter.trust.auditedOnly) {
    chips.push({
      label: "Audited only",
      onRemove: () => setFilter({ trust: { ...filter.trust, auditedOnly: false } }),
    });
  }
  if (filter.minBuyers > 0) {
    chips.push({
      label: `Min buyers: ${filter.minBuyers}`,
      onRemove: () => setFilter({ minBuyers: 0 }),
    });
  }
  if (filter.priceUsdc.from !== 0 || filter.priceUsdc.to !== 500) {
    chips.push({
      label: `Price: ${filter.priceUsdc.from}–${filter.priceUsdc.to} USDC`,
      onRemove: () => setFilter({ priceUsdc: { from: 0, to: 500 } }),
    });
  }

  function clearAll() {
    const def = defaultFilterState();
    setFilter({
      assets: def.assets,
      models: def.models,
      styles: def.styles,
      tier: def.tier,
      trust: def.trust,
      minBuyers: def.minBuyers,
      priceUsdc: def.priceUsdc,
    });
  }

  return (
    <div
      data-applied-chips
      className="px-7 pb-3 pt-1 flex items-center gap-1.5 flex-wrap"
    >
      <span className="font-mono text-[9px] tracking-[0.2em] uppercase text-text-3">
        APPLIED
      </span>
      {chips.map((c) => (
        <RemovableChip key={c.label} onRemove={c.onRemove}>
          {c.label}
        </RemovableChip>
      ))}
      <button
        type="button"
        aria-label="clear all"
        onClick={clearAll}
        className="text-text-3 text-[11.5px] ml-1 cursor-pointer underline decoration-dotted underline-offset-[3px] hover:text-text bg-transparent border-none p-0"
      >
        Clear all
      </button>
      <span className="ml-auto font-mono text-[11px] text-text-3">
        <span className="text-text-2">{matchCount.toLocaleString()}</span> matches
      </span>
    </div>
  );
}
