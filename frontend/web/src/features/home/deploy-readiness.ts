// frontend/web/src/features/home/deploy-readiness.ts
//
// Pure deploy-readiness selector for the home page (xvision-e17). Composes
// already-fetched config snapshots into an ordered checklist that answers
// "is this node safe to deploy a live strategy against?". NO JSX, NO fetch —
// the component (DeployReadinessStrip) owns rendering, the route owns fetching.
//
// Three minimum checks (spec §7.1 panel 6 + the unified plan §Wave-1/e17):
//   1. keys            — at least one non-synthetic provider has its key set
//   2. broker          — broker configured AND reachable. Configured-but-
//                        UNREACHABLE is an EXPLICIT FAIL / deploy-blocker,
//                        never an "unknown" — a dead broker silently swallows
//                        live orders.
//   3. no blocking eval — NOT safety-paused AND no in-flight run stuck > 2h.
//
// HONESTY MANDATE (spec §8.1/§8.9): this selector reasons only over honest
// config facts. It never reads, derives, or emits a P&L / capital / equity /
// budget number — even though `AlpacaTestReport` carries an `equity` field,
// that value is deliberately ignored here.

import type { BrokersReport, ProviderRow, RunSummary } from "@/api/types.gen";
import type { AlpacaTestReport } from "@/api/types.gen";
import type { SafetyStateResponse } from "@/api/safety";

/** A run is considered "stuck" once it has been *running* for longer than
 * this. Mirrors `ActiveTasksStrip.TWO_HOURS_MS` so the home page tells one
 * consistent story about what "stuck" means. */
export const DEPLOY_READINESS_STUCK_MS = 2 * 60 * 60 * 1000;

export type ReadinessStatus = "pass" | "fail" | "unknown";

export interface ReadinessCheck {
  /** Stable machine id — drives ordering, keys, and component test selectors. */
  id: "keys" | "broker" | "no-blocking-eval";
  /** Plain operator-facing label rendered next to the tone dot. */
  label: string;
  status: ReadinessStatus;
  /** Short honest explanation. Never contains a money / P&L figure. */
  detail: string;
  /** Routed fix — present on failing/unknown checks so the operator can act. */
  link?: { to: string; label: string };
}

export interface DeployReadinessInput {
  /** `listProviders().providers`; `undefined` until the query resolves. */
  providers: ProviderRow[] | undefined;
  /** `getBrokers()`; `undefined` until the query resolves. */
  brokers: BrokersReport | undefined;
  /** `testAlpacaConnection()`; `undefined` when not configured / not yet run. */
  brokerTest: AlpacaTestReport | undefined;
  /** `getSafetyState()`; `undefined` until the query resolves. */
  safety: SafetyStateResponse | undefined;
  /** In-flight eval runs (status queued|running) used for the stuck check. */
  inflightRuns: RunSummary[];
  /** Injectable clock for deterministic tests; defaults to `Date.now()`. */
  nowMs?: number;
}

// ─── individual checks ─────────────────────────────────────────────────────

/** A provider counts toward "keys" only if it actually requires an API key
 * (non-empty `api_key_env`) and is not a synthetic placeholder row. */
function requiresKey(p: ProviderRow): boolean {
  return !p.synthetic && p.api_key_env.trim() !== "";
}

function keysCheck(providers: ProviderRow[] | undefined): ReadinessCheck {
  const link = { to: "/settings/providers", label: "configure" };
  if (providers === undefined) {
    return {
      id: "keys",
      label: "keys",
      status: "unknown",
      detail: "checking provider keys…",
    };
  }

  const keyed = providers.filter(requiresKey);
  const missing = keyed.filter((p) => !p.api_key_set);

  if (missing.length > 0) {
    const names = missing.map((p) => p.name).join(", ");
    return {
      id: "keys",
      label: "keys",
      status: "fail",
      detail: `missing API key — ${names}`,
      link,
    };
  }

  // No keyed provider is missing its key. That's only a genuine PASS when at
  // least one usable provider exists; an empty provider table can't run a
  // strategy at all.
  const usable = providers.filter((p) => !p.synthetic);
  if (usable.length === 0) {
    return {
      id: "keys",
      label: "keys",
      status: "fail",
      detail: "no provider configured",
      link,
    };
  }

  return {
    id: "keys",
    label: "keys",
    status: "pass",
    detail: "provider keys present",
  };
}

