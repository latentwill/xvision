// frontend/web/src/components/home/CostRollupStrip.tsx
//
// Cost rollup strip (bead-8wn). A slim home strip showing spend SINCE LAST
// VISIT and THIS WEEK against the operator-set daily budget cap. It is
// optimizer-adjacent (cost is dominated by optimizer cycles + eval runs), so
// the route mounts it next to the OptimizerPanel.
//
// HONESTY MANDATE (§8.1/§8.9): every monetary figure is REAL cost data only.
//   - A `null` spend (the backend's "no priced call" / unknown) renders an
//     em-dash + "no cost data", NEVER a fabricated "$0.00".
//   - The cap is operator-set; when UNSET the backend returns `null` and we
//     render an em-dash denominator ("$spend / —"), NEVER a faked ceiling.
//
// WCAG 1.4.1 (use of color): the approaching/over-cap state is conveyed by a
// glyph + word ("near" / "over"), not color alone, and surfaced on
// `data-cap-state` for the styling contract.
//
// NO POPUP: inline, full-width strip — no dialog/sheet/overlay. House style
// matches the home strips: font-mono tabular-nums numerals, token colors,
// designed (honest) empty state.

import { Card } from "@/components/primitives/Card";
import type { CostRollupResponse } from "@/api/cost";

export interface CostRollupStripProps {
  /// Rollup over the since-last-visit window. `null` while loading. The window
  /// itself is the LAST_VISIT_LS boundary (shared with the home delta).
  sinceLastVisit: CostRollupResponse | null;
  /// Rollup over the trailing 7d ("this week"). `null` while loading.
  thisWeek: CostRollupResponse | null;
  /// The persisted operator-set daily cap (null = UNSET → em-dash denominator).
  dailyCapUsd: number | null;
  /// True when there is no prior last-visit boundary (first visit). The since
  /// window then shows an honest "first visit" label rather than a number.
  firstVisit?: boolean;
}

/// Cap-state classification for one window's spend against the cap.
///   - "none": no cap set (never alarm),
///   - "ok":   well under the cap,
///   - "near": within 10% of the cap (approaching),
///   - "over": at/over the cap.
/// Exported for unit reasoning; the threshold (90%) is the "approaching" line.
export type CapState = "none" | "ok" | "near" | "over";

export function capState(
  spendUsd: number | null,
  capUsd: number | null,
): CapState {
  if (capUsd == null || !Number.isFinite(capUsd) || capUsd <= 0) return "none";
  if (spendUsd == null || !Number.isFinite(spendUsd)) return "none";
  if (spendUsd >= capUsd) return "over";
  if (spendUsd >= capUsd * 0.9) return "near";
  return "ok";
}

/// Format a USD figure to 2dp, or an em-dash when the source is null/unknown
/// (honesty: never a fabricated "$0.00").
function fmtUsd(v: number | null | undefined): string {
  return v != null && Number.isFinite(v) ? `$${v.toFixed(2)}` : "—";
}

/// Whole days a window spans, from its RFC-3339 `since` to now (min 1). Used to
/// scale the daily cap to the window for an honest cumulative-vs-budget compare.
function windowDaysFrom(sinceIso: string | null | undefined): number {
  if (!sinceIso) return 1;
  const t = Date.parse(sinceIso);
  if (!Number.isFinite(t)) return 1;
  const days = Math.ceil((Date.now() - t) / 86_400_000);
  return Number.isFinite(days) && days > 0 ? days : 1;
}

// Saturated token tints; all meet ≥4.5:1 on the strip surface in both themes.
const CAP_TONE: Record<CapState, string> = {
  none: "text-text-3",
  ok: "text-text-2",
  near: "text-warn",
  over: "text-danger",
};

// Non-color cue (glyph + word) so the cap-state is legible to colour-blind
// operators and in monochrome (WCAG 1.4.1). `ok`/`none` carry no alarm word.
const CAP_GLYPH: Record<CapState, string> = {
  none: "",
  ok: "",
  near: "▲ near",
  over: "■ over",
};

