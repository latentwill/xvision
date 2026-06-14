// frontend/web/src/components/home/HomeDeltaSubtitle.tsx
//
// "Since you were last here" header delta (Control Tower bead xvision-jlm).
//
// Renders as the Topbar `sub` slot (already `text-sm text-text-2`):
//   "3 runs / 2 findings since you were last here · 5h ago"
// and a neutral welcome line on a first visit.
//
// Honesty mandate (spec §8.1/§8.9): runs/findings are EVAL facts the page
// already fetched — this surface NEVER renders live-money / P&L / capital /
// budget phrasing. No dollar values, no "real money", no "live strategies".

import type { SinceDelta } from "@/features/home/last-visit";

function plural(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

export interface HomeDeltaSubtitleProps {
  delta: SinceDelta;
}

export function HomeDeltaSubtitle({ delta }: HomeDeltaSubtitleProps) {
  // First visit: no prior boundary → a neutral, honest welcome line rather
  // than a "0 runs since…" non-event.
  if (delta.firstVisit) {
    return (
      <span data-testid="home-delta-subtitle">
        Welcome — your eval activity at a glance.
      </span>
    );
  }

  const { runsSince, findingsSince, hoursAgo } = delta;

  return (
    <span data-testid="home-delta-subtitle">
      <span className="font-mono tabular-nums text-text">
        {plural(runsSince, "run")}
      </span>{" "}
      /{" "}
      <span className="font-mono tabular-nums text-text">
        {plural(findingsSince, "finding")}
      </span>{" "}
      since you were last here
      {hoursAgo !== null ? (
        <span className="text-text-4">
          {" "}
          ·{" "}
          {hoursAgo === 0 ? (
            <span>just now</span>
          ) : (
            <span className="font-mono tabular-nums">{hoursAgo}h ago</span>
          )}
        </span>
      ) : null}
    </span>
  );
}
