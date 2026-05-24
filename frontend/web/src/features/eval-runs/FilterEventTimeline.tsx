// Filter v1 — per-bar timeline strip surfaced alongside the run's decision
// timeline. Each tick is one cadence-gated bar; color encodes:
//
//   - triggered (LLM dispatched)
//   - suppressed by `in_position`
//   - suppressed by `daily_cap`
//   - suppressed by `cooldown`
//   - not triggered (conditions evaluated to false; no dispatch, no suppression)
//
// Hover surfaces the bar timestamp and the indicator_snapshot values via the
// native `title` attribute — no popovers / tooltips (per the no-popups rule
// in CLAUDE.md; transient title text is a browser primitive, not a popup).
//
// Renders nothing when `events` is empty so the panel disappears for runs
// that produced no FilterEventV1 rows (EveryBar runs, runs that errored
// before reaching the filter loop).
//
// Spec: `docs/superpowers/specs/2026-05-21-filter-v1.md` §Acceptance #10.

import type { FC } from "react";

import type { FilterEventV1 } from "@/api/types.gen/FilterEventV1";
import type { SuppressedReason } from "@/api/types.gen/SuppressedReason";

type TickKind = "triggered" | "suppressed" | "idle";

interface TickClassification {
  kind: TickKind;
  reason: SuppressedReason | null;
}

function classify(event: FilterEventV1): TickClassification {
  if (event.triggered) return { kind: "triggered", reason: null };
  if (event.suppressed_reason !== null) {
    return { kind: "suppressed", reason: event.suppressed_reason };
  }
  return { kind: "idle", reason: null };
}

// Color tokens — distinct per acceptance criterion (each suppression
// reason renders with a *visually distinct* marker). Background colors
// pull from the existing palette so dark mode renders correctly without
// extra plumbing.
const TICK_CLASS: Record<string, string> = {
  triggered: "bg-gold",
  in_position: "bg-text-3",
  daily_cap: "bg-danger/70",
  cooldown: "bg-text-2",
  idle: "bg-surface-elev border border-border-soft",
};

function tickClassFor(c: TickClassification): string {
  if (c.kind === "triggered") return TICK_CLASS.triggered;
  if (c.kind === "suppressed" && c.reason) return TICK_CLASS[c.reason] ?? TICK_CLASS.idle;
  return TICK_CLASS.idle;
}

function tickAriaLabel(event: FilterEventV1, c: TickClassification): string {
  const ts = event.bar_timestamp;
  if (c.kind === "triggered") return `bar ${ts} — triggered`;
  if (c.kind === "suppressed") return `bar ${ts} — suppressed (${c.reason ?? "unknown"})`;
  return `bar ${ts} — not triggered`;
}

function tickTitle(event: FilterEventV1, c: TickClassification): string {
  const lines: string[] = [tickAriaLabel(event, c)];
  const snapshot = event.indicator_snapshot;
  const keys = Object.keys(snapshot).sort();
  if (keys.length > 0) {
    lines.push("");
    for (const k of keys) {
      const v = snapshot[k];
      lines.push(`${k} = ${formatIndicator(v)}`);
    }
  }
  return lines.join("\n");
}

function formatIndicator(v: number): string {
  if (!Number.isFinite(v)) return String(v);
  const abs = Math.abs(v);
  // Heuristic precision: price-magnitude values get 2 decimals, indicator
  // ratios / percentages get 4.
  const decimals = abs >= 100 ? 2 : 4;
  return v.toFixed(decimals);
}

interface LegendItem {
  kind: TickKind | SuppressedReason;
  label: string;
  className: string;
}

const LEGEND: LegendItem[] = [
  { kind: "triggered", label: "triggered", className: TICK_CLASS.triggered },
  { kind: "in_position", label: "in-position", className: TICK_CLASS.in_position },
  { kind: "daily_cap", label: "daily cap", className: TICK_CLASS.daily_cap },
  { kind: "cooldown", label: "cooldown", className: TICK_CLASS.cooldown },
  { kind: "idle", label: "not triggered", className: TICK_CLASS.idle },
];

export const FilterEventTimeline: FC<{
  events: FilterEventV1[];
  /** Optional title displayed above the strip. Omit for tightest layout. */
  title?: string;
}> = ({ events, title }) => {
  if (events.length === 0) return null;
  const first = events[0]?.bar_timestamp;
  const last = events[events.length - 1]?.bar_timestamp;

  return (
    <section
      data-testid="filter-event-timeline"
      className="rounded-card border border-border p-4 mt-4"
    >
      {title && (
        <h4 className="font-serif italic text-[14px] text-text mb-2">{title}</h4>
      )}

      {first && last && (
        <div
          data-testid="filter-event-timeline-range"
          className="mb-2 flex flex-wrap items-center justify-between gap-2 font-mono text-[11px] text-text-2"
        >
          <span>{formatTimelineStamp(first)}</span>
          <span>{formatTimelineStamp(last)}</span>
        </div>
      )}

      <div
        role="list"
        aria-label="filter event timeline"
        className="flex flex-wrap gap-[3px]"
      >
        {events.map((e, i) => {
          const c = classify(e);
          return (
            <div
              key={`${e.bar_timestamp}-${i}`}
              role="listitem"
              data-testid="filter-event-tick"
              data-kind={c.kind}
              data-reason={c.reason ?? ""}
              data-bar-timestamp={e.bar_timestamp}
              aria-label={tickAriaLabel(e, c)}
              title={tickTitle(e, c)}
              className={`h-3 w-2 rounded-[1px] ${tickClassFor(c)}`}
            />
          );
        })}
      </div>

      <ul
        data-testid="filter-event-timeline-legend"
        className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-text-3"
      >
        {LEGEND.map((item) => (
          <li
            key={item.kind}
            data-legend-kind={item.kind}
            className="flex items-center gap-1.5"
          >
            <span className={`inline-block h-2.5 w-2.5 rounded-[1px] ${item.className}`} />
            <span>{item.label}</span>
          </li>
        ))}
      </ul>
    </section>
  );
};

function formatTimelineStamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
