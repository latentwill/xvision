// frontend/web/src/features/home/failed-runs.ts
//
// Failed-run triage split (bead xvision-1zs). Pure selectors that classify
// failed `RunSummary` entries into two operator surfaces:
//
//   (a) STALE INFRA errors — provider/network/timeout-style failures that
//       have sat failed for longer than STALE_FAILED_HOURS. These are calm
//       "something upstream broke; the run is still sitting here failed"
//       nags routed to the run. Fresh infra blips are NOT nagged — they may
//       self-heal on the next retry. -> NagStrip (`failedRunNags`).
//
//   (b) SUSPICIOUS failures — anything that doesn't look like transient
//       infrastructure and isn't a deliberate stop (a panic, a strategy that
//       produced no decisions, a circuit-breaker abort). These read like a
//       real strategy/result problem worth a look. -> Recent Findings
//       (`failedRunFindings`).
//
// DELIBERATE stops are excluded from BOTH lists: a safety-pause abort
// (`aborted: safety_paused …`), the other safety-subsystem aborts
// (`aborted: safety_limit …`, `aborted: venue_label_mismatch …`), and the
// `[budget_exceeded]` clean stop are operator/policy outcomes, not failures.
//
// HONESTY: these are eval-run COUNTS and timestamps only — never P&L,
// capital, or budget money. No fabricated numbers. The wire shape today has
// no structured `error_kind` discriminator (errors are free-form anyhow
// chains), so we classify by a regex table; a clean engine-side `error_kind`
// enum is a separable follow-up.

import type { RunSummary } from "@/api/types.gen";

import { formatRelativeTime } from "./pulse";

/** Failed runs older than this many hours count as a "stale" infra nag.
 * Fresh infra failures are not nagged (they may self-heal on retry). */
export const STALE_FAILED_HOURS = 2;

const STALE_FAILED_MS = STALE_FAILED_HOURS * 60 * 60 * 1000;

/**
 * Deliberate / clean-stop error prefixes that are NOT failures. A run that
 * carries one of these in its `error` column was stopped on purpose (safety
 * subsystem) or pulled cleanly (budget cap), so it belongs in neither the
 * infra-nag list nor the suspicious-findings list.
 *
 * Mirrors `xvision_engine::eval::run::RunAbort::reason()`
 * (`aborted: <tag> — …`) and the `[budget_exceeded]` tag handled by
 * `features/eval-runs/RunSummary.tsx`.
 */
const DELIBERATE_STOP_PATTERNS: RegExp[] = [
  /aborted:\s*safety_paused/i,
  /aborted:\s*safety_limit/i,
  /aborted:\s*venue_label_mismatch/i,
  /\[budget_exceeded\]/i,
  /budget_(wall_ms|input_tokens|output_tokens)_exceeded/i,
];

/**
 * Infrastructure / transient-failure patterns: provider outages, network
 * errors, rate limits, timeouts, TLS/connection resets. These come and go
 * with upstream health rather than reflecting a strategy/result problem.
 */
export const INFRA_ERROR_PATTERNS: RegExp[] = [
  /\bconnection (refused|reset|closed|error)\b/i,
  /\bECONN(REFUSED|RESET|ABORTED)\b/i,
  /\bsocket hang ?up\b/i,
  /\btimed? ?out\b/i,
  /\btimeout\b/i,
  /\brate ?limit\b/i,
  /\b429\b/,
  /too many requests/i,
  /\b(502|503|504)\b/,
  /service unavailable/i,
  /bad gateway/i,
  /gateway timeout/i,
  /\bdns\b/i,
  /failed to lookup address/i,
  /tls handshake/i,
  /\bnetwork\b.*\b(error|unreachable)\b/i,
  /upstream/i,
  /temporarily unavailable/i,
];

export type FailedRunKind =
  | "none" // not a failed run
  | "excluded" // deliberate / clean stop
  | "infra" // transient infrastructure failure
  | "suspicious"; // looks like a real strategy/result problem

export interface FailedRunClassification {
  kind: FailedRunKind;
}

/**
 * Classify a single run. Only `status === "failed"` runs are candidates;
 * everything else is `none`. A failed run with no error string is treated
 * as `suspicious` (unexplained failures deserve a look, not a calm nag).
 */
