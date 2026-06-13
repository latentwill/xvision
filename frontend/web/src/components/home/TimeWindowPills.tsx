// frontend/web/src/components/home/TimeWindowPills.tsx
//
// Inline, full-width time-window segmented row for the home dashboard
// (bead-008): Today | 7d | 30d | All. Scopes ONLY the outcomes + findings
// surfaces; the pulse hero / leaderboard / last-visit delta stay on the
// unscoped runs query (see routes/home.tsx).
//
// House style matches PulseViewSwitcher: a `role="group"` of plain
// `<button type="button" aria-pressed>` chips (keyboard-operable, no popups,
// no overlays). The active chip uses the gold token border/text; inactive
// chips use the muted `border-border-soft` / `text-text-3` tokens — never
// border-white / gray-100 / gray-200. font-medium uppercase tabular spacing
// matches the existing strip/chip vocabulary.
//
// The pure helper `sinceForWindow(window, now?)` turns a window into the
// RFC-3339 lower bound the backend's `?since=` contract expects (inclusive),
// or `undefined` for All (no filter — first paint unchanged).

import type { ReactElement } from "react";

export type TimeWindow = "today" | "7d" | "30d" | "all";

export interface TimeWindowDef {
  value: TimeWindow;
  label: string;
}

// Canonical render order: Today | 7d | 30d | All. `All` is last and the
// default, so first paint reproduces today's unscoped behavior.
export const TIME_WINDOWS: readonly TimeWindowDef[] = [
  { value: "today", label: "Today" },
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "all", label: "All" },
] as const;

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * Pure helper: the RFC-3339 inclusive lower bound for a window.
 *
 *   - "all"   → undefined (no `since` filter; first-paint behavior).
 *   - "today" → start of LOCAL today (local midnight, as an ISO/UTC string).
 *   - "7d"    → now − 7·24h.
 *   - "30d"   → now − 30·24h.
 *
 * `now` is injectable for deterministic tests; defaults to the real clock.
 * The returned string is `Date.prototype.toISOString()` output — an
 * ISO-8601 / RFC-3339 UTC instant the backend's chrono parser accepts.
 */
export function sinceForWindow(
  window: TimeWindow,
  now: Date = new Date(),
): string | undefined {
  switch (window) {
    case "all":
      return undefined;
    case "today": {
      // Local midnight — the operator's "today", not UTC's.
      const midnight = new Date(
        now.getFullYear(),
        now.getMonth(),
        now.getDate(),
        0,
        0,
        0,
        0,
      );
      return midnight.toISOString();
    }
    case "7d":
      return new Date(now.getTime() - 7 * DAY_MS).toISOString();
    case "30d":
      return new Date(now.getTime() - 30 * DAY_MS).toISOString();
  }
}

export interface TimeWindowPillsProps {
  value: TimeWindow;
  onChange: (window: TimeWindow) => void;
}

export function TimeWindowPills({
  value,
  onChange,
}: TimeWindowPillsProps): ReactElement {
  return (
    <div
      data-testid="time-window-pills"
      role="group"
      aria-label="Time window"
      className="flex flex-wrap items-center gap-1.5"
    >
      {TIME_WINDOWS.map((w) => {
        const active = value === w.value;
        return (
          <button
            key={w.value}
            type="button"
            aria-pressed={active}
            onClick={() => {
              // No-op re-selecting the active window — avoids a pointless
              // state churn / refetch.
              if (!active) onChange(w.value);
            }}
            className={`rounded-sm border px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide tabular-nums transition-colors ${
              active
                ? "border-gold/40 text-gold"
                : "border-border-soft text-text-3 hover:text-text"
            }`}
          >
            {w.label}
          </button>
        );
      })}
    </div>
  );
}
