// frontend/web/src/features/home/last-visit.ts
//
// "Since you were last here" header delta (Control Tower bead xvision-jlm).
//
// The home subtitle answers "what changed since I last looked?" from facts
// the page already fetched — eval RUN completions and review FINDINGS. These
// are HONEST counts (not live-money / P&L / capital), so they need no CT5
// live contract. The last-visit boundary is a per-operator / per-browser
// preference, persisted in localStorage (decision §2 in the plan), not a
// server fact.
//
// LAST_VISIT_LS is exported so xvision-8wn's cost rollup windows off the
// SAME boundary key — the home delta subtitle and the cost strip must agree.

import type { RunSummary } from "@/api/types.gen";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";

/** localStorage key for the last-visit RFC3339 timestamp. Shared with 8wn. */
export const LAST_VISIT_LS = "xvn.home.last_visit";

/** Read the stored last-visit timestamp. `null` on a first visit or when
 * storage is unavailable (private mode / blocked) — `safeStorageGet`
 * swallows the throw so app startup is never blocked. */
export function readLastVisit(): string | null {
  return safeStorageGet(LAST_VISIT_LS);
}

/** Persist the supplied RFC3339 timestamp as the new last-visit boundary.
 * Best-effort: blocked storage is swallowed by `safeStorageSet`. Callers do
 * the read-before-write on mount so the in-flight render still shows the
 * PREVIOUS boundary's delta. */
export function writeLastVisit(nowIso: string): void {
  safeStorageSet(LAST_VISIT_LS, nowIso);
}

// ---------------------------------------------------------------------------
// Page-load-session boundary (read-before-write, remount-safe)
//
// The subtitle must show the delta since the PREVIOUS visit even after the
// current visit's write lands. Per-component refs are not enough: a real
// in-session remount (SPA nav Dashboard → Settings → Dashboard) — and React
// StrictMode's dev double-invoke — gives the second mount a fresh component
// that would read back the timestamp this session just wrote, collapsing the
// delta to ~0. Freezing the boundary at MODULE scope (captured once per page
// load, before any write) makes every render this session measure from the
// same prior-visit boundary, and the write happens exactly once.
// ---------------------------------------------------------------------------

/** `undefined` until first captured this page load; then the frozen boundary. */
let sessionBoundary: string | null | undefined;
let sessionPersisted = false;

/** The last-visit boundary as it was on the FIRST read this page load.
 * Idempotent across remounts/StrictMode: frozen on first call (before
 * `persistVisitOnce` overwrites storage), so every render this session sees
 * the same prior-visit boundary. */
export function snapshotLastVisit(): string | null {
  if (sessionBoundary === undefined) {
    sessionBoundary = readLastVisit();
  }
  return sessionBoundary;
}

/** Persist `nowIso` as the new boundary exactly once per page load. Safe to
 * call from every mount/effect; only the first call writes. Freezes the prior
 * boundary first so a caller that skipped `snapshotLastVisit` still can't lose
 * it. */
export function persistVisitOnce(nowIso: string): void {
  if (sessionPersisted) return;
  sessionPersisted = true;
  if (sessionBoundary === undefined) sessionBoundary = readLastVisit();
  writeLastVisit(nowIso);
}

/** Test-only: clear the module-scoped session state between tests. */
export function __resetVisitSessionForTest(): void {
  sessionBoundary = undefined;
  sessionPersisted = false;
}

/** A finding row for delta counting — only its creation stamp matters. */
export interface DeltaFinding {
  created_at?: string | null;
}

export interface SinceDeltaInput {
  runs: RunSummary[];
  findings: DeltaFinding[];
  /** Previous boundary; `null`/unparseable ⇒ first visit. */
  lastVisitIso: string | null;
  /** Epoch ms "now"; defaults to `Date.now()`. */
  now?: number;
}

export interface SinceDelta {
  /** Runs whose `completed_at` is strictly after `lastVisitIso`. */
  runsSince: number;
  /** Findings whose `created_at` is strictly after `lastVisitIso`. */
  findingsSince: number;
  /** Whole hours from `lastVisitIso` to `now`, floored, clamped ≥ 0;
   * `null` on a first visit. */
  hoursAgo: number | null;
  /** True when there is no usable prior boundary (first visit). */
  firstVisit: boolean;
}

/** Count an RFC3339-stamped collection strictly after the boundary epoch.
 * Unparseable / missing stamps are skipped (never counted). */
function countAfter(
  stamps: Array<string | null | undefined>,
  boundaryMs: number,
): number {
  let n = 0;
  for (const s of stamps) {
    if (!s) continue;
    const t = Date.parse(s);
    if (Number.isFinite(t) && t > boundaryMs) n += 1;
  }
  return n;
}

/**
 * Pure delta selector. Counts runs (by `completed_at`) and findings (by
 * `created_at`) STRICTLY AFTER `lastVisitIso` (the boundary itself is
 * excluded), and the whole-hours-ago stamp. A null/unparseable boundary is
 * a first visit: zero counts, null `hoursAgo`, `firstVisit: true`.
 */
export function computeSinceDelta(input: SinceDeltaInput): SinceDelta {
  const { runs, findings, lastVisitIso } = input;
  const now = input.now ?? Date.now();

  const boundaryMs = lastVisitIso ? Date.parse(lastVisitIso) : Number.NaN;
  if (!Number.isFinite(boundaryMs)) {
    return { runsSince: 0, findingsSince: 0, hoursAgo: null, firstVisit: true };
  }

  const runsSince = countAfter(
    runs.map((r) => r.completed_at),
    boundaryMs,
  );
  const findingsSince = countAfter(
    findings.map((f) => f.created_at),
    boundaryMs,
  );

  const hoursAgo = Math.max(0, Math.floor((now - boundaryMs) / 3_600_000));

  return { runsSince, findingsSince, hoursAgo, firstVisit: false };
}