export function classifyFailedRun(run: RunSummary): FailedRunClassification {
  if (run.status !== "failed") return { kind: "none" };

  const error = run.error ?? "";

  if (DELIBERATE_STOP_PATTERNS.some((re) => re.test(error))) {
    return { kind: "excluded" };
  }

  if (error.trim().length === 0) {
    // Failed with no recorded reason — unexplained, route to findings.
    return { kind: "suspicious" };
  }

  if (INFRA_ERROR_PATTERNS.some((re) => re.test(error))) {
    return { kind: "infra" };
  }

  return { kind: "suspicious" };
}

/** The freshness stamp used for staleness math: completion if present,
 * else the start stamp (a run that never completed but has been failed a
 * long time is still stale). Returns NaN when neither parses. */
function failedStampMs(run: RunSummary): number {
  const iso = run.completed_at ?? run.started_at;
  const t = iso ? new Date(iso).getTime() : NaN;
  return t;
}

function isStale(run: RunSummary, nowMs: number): boolean {
  const t = failedStampMs(run);
  if (!Number.isFinite(t)) return false;
  return nowMs - t >= STALE_FAILED_MS;
}

// ─── NagStrip shape ──────────────────────────────────────────────────────────
// Matches the `AttentionItem` shape consumed by NagStrip, expressed locally so
// this pure-selector module stays free of component imports.

export interface FailedRunNag {
  tone: "warn" | "danger" | "info";
  title: string;
  detail: string;
  link?: { to: string; label: string };
}

/**
 * Build calm nag rows for STALE infra failures only. Fresh infra failures
 * and all suspicious / deliberate-stop runs are omitted. Each nag routes to
 * the run with a "view run" affordance and a relative-age detail.
 */
export function failedRunNags(
  runs: RunSummary[],
  nowMs: number = Date.now(),
): FailedRunNag[] {
  const stale = runs.filter(
    (r) => classifyFailedRun(r).kind === "infra" && isStale(r, nowMs),
  );
  // Newest-failed first so the most recent stale infra issue leads.
  stale.sort((a, b) => {
    const sa = a.completed_at ?? a.started_at ?? "";
    const sb = b.completed_at ?? b.started_at ?? "";
    return sb.localeCompare(sa);
  });
  return stale.map((run) => {
    const name = run.strategy?.display_name;
    const age = formatRelativeTime(run.completed_at ?? run.started_at, nowMs);
    return {
      tone: "warn",
      title: name
        ? `${name} run failed (infra) — still unresolved`
        : "Run failed (infra) — still unresolved",
      detail: age ? `last attempt ${age}` : "",
      link: { to: `/eval-runs/${run.id}`, label: "view run" },
    };
  });
}

// ─── Recent Findings shape ───────────────────────────────────────────────────

export interface FailedRunFinding {
  /** Stable key for React lists + dedupe (`failed-run:<runId>`). */
  id: string;
  runId: string;
  strategyName?: string;
  /** One-line, operator-readable summary (the raw error, trimmed). */
  summary: string;
}

/** A short, operator-readable line from the raw error string. */
function summarizeError(error: string | null): string {
  const trimmed = (error ?? "").trim();
  if (trimmed.length === 0) return "Run failed with no recorded reason";
  // Collapse whitespace; keep it as a single readable line.
  const oneLine = trimmed.replace(/\s+/g, " ");
  return oneLine.length > 160 ? `${oneLine.slice(0, 157)}…` : oneLine;
}

/**
 * Build danger findings for SUSPICIOUS failures (age-independent). Ordered
 * newest-completed-first so the freshest problem leads. Infra and deliberate
 * stops are excluded.
 */
export function failedRunFindings(
  runs: RunSummary[],
  _nowMs: number = Date.now(),
): FailedRunFinding[] {
  const suspicious = runs.filter(
    (r) => classifyFailedRun(r).kind === "suspicious",
  );
  suspicious.sort((a, b) => {
    const sa = a.completed_at ?? a.started_at ?? "";
    const sb = b.completed_at ?? b.started_at ?? "";
    return sb.localeCompare(sa); // newest first
  });
  return suspicious.map((run) => ({
    id: `failed-run:${run.id}`,
    runId: run.id,
    strategyName: run.strategy?.display_name ?? undefined,
    summary: summarizeError(run.error),
  }));
}
