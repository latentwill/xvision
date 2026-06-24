// Filter v1 — per-bar timeline strip surfaced alongside the run's decision
// timeline. Each tick is one cadence-gated bar; color encodes:
//
//   - triggered (LLM dispatched)
//   - suppressed by `in_position`
//   - suppressed by `daily_cap`
//   - suppressed by `cooldown`
//   - not triggered (conditions evaluated to false; no dispatch, no suppression)
//
// Each tick is a focusable button. Hover/focus surfaces an inline preview
// strip above the ticks; click opens an inline detail panel beneath the
// strip with the full bar timestamp, kind/reason, conditions counts, and
// indicator snapshot. The native `title` attribute is retained as a no-JS
// and screen-reader fallback (transient browser primitive, not a popup).
//
// Renders nothing when `events` is empty so the panel disappears for runs
// that produced no FilterEventV1 rows (EveryBar runs, runs that errored
// before reaching the filter loop).
//
// Spec: `docs/superpowers/specs/2026-05-21-filter-v1.md` §Acceptance #10.

import { useState, type FC } from "react";

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

function summarizeKind(c: TickClassification): string {
  if (c.kind === "triggered") return "triggered";
  if (c.kind === "suppressed") return `suppressed (${c.reason ?? "unknown"})`;
  return "not triggered";
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

interface PreviewProps {
  event: FilterEventV1;
  classification: TickClassification;
}

const PreviewStrip: FC<PreviewProps> = ({ event, classification }) => (
  <div
    data-testid="filter-event-preview"
    data-bar-timestamp={event.bar_timestamp}
    className="mb-2 text-[12px] text-text-2 font-mono"
  >
    <span className="text-text">{event.bar_timestamp}</span>
    <span className="mx-2 text-text-3">·</span>
    <span>{summarizeKind(classification)}</span>
    <span className="mx-2 text-text-3">·</span>
    <span>
      {event.conditions_passed.length} passed · {event.conditions_failed.length} failed
    </span>
  </div>
);

const DetailPanel: FC<PreviewProps> = ({ event, classification }) => {
  const snapshot = event.indicator_snapshot;
  const keys = Object.keys(snapshot).sort();
  return (
    <div
      data-testid="filter-event-detail"
      data-bar-timestamp={event.bar_timestamp}
      className="mt-3 rounded-card border border-border-soft p-3 text-[12px] font-mono"
    >
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-text-2">
        <span className="text-text">{event.bar_timestamp}</span>
        <span>filter: {event.filter_id}</span>
        <span>{summarizeKind(classification)}</span>
        <span>
          {event.conditions_passed.length} passed · {event.conditions_failed.length} failed
        </span>
      </div>
      {keys.length > 0 && (
        <ul
          data-testid="filter-event-detail-indicators"
          className="mt-2 grid grid-cols-2 gap-x-4 gap-y-0.5 text-text-2"
        >
          {keys.map((k) => (
            <li key={k} data-indicator-key={k}>
              <span className="text-text-3">{k}</span> = {formatIndicator(snapshot[k])}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};

export const FilterEventTimeline: FC<{
  events: FilterEventV1[];
  /** Optional title displayed above the strip. Omit for tightest layout. */
  title?: string;
}> = ({ events, title }) => {
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  if (events.length === 0) return null;

  // Range endpoints used to render here as `formatTimelineStamp(first)` /
  // `(last)` in two corners of the strip — a "Feb 28, 07:00 PM" stamp in the
  // top-right read as a static page-level date rather than as the last bar.
  // The labels were also in the host's local timezone (toLocaleString) while
  // every other timestamp surface on this page is UTC, which compounded the
  // confusion. Each tick already carries the full bar ISO via the `title=`
  // tooltip and aria-label, so per-bar timestamps remain one hover away.
  // Intake 2026-05-28 §4 — keep the strip, drop the static corner stamps.

  const previewIndex = hoveredIndex ?? selectedIndex;
  const previewEvent = previewIndex !== null ? events[previewIndex] : null;
  const selectedEvent = selectedIndex !== null ? events[selectedIndex] : null;

  const toggleSelected = (i: number) =>
    setSelectedIndex((prev) => (prev === i ? null : i));

  return (
    <section
      data-testid="filter-event-timeline"
      className="rounded-card border border-border p-4 mt-4"
    >
      {title && (
        <h4 className="font-sans font-semibold text-[14px] text-text mb-2">{title}</h4>
      )}

      {previewEvent && (
        <PreviewStrip event={previewEvent} classification={classify(previewEvent)} />
      )}

      <div
        role="list"
        aria-label="filter event timeline"
        className="flex flex-wrap gap-[3px]"
      >
        {events.map((e, i) => {
          const c = classify(e);
          const isSelected = selectedIndex === i;
          return (
            <button
              key={`${e.bar_timestamp}-${i}`}
              type="button"
              role="listitem"
              data-testid="filter-event-tick"
              data-kind={c.kind}
              data-reason={c.reason ?? ""}
              data-bar-timestamp={e.bar_timestamp}
              data-selected={isSelected ? "true" : "false"}
              aria-label={tickAriaLabel(e, c)}
              aria-pressed={isSelected}
              title={tickTitle(e, c)}
              onClick={() => toggleSelected(i)}
              onMouseEnter={() => setHoveredIndex(i)}
              onMouseLeave={() => setHoveredIndex(null)}
              onFocus={() => setHoveredIndex(i)}
              onBlur={() => setHoveredIndex(null)}
              className={`h-3 w-2 rounded-[1px] focus:outline-none focus:ring-1 focus:ring-gold ${
                isSelected ? "ring-1 ring-gold" : ""
              } ${tickClassFor(c)}`}
            />
          );
        })}
      </div>

      {selectedEvent && (
        <DetailPanel event={selectedEvent} classification={classify(selectedEvent)} />
      )}

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
