// frontend/web/src/features/agent-runs/trace-labels.ts
//
// QA round 7, F-5: human-readable descriptors for `payload_ref` lines
// in the trace inspector. The raw `blob://prompts/<hash>` form gives
// the operator no signal about *what* the ref points at — replace
// it with a typed summary derived from the span's kind + attributes,
// and keep the hash as a secondary affordance (rendered separately in
// the inspector as a copyable value).
//
// Kept as a small pure function so the unit-test surface is one
// table-driven `describe` block, not an inline switch sprinkled
// through SpanInspector.

import type { RunSpan } from "@/api/types-agent-runs";

/**
 * Variant tag for the secondary surface (copy button / tooltip) in the
 * inspector. The shape mirrors the call sites in SpanInspector — each
 * payload ref renders as one of these and the inspector picks the
 * appropriate label.
 */
export type PayloadRefKind = "prompt" | "response" | "tool_input" | "tool_output";

export type TraceLabelInput = {
  /** The span the ref belongs to. Kind + name + attributes drive the label. */
  span: RunSpan;
  /** Which ref on the span we're labelling. */
  refKind: PayloadRefKind;
  /** The ref string. Used for byte-size heuristics only — never rendered as the label. */
  ref?: string;
};

/**
 * Build the one-line operator-readable descriptor for a payload ref.
 *
 * Examples (from the F-5 spec):
 *   `system prompt — trader/v3`
 *   `tool result — bars[1h, 480 rows]`
 *   `compute_indicator(rsi, 14)`
 *   `TraderDecision · BUY 0.4 BTC`
 *
 * When the producer hasn't populated enough attributes to write a rich
 * summary, fall back to a typed placeholder (`prompt blob`, `tool input`)
 * rather than a bare hash — the hash always lives on the secondary
 * affordance.
 */
export function formatTraceLabel(input: TraceLabelInput): string {
  const { span, refKind } = input;
  // Broker spans carry a typed payload — the highest-fidelity summary
  // possible without fetching the blob.
  if (span.broker_call) {
    return formatBrokerCallLabel(span);
  }
  switch (refKind) {
    case "prompt":
      return formatPromptLabel(span);
    case "response":
      return formatResponseLabel(span);
    case "tool_input":
      return formatToolInputLabel(span);
    case "tool_output":
      return formatToolOutputLabel(span);
  }
}

/**
 * Operator-surface label for an OPTI scope row (WS-11a). The autooptimizer
 * cycle trace projects developer-surface kinds (`opti.gate`, `opti.experiment`,
 * …) but the dock must read in plain language per the terminology lock —
 * "Experiment proposed", "Active" / "Suspect" / "Rejected" gate outcomes,
 * "Honesty check", "Judge finding", "Flywheel compiled".
 *
 * Returns `null` for any non-OPTI span so existing call sites keep their
 * payload-ref label path untouched.
 */
export function optiSpanLabel(span: RunSpan): string | null {
  switch (span.kind) {
    case "opti.cycle":
      return "Optimizer cycle";
    case "opti.parent":
      return "Parent selected";
    case "opti.experiment":
      return "Experiment proposed";
    case "opti.honesty":
      return "Honesty check";
    case "opti.judge":
      return "Judge finding";
    case "opti.flywheel":
      return "Flywheel compiled";
    case "opti.gate": {
      const outcome = (span.attributes as { outcome?: unknown }).outcome;
      if (outcome === "kept") return "Active";
      if (outcome === "suspect") return "Suspect";
      if (outcome === "rejected") return "Rejected";
      return "Gate evaluated";
    }
    default:
      return null;
  }
}

function formatBrokerCallLabel(span: RunSpan): string {
  const bc = span.broker_call!;
  const side = bc.side.toUpperCase();
  const qty = formatQty(bc.qty);
  const symbol = bc.symbol || "—";
  const head = `BrokerCall · ${side} ${qty} ${symbol}`.trim();
  if (bc.outcome === "filled" && bc.fill_price != null) {
    return `${head} @ ${formatPrice(bc.fill_price)}`;
  }
  if (bc.outcome) {
    return `${head} (${bc.outcome})`;
  }
  return head;
}

function formatPromptLabel(span: RunSpan): string {
  // Stage label lives in attributes (`stage` per harness-span-attrs-populate
  // PR #294) or, failing that, in the span name.
  const stage = pickStage(span);
  const model = pickModel(span);
  if (stage && model) return `prompt — ${stage} · ${model}`;
  if (stage) return `prompt — ${stage}`;
  if (model) return `prompt — ${model}`;
  return "prompt blob";
}

