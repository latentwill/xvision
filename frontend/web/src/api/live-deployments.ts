// frontend/web/src/api/live-deployments.ts
//
// CT5 live-deployment surface — the honesty-constrained projection over
// `eval_runs WHERE mode='live'`, joined with broker/execution truth. See
// docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md.
//
// Two read paths (CT5 §3 + §4):
//
//   - POLL (list membership + honest capital floor): `listDeployments()` hits
//     `GET /api/live/deployments` (~5s refetch) and returns the full
//     `LiveDeploymentSummary` rows. This is the SOURCE OF TRUTH for list
//     membership and the degrade target for every capital field.
//
//   - SSE (live-ticking capital, s78.1): `openDeploymentStream()` connects to
//     `GET /api/live/deployments/:id/stream` and overlays the per-tick capital
//     block (esp. unrealized P&L) on top of the poll value. The streamed
//     numbers are the SAME honest book/execution-sourced values as the poll —
//     a field with no real data is OMITTED from the wire (never a fabricated
//     `0`), so it stays `undefined` here and the consumer falls back to the
//     poll value (no blank / `0` flash). No value is ever sourced from
//     `agent_runs` / eval.
//
// Mirrors the `apiFetch` + `buildUrl` + cache-key pattern in `api/eval.ts`, and
// the `EventSource` + exponential-backoff-reconnect pattern in
// `api/agent-runs.ts` (`SSE_BACKOFF_MS` copied verbatim).

import { apiFetch } from "./client";
import type { DeploymentMetricsTick, LiveDeploymentSummary } from "./types.gen";

/// List-envelope returned by `GET /api/live/deployments`. `total` is the
/// pre-limit filtered count (§3). Hand-written because the dashboard route's
/// envelope is Serialize-only and not ts-rs-exported — replace with generated
/// bindings when the backend lands ts-rs derives.
type DeploymentsListResponse = {
  items: LiveDeploymentSummary[];
  total: number;
};

export type ListDeploymentsParams = {
  /// Status filter, comma-joined (e.g. `"running,paused"`). The default
  /// server filter is active-only; ActiveTasksStrip passes `running,paused`.
  status?: string;
  /// `"paper" | "live"` — venue-label filter. Absent => both.
  mode?: string;
  /// Page size. Server defaults to 20, caps at 100.
  limit?: number;
  /// bead s78.2: the operator's last-visit boundary (RFC-3339). When present,
  /// the backend populates `risk_veto_count_since_last_visit` with a REAL count
  /// of recorded risk-veto supervisor notes whose `created_at >= since`. Absent
  /// (first visit) => the field stays `null` (can't count "since an unknown
  /// time"). Invalid RFC-3339 => the endpoint returns 400.
  since?: string;
};

export const deploymentKeys = {
  all: ["live-deployments"] as const,
  /// Cache key folds the params into a stable tuple so a status/mode change
  /// refetches instead of slicing a single full-list result. Absent params
  /// collapse onto the same key as empty params (no extra fetch on first
  /// paint when a caller omits the object).
  list: (params?: ListDeploymentsParams) =>
    [
      ...deploymentKeys.all,
      "list",
      params?.status ?? "",
      params?.mode ?? "",
      params?.limit ?? null,
      params?.since ?? "",
    ] as const,
  one: (id: string) => [...deploymentKeys.all, "one", id] as const,
};

export function buildDeploymentsListUrl(params?: ListDeploymentsParams): string {
  const qs = new URLSearchParams();
  if (params?.status) {
    qs.set("status", params.status);
  }
  if (params?.mode) {
    qs.set("mode", params.mode);
  }
  if (params?.limit !== undefined) {
    qs.set("limit", String(params.limit));
  }
  if (params?.since) {
    qs.set("since", params.since);
  }
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return `/api/live/deployments${suffix}`;
}

/// List live/paper deployments. Returns just the rows (drops `total`) — the
/// ActiveTasksStrip 5s poll only needs list membership; the full
/// `LiveDeploymentSummary` (including the honest null-able capital fields) is
/// carried on each item. The endpoint is connection-as-data and never 500s on
/// a venue outage (§2.3), so a normal resolution may still carry rows with
/// `venue_connected=false` and `null` capital fields.
export function listDeployments(
  params?: ListDeploymentsParams,
): Promise<LiveDeploymentSummary[]> {
  return apiFetch<DeploymentsListResponse>(buildDeploymentsListUrl(params)).then(
    (r) => r.items ?? [],
  );
}

