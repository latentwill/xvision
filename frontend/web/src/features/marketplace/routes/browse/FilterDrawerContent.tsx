// src/features/marketplace/routes/browse/FilterDrawerContent.tsx
// Content rendered inside the F0 FilterDrawer shell.
// Sections: Sort · Assets · Models · Style · Trust · Price · Min buyers.
// TODO(Phase 1): auditedOnly filter has no ListingRow field yet — toggle renders but is a no-op.
import type { FilterState, SortKey, Tier } from "@/features/marketplace/data/types";
import { defaultFilterState } from "@/features/marketplace/data/filter";

// ── Constants ────────────────────────────────────────────────────────────────

const SORT_OPTIONS: { key: SortKey; label: string }[] = [
  { key: "return30d", label: "30d return" },
  { key: "sharpe", label: "Sharpe" },
  { key: "buyers", label: "Buyers (humans + agents)" },
  { key: "newest", label: "Newest" },
];

const ASSET_GROUPS: { group: string; items: { sym: string; name: string }[] }[] = [
  {
    group: "Crypto · majors",
    items: [
      { sym: "BTC", name: "Bitcoin" },
      { sym: "ETH", name: "Ethereum" },
      { sym: "SOL", name: "Solana" },
      { sym: "MATIC", name: "Polygon" },
      { sym: "AVAX", name: "Avalanche" },
    ],
  },
  {
    group: "Crypto · L2 & memes",
    items: [
      { sym: "ARB", name: "Arbitrum" },
      { sym: "OP", name: "Optimism" },
      { sym: "BASE", name: "Base" },
      { sym: "MNT", name: "Mantle" },
      { sym: "DOGE", name: "Dogecoin" },
      { sym: "WIF", name: "dogwifhat" },
      { sym: "PEPE", name: "Pepe" },
    ],
  },
  {
    group: "Equities",
    items: [
      { sym: "SPY", name: "S&P 500 ETF" },
      { sym: "QQQ", name: "Nasdaq-100" },
      { sym: "NVDA", name: "NVIDIA" },
      { sym: "TSLA", name: "Tesla" },
    ],
  },
  {
    group: "FX",
    items: [
      { sym: "EUR/USD", name: "Euro / USD" },
      { sym: "USD/JPY", name: "USD / Yen" },
    ],
  },
];

const MODEL_OPTIONS = [
  "Claude · Haiku 4.5",
  "Claude · Sonnet 4.5",
  "GPT-5",
  "Gemini 3 Pro",
  "Llama 4",
];

const STYLE_OPTIONS = ["Long", "Long/Short", "Day", "Swing", "Mean-reversion", "Momentum"];

// ── Section wrapper ───────────────────────────────────────────────────────────

function DrawerSection({
  title,
  sub,
  children,
}: {
  title: string;
  sub?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="px-[18px] py-3.5 border-b border-border">
      <div className="flex items-baseline justify-between mb-2.5">
        <span className="text-[12.5px] font-semibold text-text">{title}</span>
        {sub && <span className="font-mono text-[10px] text-text-3">{sub}</span>}
      </div>
      {children}
    </div>
  );
}

// ── CheckRow helper ───────────────────────────────────────────────────────────

function CheckRow({
  id,
  label,
  sub,
  checked,
  onToggle,
}: {
  id: string;
  label: string;
  sub?: string;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <label
      htmlFor={id}
      className={[
        "grid items-center gap-2.5 px-1.5 py-1 rounded-[3px] cursor-pointer",
        checked ? "bg-gold/10" : "hover:bg-surface-elev",
      ].join(" ")}
      style={{ gridTemplateColumns: "18px 1fr auto" }}
    >
      <input
        type="checkbox"
        id={id}
        aria-label={label}
        checked={checked}
        onChange={onToggle}
        className="sr-only"
      />
      <span
        className={[
          "w-[13px] h-[13px] rounded-[2px] border flex items-center justify-center shrink-0",
          checked ? "bg-gold border-gold" : "bg-transparent border-border-strong",
        ].join(" ")}
        aria-hidden="true"
      >
        {checked && (
          <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="var(--on-accent)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M1.5 4.5L4 7l4-5" />
          </svg>
        )}
      </span>
      <span className={`text-[12px] ${checked ? "text-gold" : "text-text"}`}>{label}</span>
      {sub && <span className="font-mono text-[10.5px] text-text-3">{sub}</span>}
    </label>
  );
}

