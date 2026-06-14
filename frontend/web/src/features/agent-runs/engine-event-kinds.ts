// frontend/web/src/features/agent-runs/engine-event-kinds.ts
//
// WS-8 taxonomy completeness. The trace carries two kinds of timeline rows:
// observability *spans* (kept in `span-colors.ts`) and *engine events* — the
// bar-level lifecycle signals written to the `events` table and streamed live
// as `engine_event` SSE frames (`EngineEvent.kind`, see
// `crates/xvision-observability/src/events.rs`).
//
// Until WS-8, engine events never reached the dock: the trace rendered only
// `spans`. This module is the canonical registry that gives every known
// `EngineEvent.kind` a FAMILY (color/category) and a human LABEL so it renders
// as a first-class, distinctly-colored row — and gives any UNKNOWN kind a
// typed fallback (raw kind shown) so a new engine event is never silently
// dropped.
//
// The family taxonomy mirrors the operator's mental model of the trading loop:
// decision · risk · filter · regime · order · memory · attest · cost · error
// · lifecycle. Adding a new engine kind upstream means adding one row here; a
// kind we forget still renders (typed fallback), it just isn't colored as its
// true family.

/**
 * Visual + filterable family for an engine event. `unknown` is the typed
 * fallback for kinds not yet in the registry — a row in this family still
 * renders (it shows the raw kind) so nothing is dropped.
 */
export type EngineEventFamily =
  | "decision"
  | "risk"
  | "filter"
  | "regime"
  | "order"
  | "memory"
  | "attest"
  | "cost"
  | "error"
  | "lifecycle"
  | "unknown";

type FamilyStyle = {
  /** Canonical color (bar / dot / badge). */
  hex: string;
  /** SHORT uppercase tag shown in the inspector + tree badge. */
  label: string;
};

/**
 * Per-family palette. Tints are chosen to sit alongside the span palette in
 * `span-colors.ts` without colliding: engine events read as a parallel band of
 * lifecycle signals. `unknown` is a neutral slate so a fallback row is visibly
 * "uncategorised" rather than masquerading as a known family.
 */
export const ENGINE_EVENT_FAMILY_STYLES: Record<EngineEventFamily, FamilyStyle> = {
  decision: { hex: "#fbbf24", label: "DECIS" },
  risk: { hex: "#f87171", label: "RISK" },
  filter: { hex: "#34d399", label: "FILTR" },
  regime: { hex: "#c084fc", label: "REGIM" },
  order: { hex: "#f472b6", label: "ORDER" },
  memory: { hex: "#22d3ee", label: "MEMRY" },
  attest: { hex: "#facc15", label: "ATTST" },
  cost: { hex: "#fb923c", label: "COST" },
  error: { hex: "#ef4444", label: "ERROR" },
  lifecycle: { hex: "#94a3b8", label: "LIFE" },
  // Neutral slate — a fallback row should read as "uncategorised", not as a
  // confidently-typed family.
  unknown: { hex: "#64748b", label: "EVENT" },
};

/**
 * Registry of every known `EngineEvent.kind`. Sourced from:
 *   - the `EngineEvent` struct docs in
 *     `crates/xvision-observability/src/events.rs` (known F43 kinds), and
 *   - every `obs.emit_engine_event("…")` call site in `xvision-engine`.
 * Plus the convergence kinds named in the WS-8 contract (order / regime /
 * memory / attest / position-exit) so the registry is forward-complete for the
 * UnifiedEvent convergence.
 *
 * Each entry maps the wire kind to a family + a friendly label. The label MUST
 * NOT be the raw snake_case kind — it's the operator-facing descriptor.
 */
const REGISTRY: Record<string, { family: EngineEventFamily; label: string }> = {
  // ── decision lifecycle ──
  decision_started: { family: "decision", label: "Decision started" },
  decision_completed: { family: "decision", label: "Decision completed" },
  fill_attempted: { family: "decision", label: "Fill attempted" },
  flat_skip_fired: { family: "decision", label: "Flat skip" },
  guardrail_fired: { family: "decision", label: "Guardrail fired" },
  early_stop_triggered: { family: "decision", label: "Early stop" },
  preflight_warning: { family: "lifecycle", label: "Preflight warning" },

  // ── risk gate ──
  risk_veto: { family: "risk", label: "Risk veto" },

  // ── filter / signal ──
  filter_fired: { family: "filter", label: "Filter fired" },
  filter_parse_error: { family: "filter", label: "Filter parse error" },
  graph_agent_gated_out: { family: "filter", label: "Agent gated out" },

  // ── regime ──
  regime_transition: { family: "regime", label: "Regime transition" },

  // ── orders / venue ──
  order_signed: { family: "order", label: "Order signed" },
  order_state: { family: "order", label: "Order state" },
  broker_rule_violation: { family: "order", label: "Broker rule violation" },
  venue_account_snapshot: { family: "order", label: "Venue account snapshot" },
  position_exit: { family: "order", label: "Position exit" },

  // ── memory ──
  memory_recall: { family: "memory", label: "Memory recall" },
  memory_write: { family: "memory", label: "Memory write" },

  // ── attestation ──
  attest_boundary_reached: { family: "attest", label: "Attest boundary" },

  // ── cost / budget ──
  cost_cap_warning: { family: "cost", label: "Cost cap warning" },
  granularity_fallback: { family: "lifecycle", label: "Granularity fallback" },
  data_defect: { family: "error", label: "Data defect" },
};

/**
 * The known engine-event kinds. Exported for the parity test so the
 * exhaustive-coverage check has a single source of truth.
 */
export const KNOWN_ENGINE_EVENT_KINDS: readonly string[] = Object.keys(REGISTRY);

/**
 * Family for an engine-event kind. Unknown kinds resolve to `"unknown"` — the
 * typed fallback — so they still render (as an uncategorised event row) rather
 * than being dropped.
 */
export function engineEventFamilyOf(kind: string): EngineEventFamily {
  return REGISTRY[kind]?.family ?? "unknown";
}

/**
 * Human-readable label for an engine-event kind. For known kinds this is the
 * registered descriptor; for unknown kinds it's a Title-Cased rendering of the
 * raw kind so the row is never blank and the operator still sees what fired.
 */
export function engineEventLabelOf(kind: string): string {
  const known = REGISTRY[kind];
  if (known) return known.label;
  return humanizeKind(kind);
}

/** `some_future_engine_kind` → `some future engine kind` (lowercased words). */
function humanizeKind(kind: string): string {
  const cleaned = kind.replace(/[_\-.]+/g, " ").trim();
  return cleaned.length > 0 ? cleaned : kind;
}

/** Style (color + short badge) for an engine-event kind's family. */
export function engineEventStyle(kind: string): FamilyStyle {
  return ENGINE_EVENT_FAMILY_STYLES[engineEventFamilyOf(kind)];
}

/** Whether an engine-event kind is in the registry (vs. a fallback). */
export function isKnownEngineEventKind(kind: string): boolean {
  return kind in REGISTRY;
}
