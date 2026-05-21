// Filter v1 — read-only summary panel surfaced on the eval-run detail.
//
// Renders one block per `FilterSummary` produced by a FilterGated run:
// bars scanned, wake-ups, suppression breakdown, LLM calls saved,
// estimated tokens saved. Inline (no popups), per the dashboard's
// no-popups rule. Returns `null` when the run has no filter summaries so
// EveryBar runs render nothing.
//
// Spec: `docs/superpowers/specs/2026-05-21-filter-v1.md` §Acceptance #10.

import type { FC } from "react";

import type { FilterSummary } from "@/api/types.gen/FilterSummary";

export const FilterSummaryPanel: FC<{ summaries: FilterSummary[] }> = ({
  summaries,
}) => {
  if (summaries.length === 0) return null;

  return (
    <section
      data-testid="filter-summary-panel"
      className="rounded-card border border-border p-4 mt-6"
    >
      <h3 className="font-serif italic text-[16px] text-text mb-3">
        Filters
        <span className="text-text-3 text-[12px] not-italic font-sans ml-2">
          ({summaries.length})
        </span>
      </h3>
      <ul className="space-y-4">
        {summaries.map((s) => (
          <FilterSummaryRow key={s.filter_id} summary={s} />
        ))}
      </ul>
    </section>
  );
};

function FilterSummaryRow({ summary: s }: { summary: FilterSummary }) {
  const suppressedTotal =
    s.suppressed_in_position + s.suppressed_daily_cap + s.suppressed_cooldown;
  const wakeRatePct =
    s.bars_scanned > 0 ? (s.wakeups / s.bars_scanned) * 100 : 0;

  return (
    <li
      data-testid="filter-summary-row"
      data-filter-id={s.filter_id}
      className="text-[13px]"
    >
      <div className="flex items-baseline justify-between gap-3 mb-2">
        <code className="font-mono text-text-2 text-[12px] truncate">
          {s.filter_id}
        </code>
        <span className="text-text-3 text-[11px]">
          {fmtCount(s.wakeups)} / {fmtCount(s.bars_scanned)} bars woke ·{" "}
          {wakeRatePct.toFixed(1)}%
        </span>
      </div>

      <div className="grid grid-cols-3 gap-x-6 gap-y-2">
        <Metric label="bars scanned" value={fmtCount(s.bars_scanned)} />
        <Metric label="wake-ups" value={fmtCount(s.wakeups)} tone="pos" />
        <Metric
          label="suppressed"
          value={fmtCount(suppressedTotal)}
          tone={suppressedTotal > 0 ? "muted" : "neutral"}
        />
        <Metric
          label="in-position"
          value={fmtCount(s.suppressed_in_position)}
          tone="muted"
        />
        <Metric
          label="daily cap"
          value={fmtCount(s.suppressed_daily_cap)}
          tone="muted"
        />
        <Metric
          label="cooldown"
          value={fmtCount(s.suppressed_cooldown)}
          tone="muted"
        />
        <Metric
          label="LLM calls saved"
          value={fmtCount(s.llm_calls_saved)}
          tone="pos"
        />
        <Metric
          label="est. tokens saved"
          value={fmtCount(s.estimated_tokens_saved)}
          tone="pos"
          titleValue={`${s.estimated_tokens_saved.toLocaleString()} tokens (≈ llm_calls_saved × 50,000)`}
        />
        <div />
      </div>
    </li>
  );
}

function Metric({
  label,
  value,
  tone = "neutral",
  titleValue,
}: {
  label: string;
  value: string;
  tone?: "pos" | "neutral" | "muted";
  titleValue?: string;
}) {
  const valueClass =
    tone === "pos"
      ? "text-gold"
      : tone === "muted"
        ? "text-text-2"
        : "text-text";
  return (
    <div>
      <div className="text-text-3 text-[11px] uppercase tracking-wide mb-1">
        {label}
      </div>
      <div className={`font-mono text-[13px] ${valueClass}`} title={titleValue}>
        {value}
      </div>
    </div>
  );
}

function fmtCount(n: number): string {
  return n.toLocaleString("en-US");
}