function WindowSegment({
  testid,
  label,
  spendUsd,
  capUsd,
  windowDays = 1,
  emptyLabel,
}: {
  testid: string;
  label: string;
  spendUsd: number | null;
  capUsd: number | null;
  /// Days this window spans, so the DAILY cap is scaled to a like-for-like
  /// budget (cumulative window spend vs cap × days). Defaults to 1.
  windowDays?: number;
  /// Override copy when there is no usable spend datum (first visit / unknown).
  emptyLabel?: string;
}) {
  // Scale the per-day cap to the window so a week of spend is compared to a
  // week's budget, not a single day's (bead xvision-s78.3). null stays null.
  const days = Number.isFinite(windowDays) && windowDays > 0 ? windowDays : 1;
  const windowCap =
    capUsd != null && Number.isFinite(capUsd) ? capUsd * days : capUsd;
  const state = capState(spendUsd, windowCap);
  const cue = CAP_GLYPH[state];

  // First-visit / unknown-spend windows show their honest empty copy and no
  // alarm. `data-cap-state` still reflects the (non-alarming) state.
  const body = emptyLabel ? (
    <span className="text-text-3">{emptyLabel}</span>
  ) : (
    <>
      <span className="font-mono tabular-nums">{fmtUsd(spendUsd)}</span>
      <span aria-hidden="true"> / </span>
      <span className="font-mono tabular-nums">{fmtUsd(windowCap)}</span>
      {cue && (
        <span className={`ml-1.5 text-[11px] font-medium ${CAP_TONE[state]}`}>
          {cue}
        </span>
      )}
    </>
  );

  return (
    <span
      data-testid={testid}
      data-cap-state={state}
      className="inline-flex items-baseline gap-1"
    >
      <span className="text-text-3">{label}</span>{" "}
      <span className={emptyLabel ? undefined : CAP_TONE[state]}>{body}</span>
    </span>
  );
}

export function CostRollupStrip({
  sinceLastVisit,
  thisWeek,
  dailyCapUsd,
  firstVisit = false,
}: CostRollupStripProps) {
  // Nothing fetched yet on EITHER window → render nothing (the band shrinks
  // instead of stacking an empty placeholder), matching the home page's "say
  // nothing when you have nothing to say" contract.
  if (sinceLastVisit == null && thisWeek == null) return null;

  // Honest empty copy for null spend (no priced call yet) vs a first visit.
  const sinceSpend = sinceLastVisit?.spend_usd ?? null;
  const sinceEmpty = firstVisit
    ? "first visit"
    : sinceLastVisit == null
      ? "…"
      : sinceSpend == null
        ? "no cost data"
        : undefined;

  const weekSpend = thisWeek?.spend_usd ?? null;
  const weekEmpty =
    thisWeek == null ? "…" : weekSpend == null ? "no cost data" : undefined;

  return (
    <Card className="p-0 overflow-hidden xvn-card-hover">
      <section
        data-testid="cost-rollup-strip"
        aria-label="Spend rollup"
        className="flex flex-wrap items-baseline gap-x-5 gap-y-1 px-5 py-2.5 text-[12px] leading-5 text-text-3"
      >
        <span className="font-medium text-text-2">Spend</span>
      <WindowSegment
        testid="cost-rollup-since"
        label="since last here"
        spendUsd={sinceSpend}
        capUsd={dailyCapUsd}
        windowDays={windowDaysFrom(sinceLastVisit?.since)}
        emptyLabel={sinceEmpty}
      />
      <WindowSegment
        testid="cost-rollup-week"
        label="this week"
        spendUsd={weekSpend}
        capUsd={dailyCapUsd}
        windowDays={7}
        emptyLabel={weekEmpty}
      />
      </section>
    </Card>
  );
}