export function getDeployment(id: string): Promise<LiveDeploymentSummary> {
  return apiFetch<LiveDeploymentSummary>(
    `/api/live/deployments/${encodeURIComponent(id)}`,
  );
}

// ---------------------------------------------------------------------------
// SSE — per-deployment live-ticking capital (CT5 §4, bead s78.1)
// ---------------------------------------------------------------------------

/// Backoff schedule for SSE reconnect, copied verbatim from
/// `api/agent-runs.ts::SSE_BACKOFF_MS`. Each step is the delay before the
/// (attempt+1)th reconnect; the last value is the steady-state ceiling.
const SSE_BACKOFF_MS = [500, 1000, 2000, 4000, 8000];

/// The `event:` names the deployment SSE emits, matching the Rust
/// `event_name()` in `crates/xvision-dashboard/src/sse/live_deployment_sse.rs`
/// EXACTLY. The `RunChartEvent` enum maps:
///   Status        -> "status"   (lifecycle / terminal)
///   Equity        -> "metrics"  (equity-only heartbeat, tagged envelope)
///   DeploymentMetrics -> "metrics"  (flat capital tick)
///   Decision      -> "decision"
///   Bar           -> "bar"
///   IndicatorTail -> "indicator_tail"
///   Marker        -> "marker"
/// plus the builder's synthetic `snapshot`, `lagged`, and `error` frames.
///
/// `metrics` is the channel this bead consumes: it carries BOTH the flat
/// capital tick AND the legacy equity-only tagged envelope (see
/// `parseMetricsTick`). The rest are registered so they are not silently
/// dropped, but the consumer only acts on `snapshot` + `metrics` today.
export const LIVE_SSE_EVENTS = [
  "snapshot",
  "metrics",
  "decision",
  "status",
  "bar",
  "indicator_tail",
  "marker",
  "lagged",
  "error",
] as const;
export type LiveSseEventName = (typeof LIVE_SSE_EVENTS)[number];

/// A normalized live capital tick. Every capital field is OPTIONAL: a field the
/// backend omitted from the wire (honesty — no real data) stays `undefined`
/// here so the consumer falls back to the poll value, never a fabricated `0`.
/// `equity_usd` is the only always-present field on a real tick (it is what
/// triggers the emission); on the equity-only heartbeat it is the lone signal
/// and the capital fields are all `undefined`, which is exactly the degrade
/// path (live equity heartbeat + 5s poll for capital).
export type DeploymentMetricsPatch = Partial<DeploymentMetricsTick> & {
  equity_usd?: number;
};

/// Hand-typed deployment-stream event union — replace with generated bindings
/// when the backend lands ts-rs derives for the SSE payload structs (only
/// `DeploymentMetricsTick` is generated today). The `metrics` event's `data`
/// is the NORMALIZED patch (see `parseMetricsTick`): both the flat capital tick
/// and the equity-only envelope collapse onto it.
export type DeploymentStreamEvent =
  | { event: "snapshot"; data: LiveDeploymentSummary }
  | { event: "metrics"; data: DeploymentMetricsPatch }
  | { event: "decision"; data: Record<string, unknown> }
  | { event: "status"; data: Record<string, unknown> }
  | { event: "lagged"; data: { dropped: number } }
  | {
      event: Exclude<
        LiveSseEventName,
        "snapshot" | "metrics" | "decision" | "status" | "lagged"
      >;
      data: Record<string, unknown>;
    };