function brokerCheck(
  brokers: BrokersReport | undefined,
  brokerTest: AlpacaTestReport | undefined,
): ReadinessCheck {
  const link = { to: "/settings/brokers", label: "configure" };
  if (brokers === undefined) {
    return {
      id: "broker",
      label: "broker",
      status: "unknown",
      detail: "checking broker…",
    };
  }

  if (!brokers.alpaca.configured) {
    // Nothing to deploy against yet — not a blocker, just not ready.
    return {
      id: "broker",
      label: "broker",
      status: "unknown",
      detail: "no broker configured",
      link,
    };
  }

  // Configured. Reachability is the gate.
  if (brokerTest === undefined) {
    return {
      id: "broker",
      label: "broker",
      status: "unknown",
      detail: "testing connection…",
      link,
    };
  }

  if (!brokerTest.ok) {
    // §7.1 panel 6: configured-but-unreachable is an EXPLICIT deploy-blocker.
    const reason = brokerTest.error?.trim();
    return {
      id: "broker",
      label: "broker",
      status: "fail",
      detail: reason ? `unreachable — ${reason}` : "unreachable",
      link,
    };
  }

  return {
    id: "broker",
    label: "broker",
    status: "pass",
    detail: "configured and reachable",
  };
}

/** A run blocks deploy when it has been *running* (not merely queued) past the
 * stuck threshold. Queued runs that simply wait do not count. */
function isStuckRun(run: RunSummary, nowMs: number): boolean {
  if (run.status !== "running") return false;
  if (!run.started_at) return false;
  const startedMs = Date.parse(run.started_at);
  if (Number.isNaN(startedMs)) return false;
  return nowMs - startedMs > DEPLOY_READINESS_STUCK_MS;
}

function noBlockingEvalCheck(
  safety: SafetyStateResponse | undefined,
  inflightRuns: RunSummary[],
  nowMs: number,
): ReadinessCheck {
  if (safety === undefined) {
    return {
      id: "no-blocking-eval",
      label: "no blocking eval",
      status: "unknown",
      detail: "checking safety state…",
    };
  }

  if (safety.paused) {
    const reason = safety.reason?.trim();
    return {
      id: "no-blocking-eval",
      label: "no blocking eval",
      status: "fail",
      detail: reason ? `safety paused — ${reason}` : "safety paused",
      link: { to: "/safety", label: "resume" },
    };
  }

  const stuck = inflightRuns.filter((r) => isStuckRun(r, nowMs));
  if (stuck.length > 0) {
    const n = stuck.length;
    return {
      id: "no-blocking-eval",
      label: "no blocking eval",
      status: "fail",
      detail: `${n} run${n === 1 ? "" : "s"} stuck (running over 2h)`,
      link: { to: "/eval-runs", label: "review" },
    };
  }

  return {
    id: "no-blocking-eval",
    label: "no blocking eval",
    status: "pass",
    detail: "no blocking eval",
  };
}

// ─── public selector ───────────────────────────────────────────────────────

/** Build the ordered deploy-readiness checklist. Order is stable: keys,
 * broker, no-blocking-eval. */
export function buildDeployReadiness(input: DeployReadinessInput): ReadinessCheck[] {
  const nowMs = input.nowMs ?? Date.now();
  return [
    keysCheck(input.providers),
    brokerCheck(input.brokers, input.brokerTest),
    noBlockingEvalCheck(input.safety, input.inflightRuns, nowMs),
  ];
}

/** True when every check is a clean PASS — the component collapses to a single
 * "Ready to deploy" line in that case. */
export function isDeployReady(checks: ReadinessCheck[]): boolean {
  return checks.length > 0 && checks.every((c) => c.status === "pass");
}
