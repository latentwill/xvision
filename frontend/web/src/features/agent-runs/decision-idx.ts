// frontend/web/src/features/agent-runs/decision-idx.ts
//
// Extract the per-decision-cycle index from a broker.call span's raw
// attributes bag. The carrier is the broker-submit idempotency key,
// formatted as `"<run_id>-<decision_idx>"` by the paper executor
// (`crates/xvision-engine/src/eval/executor/paper.rs:1048`) and stored
// inside `attributes_json.broker_call.idempotency_key`. See the
// documented carrier contract on `BrokerCallStartedEvent.idempotency_key`
// in `crates/xvision-observability/src/events.rs`.
//
// PR #385 introduced the TRADE button + broker-span filter chip but the
// `RunSpan.decision_idx` consumer fields (FilterBar dropdown,
// `deriveDecisions`, `use-span-filter`) never received a value — this
// projection closes that gap. Option B: JS-side extraction. Rationale
// in PR body; in short: `RunSpan` is hand-maintained (not ts-rs),
// the wire payload already carries the value, and centralising the
// parse here keeps every consumer typed without a schema change.
//
// Per `feedback_alpha_root_cause.md`: parse failures return `undefined`
// so the field is dropped from the wire shape (RunSpan.decision_idx is
// optional). We do not silently coerce — a malformed key is treated as
// "no decision attached" rather than `0`.

/** Object-shape guard. Re-declared locally to avoid a cycle with `agent-runs.ts`. */
function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Parse `"<run_id>-<decision_idx>"` into the trailing integer. Returns
 * `undefined` when the key is missing or the trailing segment is not a
 * non-negative integer.
 *
 * `run_id` may itself contain `-` (e.g. `run_eval_2026-05-20-abc`), so
 * we split on the *last* `-` rather than the first. Empty trailing
 * segment → `undefined`.
 */
export function decisionIdxFromIdempotencyKey(
  key: string | null | undefined,
): number | undefined {
  if (typeof key !== "string" || key.length === 0) return undefined;
  const lastDash = key.lastIndexOf("-");
  if (lastDash < 0 || lastDash === key.length - 1) return undefined;
  const tail = key.slice(lastDash + 1);
  // Reject anything that isn't a non-negative integer (no signs, no
  // floats, no leading zeros that would imply a non-numeric scheme).
  // `Number()` would coerce `"4abc"` → NaN but also `""` → 0, so a
  // strict regex is the clearer contract.
  if (!/^\d+$/.test(tail)) return undefined;
  const n = Number.parseInt(tail, 10);
  return Number.isFinite(n) ? n : undefined;
}

/**
 * Read the broker-submit `idempotency_key` out of a parsed
 * `attributes_json` payload and project the trailing decision index.
 * Returns `undefined` when the span isn't a broker.call, the
 * `broker_call` blob is malformed, or the key isn't shaped as
 * `<run_id>-<decision_idx>`.
 *
 * Callers should pass the already-parsed attributes bag (the same
 * `attrs` value `flattenExportSpans` builds). This helper does not
 * re-parse `attributes_json` strings — that's the normaliser's job.
 */
export function decisionIdxFromAttributes(
  attrs: Record<string, unknown>,
): number | undefined {
  const brokerCall = attrs["broker_call"];
  if (!isObject(brokerCall)) return undefined;
  const key = brokerCall["idempotency_key"];
  if (typeof key !== "string") return undefined;
  return decisionIdxFromIdempotencyKey(key);
}