// ── Toggle switch ─────────────────────────────────────────────────────────────

function ToggleSwitch({
  label,
  subtitle,
  checked,
  onToggle,
}: {
  label: string;
  subtitle: string;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <div
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={onToggle}
      onKeyDown={(e) => e.key === "Enter" && onToggle()}
      tabIndex={0}
      className="flex items-center gap-2.5 py-1 cursor-pointer"
    >
      <span
        className={[
          "w-[30px] h-[17px] rounded-full relative shrink-0 transition-colors",
          checked ? "bg-gold" : "bg-border-strong",
        ].join(" ")}
      >
        <span
          className={[
            "absolute top-0.5 w-[13px] h-[13px] rounded-full bg-[var(--on-accent)] transition-[left]",
            checked ? "left-[15px]" : "left-0.5",
          ].join(" ")}
        />
      </span>
      <div>
        <div className="text-[12px] text-text">{label}</div>
        <div className="font-mono text-[10px] text-text-3 mt-0.5">{subtitle}</div>
      </div>
    </div>
  );
}

// ── Range visual ─────────────────────────────────────────────────────────────
// Renders number inputs for USDC range and min-buyers for test-friendliness.

function PriceRange({
  from,
  to,
  onChange,
}: {
  from: number;
  to: number;
  onChange: (from: number, to: number) => void;
}) {
  const fromPct = (from / 500) * 100;
  const toPct = (to / 500) * 100;
  return (
    <div>
      <div className="relative h-[30px] py-[10px]">
        <div className="absolute left-0 right-0 top-[14px] h-[3px] rounded bg-border-strong" />
        <div
          className="absolute top-[14px] h-[3px] rounded bg-gold"
          style={{ left: `${fromPct}%`, right: `${100 - toPct}%` }}
        />
      </div>
      <div className="flex gap-2 mt-1">
        <input
          type="number"
          aria-label="price from"
          min={0}
          max={to}
          value={from}
          onChange={(e) => onChange(Number(e.target.value), to)}
          className="w-full bg-surface-elev border border-border-strong rounded px-2 py-1 font-mono text-[11px] text-text-2 outline-none focus:border-gold/60"
        />
        <input
          type="number"
          aria-label="price to"
          min={from}
          max={500}
          value={to}
          onChange={(e) => onChange(from, Number(e.target.value))}
          className="w-full bg-surface-elev border border-border-strong rounded px-2 py-1 font-mono text-[11px] text-text-2 outline-none focus:border-gold/60"
        />
      </div>
    </div>
  );
}