/// Normalize a raw `metrics` frame into a `DeploymentMetricsPatch`.
///
/// The `metrics` channel multiplexes TWO honest shapes (CT5 §4):
///   1. The FLAT capital tick (`DeploymentMetricsTick`) — top-level
///      `equity_usd`, capital fields present only when sourced (null/omitted
///      otherwise). `RunChartEvent::DeploymentMetrics`.
///   2. The legacy equity-only TAGGED envelope `{ event: "equity", data: {
///      time, equity_usd } }` — the bare `RunChartEvent::Equity` heartbeat,
///      which still maps to `event: metrics` so a pre-capital-tick client gets
///      a live equity signal and DEGRADES to the poll for the capital fields.
///
/// Returns `null` for an unrecognizable payload (dropped silently — the next
/// snapshot / poll re-syncs). Capital fields absent on the wire stay absent on
/// the patch (NEVER coerced to `0`); the consumer keeps the poll value.
export function parseMetricsTick(raw: unknown): DeploymentMetricsPatch | null {
  if (typeof raw !== "object" || raw === null) return null;
  const obj = raw as Record<string, unknown>;

  // Shape 2: equity-only tagged envelope. Detect via the `event` tag (the flat
  // tick never carries `event`; the builder strips it).
  if (obj.event === "equity" && typeof obj.data === "object" && obj.data !== null) {
    const inner = obj.data as Record<string, unknown>;
    const patch: DeploymentMetricsPatch = {};
    if (typeof inner.equity_usd === "number") patch.equity_usd = inner.equity_usd;
    if (typeof inner.time === "number") patch.time = inner.time;
    return patch;
  }

  // Shape 1: flat capital tick. Copy only the numeric fields that are present;
  // an omitted (honest-null) field is left undefined so the poll value wins.
  const patch: DeploymentMetricsPatch = {};
  const numericKeys: Array<keyof DeploymentMetricsTick> = [
    "time",
    "equity_usd",
    "drawdown_pct",
    "deployed_capital_usd",
    "unrealized_pnl_usd",
    "realized_pnl_usd",
    "daily_loss_limit_remaining_usd",
    "n_trades",
  ];
  let sawAny = false;
  for (const k of numericKeys) {
    const v = obj[k];
    if (typeof v === "number") {
      (patch as Record<string, number>)[k] = v;
      sawAny = true;
    }
  }
  return sawAny ? patch : null;
}

/// Open a per-deployment SSE stream. Returns a `close()` handle that tears the
/// `EventSource` down and cancels any pending reconnect — call it on unmount /
/// row change to avoid leaks.
///
/// Reconnect is exponential-backoff per `SSE_BACKOFF_MS`. The snapshot is the
/// first frame on every (re)connect, so a dropped connection self-heals. The
/// caller's `onEvent` receives one typed `DeploymentStreamEvent` per frame; the
/// `metrics` event is already normalized (flat tick OR equity envelope) so the
/// consumer overlays present capital fields and leaves the rest on the poll.
export function openDeploymentStream(
  id: string,
  onEvent: (ev: DeploymentStreamEvent) => void,
): () => void {
  let closed = false;
  let attempt = 0;
  let source: EventSource | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  const url = `/api/live/deployments/${encodeURIComponent(id)}/stream`;

  const handle = (eventName: LiveSseEventName) => (ev: MessageEvent) => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(ev.data as string);
    } catch {
      // Drop malformed frames — the next snapshot / poll re-syncs.
      return;
    }

    if (eventName === "snapshot") {
      onEvent({ event: "snapshot", data: parsed as LiveDeploymentSummary });
      return;
    }
    if (eventName === "metrics") {
      const patch = parseMetricsTick(parsed);
      if (!patch) return;
      onEvent({ event: "metrics", data: patch });
      return;
    }
    if (eventName === "lagged") {
      const dropped =
        typeof (parsed as { dropped?: unknown })?.dropped === "number"
          ? (parsed as { dropped: number }).dropped
          : 0;
      onEvent({ event: "lagged", data: { dropped } });
      return;
    }
    onEvent({
      event: eventName,
      data: parsed as Record<string, unknown>,
    } as DeploymentStreamEvent);
  };

  const connect = () => {
    if (closed) return;
    source = new EventSource(url);
    source.addEventListener("open", () => {
      attempt = 0;
    });
    for (const name of LIVE_SSE_EVENTS) {
      source.addEventListener(name, handle(name) as EventListener);
    }
    source.addEventListener("error", () => {
      if (closed) return;
      source?.close();
      source = null;
      const delay = SSE_BACKOFF_MS[Math.min(attempt, SSE_BACKOFF_MS.length - 1)]!;
      attempt += 1;
      reconnectTimer = setTimeout(connect, delay);
    });
  };

  connect();

  return () => {
    closed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    source?.close();
    source = null;
  };
}