function formatResponseLabel(span: RunSpan): string {
  // For trader-stage model.calls the response IS the TraderDecision —
  // surface it as such so the operator knows the ref opens a decision,
  // not a raw completion.
  const stage = pickStage(span);
  const model = pickModel(span);
  if (stage === "trader") {
    return model ? `TraderDecision · ${model}` : "TraderDecision";
  }
  if (stage && model) return `response — ${stage} · ${model}`;
  if (stage) return `response — ${stage}`;
  if (model) return `response — ${model}`;
  return "response blob";
}

function formatToolInputLabel(span: RunSpan): string {
  const toolName = pickToolName(span);
  const argSummary = pickArgSummary(span);
  if (toolName && argSummary) return `${toolName}(${argSummary})`;
  if (toolName) return toolName;
  return "tool input";
}

function formatToolOutputLabel(span: RunSpan): string {
  const toolName = pickToolName(span);
  const summary = pickResultSummary(span);
  if (toolName && summary) return `tool result — ${toolName}[${summary}]`;
  if (toolName) return `tool result — ${toolName}`;
  return "tool result";
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function pickStage(span: RunSpan): string | null {
  const attrs = span.attributes ?? {};
  const stage = (attrs as Record<string, unknown>).stage;
  if (typeof stage === "string" && stage) return stage;
  // Fall back to the span name when the producer hasn't populated
  // `attributes.stage` yet (pre-PR-#294 runs). `decision.model` is the
  // WS-17 rename of the model-call span; `model.call` is kept as a
  // legacy alias for historical exports.
  if (span.name && (span.kind === "decision.model" || span.kind === "model.call")) {
    const lower = span.name.toLowerCase();
    if (lower.includes("trader")) return "trader";
    if (lower.includes("risk")) return "risk";
    if (lower.includes("regime")) return "regime";
  }
  return null;
}

function pickModel(span: RunSpan): string | null {
  if (span.provider && span.model) return `${span.provider}/${span.model}`;
  if (span.model) return span.model;
  return null;
}

function pickToolName(span: RunSpan): string | null {
  // `attributes.tool_name` is reserved per PR #294 even though the
  // engine hasn't backfilled it everywhere yet. The span `name` is
  // the universal fallback for tool.* kinds.
  const attrs = span.attributes ?? {};
  const fromAttr = (attrs as Record<string, unknown>).tool_name;
  if (typeof fromAttr === "string" && fromAttr) return fromAttr;
  if (
    span.kind === "tool.call" ||
    span.kind === "tool.validate_input" ||
    span.kind === "tool.validate_output"
  ) {
    return span.name || null;
  }
  return null;
}

function pickArgSummary(span: RunSpan): string | null {
  if (span.args === undefined || span.args === null) return null;
  if (typeof span.args === "string") return truncate(span.args, 32);
  if (typeof span.args === "object") {
    const keys = Object.keys(span.args as Record<string, unknown>);
    if (keys.length === 0) return null;
    // Render the first 1-2 key=value pairs so the operator sees the
    // shape without the inspector body collapsing to JSON noise.
    const parts: string[] = [];
    for (const k of keys.slice(0, 2)) {
      const v = (span.args as Record<string, unknown>)[k];
      parts.push(`${k}=${formatScalar(v)}`);
    }
    if (keys.length > 2) parts.push("…");
    return parts.join(", ");
  }
  return String(span.args);
}

function pickResultSummary(span: RunSpan): string | null {
  if (span.result === undefined || span.result === null) return null;
  if (Array.isArray(span.result)) {
    return `${span.result.length} rows`;
  }
  if (typeof span.result === "object") {
    const obj = span.result as Record<string, unknown>;
    if (Array.isArray(obj.bars)) {
      const timeframe =
        typeof obj.timeframe === "string" ? obj.timeframe : null;
      return timeframe
        ? `${timeframe}, ${obj.bars.length} rows`
        : `${obj.bars.length} rows`;
    }
    const keys = Object.keys(obj);
    if (keys.length === 0) return null;
    return `${keys.length} fields`;
  }
  return truncate(String(span.result), 32);
}

function formatScalar(v: unknown): string {
  if (v === null || v === undefined) return "null";
  if (typeof v === "string") return truncate(v, 16);
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  return "…";
}

function formatQty(qty: number): string {
  if (!Number.isFinite(qty)) return "—";
  // Trader quantities are typically fractional ETH/BTC; trim to 4 dp
  // and drop trailing zeros so `0.4000` reads as `0.4`.
  const fixed = qty.toFixed(4);
  return fixed.replace(/\.?0+$/, "");
}

function formatPrice(price: number): string {
  if (!Number.isFinite(price)) return "—";
  return price.toFixed(2);
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return `${s.slice(0, max - 1)}…`;
}