function MinBuyersRange({
  value,
  onChange,
}: {
  value: number;
  onChange: (v: number) => void;
}) {
  const pct = Math.min((value / 500) * 100, 100);
  return (
    <div>
      <div className="relative h-[30px] py-[10px]">
        <div className="absolute left-0 right-0 top-[14px] h-[3px] rounded bg-border-strong" />
        <div
          className="absolute left-0 top-[14px] h-[3px] rounded bg-gold"
          style={{ right: `${100 - pct}%` }}
        />
      </div>
      <div className="flex justify-between mt-1 font-mono text-[11px] text-text-2">
        <span>min {value}</span>
        <span className="text-text-3">unlimited</span>
      </div>
      <input
        type="range"
        aria-label="minimum buyers"
        min={0}
        max={500}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-[var(--gold,#00E676)] mt-1"
      />
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface FilterDrawerContentProps {
  filter: FilterState;
  setFilter: (patch: Partial<FilterState>) => void;
  matchCount: number;
  /** Total listings in the marketplace (drives the "of N match" line, QA1). */
  totalCount: number;
  onClose: () => void;
}

export function FilterDrawerContent({
  filter,
  setFilter,
  matchCount,
  totalCount,
  onClose,
}: FilterDrawerContentProps) {
  const def = defaultFilterState();

  function clearAll() {
    setFilter({
      assets: def.assets,
      models: def.models,
      styles: def.styles,
      tier: def.tier,
      trust: def.trust,
      minBuyers: def.minBuyers,
      priceUsdc: def.priceUsdc,
      sort: def.sort,
    });
  }

  function toggleAsset(sym: string) {
    const next = filter.assets.includes(sym)
      ? filter.assets.filter((a) => a !== sym)
      : [...filter.assets, sym];
    setFilter({ assets: next });
  }

  function toggleModel(m: string) {
    const next = filter.models.includes(m)
      ? filter.models.filter((x) => x !== m)
      : [...filter.models, m];
    setFilter({ models: next });
  }

  function toggleStyle(s: string) {
    const next = filter.styles.includes(s)
      ? filter.styles.filter((x) => x !== s)
      : [...filter.styles, s];
    setFilter({ styles: next });
  }

  function toggleTier(t: Tier) {
    const next = filter.tier.includes(t)
      ? filter.tier.filter((x) => x !== t)
      : [...filter.tier, t];
    setFilter({ tier: next });
  }

  return (
    // Cap the accordion body at ~60vh with internal scroll so the sticky
    // footer (Clear all · matches · Done) is always reachable without scrolling
    // the whole page (QA fix: Done button was ~790px below the fold at 1440×900).
    <div className="flex flex-col max-h-[60vh]">
      <div className="overflow-y-auto min-h-0 flex-1">
      {/* Header meta line */}
      <div className="px-[18px] pb-2 pt-1 font-mono text-[10.5px] text-text-3">
        {filter.assets.length + filter.models.length + filter.styles.length +
          filter.tier.length +
          (filter.trust.verifiedOnly ? 1 : 0) +
          (filter.trust.acceptsAgents ? 1 : 0) +
          (filter.trust.auditedOnly ? 1 : 0) > 0 ? (
          <>
            <span className="text-gold">
              {filter.assets.length + filter.models.length + filter.styles.length +
                filter.tier.length +
                (filter.trust.verifiedOnly ? 1 : 0) +
                (filter.trust.acceptsAgents ? 1 : 0) +
                (filter.trust.auditedOnly ? 1 : 0)}{" "}
              filters active
            </span>{" "}
            · {matchCount.toLocaleString()} of {totalCount.toLocaleString()} match
          </>
        ) : (
          <span>{matchCount.toLocaleString()} strategies</span>
        )}
      </div>

      {/* Sort by */}
      <DrawerSection title="Sort by">
        <div className="flex flex-col gap-1.5">
          {SORT_OPTIONS.map((o) => (
            <label
              key={o.key}
              className="flex items-center gap-2.5 px-1 py-1.5 cursor-pointer"
            >
              <input
                type="radio"
                name="sort"
                aria-label={o.label}
                checked={filter.sort === o.key}
                onChange={() => setFilter({ sort: o.key })}
                className="sr-only"
              />
              <span
                className={[
                  "w-[13px] h-[13px] rounded-full border flex items-center justify-center shrink-0",
                  filter.sort === o.key
                    ? "border-gold bg-gold/10"
                    : "border-border-strong bg-transparent",
                ].join(" ")}
                aria-hidden="true"
              >
                {filter.sort === o.key && (
                  <span className="w-[5px] h-[5px] rounded-full bg-gold" />
                )}
              </span>
              <span
                className={`text-[12.5px] ${filter.sort === o.key ? "text-text" : "text-text-2"}`}
              >
                {o.label}
              </span>
            </label>
          ))}
        </div>
      </DrawerSection>

      {/* Assets */}
      <DrawerSection
        title="Assets"
        sub={filter.assets.length > 0 ? `${filter.assets.length} selected` : undefined}
      >
        <div className="flex items-center gap-2 px-2 py-1.5 mb-2 border border-border-strong rounded-[3px] bg-surface-elev">
          <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-text-3 shrink-0" aria-hidden="true">
            <circle cx="6" cy="6" r="4" />
            <path d="M9.5 9.5l2.5 2.5" strokeLinecap="round" />
          </svg>
          <span className="font-mono text-[11px] text-text-3">filter assets…</span>
        </div>
        {ASSET_GROUPS.map((g, gi) => (
          <div key={g.group} className={gi < ASSET_GROUPS.length - 1 ? "mb-1.5" : ""}>
            <div className="flex items-baseline justify-between py-1.5">
              <span className="font-mono text-[9px] tracking-[0.18em] uppercase text-text-3">
                {g.group}
              </span>
              <span className="font-mono text-[9.5px] text-text-4">{g.items.length}</span>
            </div>
            {g.items.map((a) => (
              <CheckRow
                key={a.sym}
                id={`asset-${a.sym}`}
                label={a.sym}
                sub={a.name}
                checked={filter.assets.includes(a.sym)}
                onToggle={() => toggleAsset(a.sym)}
              />
            ))}
          </div>
        ))}
      </DrawerSection>

      {/* Models */}
      <DrawerSection
        title="Models"
        sub={filter.models.length > 0 ? `${filter.models.length} selected` : undefined}
      >
        {MODEL_OPTIONS.map((m) => (
          <CheckRow
            key={m}
            id={`model-${m}`}
            label={m}
            checked={filter.models.includes(m)}
            onToggle={() => toggleModel(m)}
          />
        ))}
      </DrawerSection>

      {/* Style chips */}
      <DrawerSection title="Style">
        <div className="flex gap-1.5 flex-wrap">
          {STYLE_OPTIONS.map((s) => {
            const active = filter.styles.includes(s);
            return (
              <button
                key={s}
                type="button"
                onClick={() => toggleStyle(s)}
                className={[
                  "px-2 py-1 rounded-[3px] border font-mono text-[10.5px] cursor-pointer",
                  active
                    ? "border-gold/30 bg-gold/10 text-gold"
                    : "border-border-strong bg-transparent text-text-2 hover:border-border",
                ].join(" ")}
              >
                {s}
              </button>
            );
          })}
        </div>
      </DrawerSection>

      {/* Tier */}
      <DrawerSection
        title="Tier"
        sub={filter.tier.length > 0 ? `${filter.tier.length} selected` : undefined}
      >
        <div className="flex gap-1.5 flex-wrap">
          {(["open", "sealed"] as Tier[]).map((t) => {
            const active = filter.tier.includes(t);
            return (
              <button
                key={t}
                type="button"
                onClick={() => toggleTier(t)}
                className={[
                  "px-2 py-1 rounded-[3px] border font-mono text-[10.5px] cursor-pointer capitalize",
                  active
                    ? "border-gold/30 bg-gold/10 text-gold"
                    : "border-border-strong bg-transparent text-text-2 hover:border-border",
                ].join(" ")}
              >
                {t === "open" ? "Open (free)" : "Sealed (paid)"}
              </button>
            );
          })}
        </div>
      </DrawerSection>

      {/* Trust toggles */}
      <DrawerSection title="Trust">
        <div className="flex flex-col gap-2">
          <ToggleSwitch
            label="Verified only"
            subtitle="green-check strategies"
            checked={filter.trust.verifiedOnly}
            onToggle={() =>
              setFilter({ trust: { ...filter.trust, verifiedOnly: !filter.trust.verifiedOnly } })
            }
          />
          <ToggleSwitch
            label="Accepts agents (x402)"
            subtitle="agent-paid purchase"
            checked={filter.trust.acceptsAgents}
            onToggle={() =>
              setFilter({ trust: { ...filter.trust, acceptsAgents: !filter.trust.acceptsAgents } })
            }
          />
          <ToggleSwitch
            label="Audited only"
            subtitle="creator audit attestation"
            // TODO(Phase 1): auditedOnly has no ListingRow field yet — toggle is visual-only
            checked={filter.trust.auditedOnly}
            onToggle={() =>
              setFilter({ trust: { ...filter.trust, auditedOnly: !filter.trust.auditedOnly } })
            }
          />
        </div>
      </DrawerSection>

      {/* Price range */}
      <DrawerSection title="Price (USDC)">
        <PriceRange
          from={filter.priceUsdc.from}
          to={filter.priceUsdc.to}
          onChange={(from, to) => setFilter({ priceUsdc: { from, to } })}
        />
      </DrawerSection>

      {/* Min buyers */}
      <DrawerSection title="Minimum buyers">
        <MinBuyersRange
          value={filter.minBuyers}
          onChange={(v) => setFilter({ minBuyers: v })}
        />
      </DrawerSection>
      </div>

      {/* Footer — pinned at the bottom of the capped accordion (always reachable). */}
      <div className="shrink-0 px-4 py-3 border-t border-border bg-bg flex items-center gap-2">
        <button
          type="button"
          onClick={clearAll}
          className="text-[11.5px] text-text-3 bg-transparent border-none cursor-pointer underline decoration-dotted underline-offset-[3px] p-0 hover:text-text"
        >
          Clear all
        </button>
        <span className="ml-auto font-mono text-[11px] text-text-3">
          <span className="text-text-2">{matchCount.toLocaleString()}</span> matches
        </span>
        <button
          type="button"
          aria-label="done"
          onClick={onClose}
          className="px-4 py-1.5 rounded bg-gold text-bg text-[12px] font-medium transition-colors hover:bg-gold-soft motion-safe:active:scale-[0.96]"
        >
          Done
        </button>
      </div>
    </div>
  );
}
