// frontend/web/src/features/agent-runs/SpanInspector.tsx
import { useCallback, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import type {
  AgentRunDetail,
  AgentRunSummary,
  BrokerCallDetail,
  RetentionMode,
  RunSpan,
} from "@/api/types-agent-runs";
import { TrajectoryModePill } from "./TrajectoryModeBadge";
import { agentRunKeys, fetchAgentRunBlob } from "@/api/agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { useTraceDock } from "@/stores/trace-dock";
import { useCurrentTraceScope } from "./use-trace-scope";
import { spanColor, withAlpha } from "./span-colors";
import { PullQuote } from "./PullQuote";
import { formatTraceLabel } from "./trace-labels";
import { useQueryClient } from "@tanstack/react-query";

type SpanInspectorProps = {
  span: RunSpan;
  isLive: boolean;
  onRerun: (spanId: string) => void;
  onJumpToDecision: (spanId: string, decisionIdx?: number) => void;
  onCopyJson?: (span: RunSpan) => void;
  /**
   * Optional run-level summary carried down so the inspector can surface
   * trajectory-mode and replay metrics (hit ratio, dropped events,
   * recovery reason) inline on `agent.run` spans. Absent on callers
   * that haven't wired it yet — fields are rendered defensively.
   */
  runSummary?: AgentRunSummary;
  /**
   * Trace-dock density flag (F-7). When `true`, the FIELDS attribute
   * grid collapses to a single-line summary derived from the typed
   * `RunSpan` projection of F-2's `SpanAttributes` bag. Defaults to
   * `false` so callers that haven't wired the dock density toggle
   * keep the existing full-grid behaviour.
   */
  simpleMode?: boolean;
  /**
   * `true` when the currently-rendered span lives in a kind that
   * Simple mode would hide (validate brackets, state.transition). The
   * inspector then renders a notice + a "Switch to Advanced" CTA so
   * the operator can see the span's full context without losing their
   * selection. F-7.
   */
  hiddenInSimpleMode?: boolean;
  /**
   * Callback fired when the operator clicks "Switch to Advanced" on
   * the hidden-span notice. Wired to the dock store's
   * `setAdvancedView(true)` action by the dock owners.
   */
  onRequestAdvanced?: () => void;
};

/**
 * Build the Simple-mode one-line summary from the span's typed
 * projection of F-2's `SpanAttributes` bag. Missing fields are
 * elided rather than rendered as `null` / `undefined` — the
 * operator's only triage signal here is what the run actually
 * carried.
 *
 * Pulled out as a pure function so the unit test surface is small.
 */
export function buildSimpleSummary(span: RunSpan): string {
  const parts: string[] = [];
  parts.push(span.span_id.slice(0, 8));
  parts.push(span.kind);
  // Stage label (agent role) lives on the span as `name` for
  // model.call rows projected upstream, but is most operator-readable
  // through the model identifier when present.
  if (span.provider && span.model) {
    parts.push(`${span.provider}/${span.model}`);
  } else if (span.model) {
    parts.push(span.model);
  }
  if (
    span.kind === "tool.call" ||
    span.kind === "tool.validate_input" ||
    span.kind === "tool.validate_output"
  ) {
    parts.push(`tool=${span.name}`);
  }
  if (span.decision_idx !== undefined) {
    parts.push(`#${span.decision_idx}`);
  }
  return parts.join(" · ");
}

function durationMs(span: RunSpan): number | null {
  if (!span.finished_at) return null;
  return new Date(span.finished_at).getTime() - new Date(span.started_at).getTime();
}

function formatDuration(ms: number): string {
  if (ms >= 60_000) return `${(ms / 60_000).toFixed(1)}m`;
  if (ms >= 1_000) return `${(ms / 1_000).toFixed(1)}s`;
  return `${ms}ms`;
}

/**
 * Reason text for the prompt / response placeholder when no
 * `payload_ref` is available on the span. Returns retention-mode-aware
 * copy so the operator understands why the body is absent.
 */
export function payloadPlaceholderReason(
  retentionMode: RetentionMode | undefined,
  kind: "prompt" | "response",
): string {
  const noun = kind === "prompt" ? "prompt body" : "completion body";
  switch (retentionMode) {
    case "full_debug":
      return `${noun} not captured for this run — re-run to capture`;
    case "redacted":
      return `redacted retention — ${noun} suppressed`;
    case "hash_only":
      return `hash-only retention — ${noun} not stored on disk`;
    default:
      return `${noun} not stored on disk`;
  }
}

/** Back-compat alias — `promptPlaceholderReason("full_debug")` is the
 * historical shape used by the unit test suite. */
export const promptPlaceholderReason = (mode: RetentionMode | undefined): string =>
  payloadPlaceholderReason(mode, "prompt");

/**
 * Inline `<details>` block that fetches the body bytes referenced by
 * `payloadRef` on first expand.
 *
 * F-5 (qa round 7): the visible `<summary>` now renders a human-readable
 * label derived from the span (`formatTraceLabel`), not the raw hash.
 * The hash stays accessible via a small copy button next to the label
 * so operators can still cross-reference. Errors land inline as muted
 * text — no popup, per project UI rule.
 */
function PayloadRefDetails({
  runId,
  payloadRef,
  label,
  testIdPrefix,
}: {
  runId: string | null;
  payloadRef: string;
  /** Human-readable descriptor built by `formatTraceLabel`. */
  label: string;
  testIdPrefix: string;
}) {
  const [phase, setPhase] = useState<
    | { kind: "idle" }
    | { kind: "loading" }
    | { kind: "ready"; body: string }
    | { kind: "error"; message: string }
  >({ kind: "idle" });

  const onToggle = useCallback(
    (e: React.SyntheticEvent<HTMLDetailsElement>) => {
      if (!e.currentTarget.open) return;
      // Only fetch the first time the user opens the details panel.
      if (phase.kind !== "idle") return;
      if (!runId) {
        setPhase({
          kind: "error",
          message: "no active run id — open this span from the run detail page",
        });
        return;
      }
      setPhase({ kind: "loading" });
      fetchAgentRunBlob(runId, payloadRef)
        .then((body) => setPhase({ kind: "ready", body }))
        .catch((err: unknown) => {
          const message =
            err instanceof Error && err.message
              ? err.message
              : "failed to load blob";
          setPhase({ kind: "error", message });
        });
    },
    [phase.kind, runId, payloadRef],
  );

  return (
    <details
      data-testid={`${testIdPrefix}-details`}
      className="text-[11px] font-mono text-text-2 break-all"
      onToggle={onToggle}
    >
      <summary className="cursor-pointer select-none flex items-center gap-2 flex-wrap">
        <span
          data-testid={`${testIdPrefix}-label`}
          className="text-text"
        >
          {label}
        </span>
        <button
          type="button"
          // Stop the click from toggling the parent <details> — the
          // operator wants the hash, not a fetch-on-open side effect.
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            navigator.clipboard?.writeText(payloadRef);
          }}
          title={`copy ref · ${payloadRef}`}
          aria-label="copy payload ref"
          data-testid={`${testIdPrefix}-copy`}
          className="h-5 px-1.5 text-[9px] font-mono tracking-[0.14em] rounded text-text-3 hover:text-text"
          style={{
            background: "transparent",
            border: "1px solid var(--border)",
          }}
        >
          COPY REF
        </button>
      </summary>
      <div className="mt-2 border-l-2 border-border pl-2">
        {phase.kind === "idle" ? (
          <span className="text-text-3">click to load body…</span>
        ) : phase.kind === "loading" ? (
          <span
            data-testid={`${testIdPrefix}-loading`}
            className="text-text-3"
          >
            Loading…
          </span>
        ) : phase.kind === "ready" ? (
          <pre
            data-testid={`${testIdPrefix}-body`}
            className="m-0 whitespace-pre-wrap text-text"
          >
            {phase.body}
          </pre>
        ) : (
          <span
            data-testid={`${testIdPrefix}-error`}
            className="text-text-3"
          >
            could not load body — {phase.message}
          </span>
        )}
      </div>
    </details>
  );
}

const Row = ({ k, v, tone }: { k: string; v: ReactNode; tone?: "gold" }) => (
  <div className="flex items-baseline gap-3 py-1 border-b border-border">
    <div className="w-[100px] shrink-0 text-[10px] uppercase tracking-wider text-text-3">{k}</div>
    <div
      className="flex-1 text-[11px] font-mono tabular-nums break-all"
      style={{ color: tone === "gold" ? "var(--gold)" : "var(--text)" }}
    >
      {v}
    </div>
  </div>
);

export function SpanInspector({
  span,
  isLive,
  onRerun,
  onJumpToDecision,
  onCopyJson,
  simpleMode = false,
  hiddenInSimpleMode = false,
  onRequestAdvanced,
  runSummary,
}: SpanInspectorProps) {
  const color = spanColor(span.kind);
  const ms = durationMs(span);
  // The blob-fetch route is keyed by run id; the inspector itself
  // doesn't carry it as a prop (parent always sets activeRunId on the
  // trace dock when navigating to the run detail page). Read the
  // current route's scope slice so the eval and live surfaces resolve
  // the right run id.
  const scope = useCurrentTraceScope();
  const activeRunId = useTraceDock((s) => s.byScope[scope].activeRunId);
  // Read retention mode from the React Query cache so the placeholder
  // copy is accurate per-run without an extra network request.
  const queryClient = useQueryClient();
  const retentionMode: RetentionMode | undefined = activeRunId
    ? (queryClient.getQueryData<AgentRunDetail>(agentRunKeys.run(activeRunId))
        ?.summary?.retention_mode ?? undefined)
    : undefined;
  // The streaming slice is populated by `agent-runs.ts`'s real SSE
  // branch. Span is considered live-streaming iff the SSE feed has
  // marked it active AND it has not been closed by `span_finished` /
  // `model_call_finished` / a tool terminal event.
  const isActiveSseSpan = useTraceDock((s) =>
    s.streamingState.activeSpanIds.has(span.span_id),
  );
  const deltaChars = useTraceDock(
    (s) => s.streamingState.deltaCharsBySpan[span.span_id] ?? 0,
  );
  // Accumulated assistant body from delta_text frames. Empty when the
  // producer is the legacy chars-only wire — the inspector falls back
  // to the "Streaming…" indicator in that case.
  const streamingBody = useTraceDock(
    (s) => s.streamingState.bodiesBySpan[span.span_id] ?? "",
  );
  // The live RESPONSE PullQuote covers two paths:
  //   - real SSE wire: the span is in `streamingState.activeSpanIds`
  //   - legacy / fixture wire: the span carries a `streaming: true`
  //     flag AND there is no `response_partial` body to render (which
  //     has its own streaming indicator via the PARTIAL PullQuote
  //     below). Without this fallback, a live model.call span with
  //     `streaming: true`, no SSE active entry, and no response body
  //     would render no streaming indicator at all — the dedupe in
  //     PR #264 dropped the SpanInspector header pill that previously
  //     covered it.
  const isLiveStreamingModelCall =
    isLive &&
    span.kind === "model.call" &&
    (isActiveSseSpan ||
      (span.streaming === true && !span.response_partial));

  return (
    <div
      className="w-full min-w-0 flex flex-col h-full"
      data-testid="span-inspector"
      data-span-id={span.span_id}
    >
      {/* Header strip */}
      <div className="px-3 py-2 border-b border-border flex items-center gap-2 min-w-0">
        <span
          className="px-1.5 py-0.5 text-[9px] tracking-[0.16em] font-mono rounded shrink-0"
          style={{
            color: color.hex,
            background: withAlpha(color.hex, 0.08),
            border: `1px solid ${withAlpha(color.hex, 0.4)}`,
          }}
        >
          {color.label}
        </span>
        <span className="text-[11px] font-mono text-text truncate">{span.name}</span>
        {span.status === "error" ? (
          // Error badge rides next to the name so operators see the
          // failed state without scrolling to the body. The full
          // message lands as a pull-quote in the body below.
          // qa-trace-error-surfacing (2026-05-17).
          <span
            data-testid="span-error-badge"
            className="ml-auto px-1.5 py-0.5 text-[9px] tracking-[0.16em] font-mono rounded shrink-0"
            style={{
              color: "var(--danger)",
              background: withAlpha("#FF4D4D", 0.12),
              border: `1px solid ${withAlpha("#FF4D4D", 0.5)}`,
            }}
          >
            ERROR
          </span>
        ) : null}
        {/*
          The streaming indicator now lives next to the PullQuote body
          (the "● STREAMING" label + animated caret in PullQuote.tsx).
          Operator reported a duplicate icon — the header pill repeated
          what the body pull-quote already shows. The ERROR badge above
          stays because there is no body affordance for it.
        */}
      </div>

      {/* Body */}
      <div className="scrollbar-stable flex-1 overflow-x-auto px-3 py-3">
        {span.status === "error" && span.error_message ? (
          // Error pull-quote: the operator's primary debug signal when a
          // span failed. Rendered before prompt/response so it's the
          // first thing scrolled to. Falls back to status alone when
          // no message is attached (e.g. older runs).
          <PullQuote
            label="ERROR"
            body={span.error_message}
            accent="var(--danger)"
            glyph="!"
          />
        ) : null}
        {span.kind === "broker.call" && span.broker_call ? (
          // qa-trace-broker-spans: render the broker submit detail as
          // a key/value pull-quote. Operators look here for short-sale
          // fills (#14 round-2 intake) and broker-side errors.
          <PullQuote
            label="BROKER CALL"
            accent={color.hex}
            glyph="$"
            body={<BrokerCallDetailRows detail={span.broker_call} />}
          />
        ) : null}
        {span.kind === "opti.eval-run" ? (
          // WS-11b: the candidate's nested eval-run drill-link. The OPTI cycle
          // trace surfaces the candidate's PERSISTED eval run as a navigable
          // node under its experiment — this is the only place the operator can
          // jump from "experiment kept/rejected" to "the actual backtest the
          // gate scored". The full span tree of that run is NOT inlined here
          // (a future step); this is a route link to its trace surface. No
          // popup — it's a plain in-flow link per the project UI rule.
          <EvalRunDrillLink
            evalRunId={
              typeof span.attributes.eval_run_id === "string"
                ? span.attributes.eval_run_id
                : null
            }
            accent={color.hex}
          />
        ) : null}
        {span.kind === "agent.decision" ? (
          // QA30: decision spans previously rendered empty. The engine
          // now stamps the entry-state snapshot (asset, bar_ts,
          // mark_price, position_pre) into `attributes_json` at span
          // open so the inspector has something to show even when the
          // close-time payload (action, fill, position_post) hasn't
          // been folded onto the row yet — that flows through the
          // separate `decision_completed` engine event.
          <PullQuote
            label="DECISION"
            accent={color.hex}
            glyph="◆"
            body={<DecisionSpanDetailRows attrs={span.attributes} />}
          />
        ) : null}
        {span.prompt ? (
          <PullQuote label="PROMPT" body={span.prompt} accent={color.hex} glyph="›" />
        ) : span.kind === "model.call" && (span.prompt_payload_ref || span.hash) ? (
          <PullQuote
            label="PROMPT"
            accent={color.hex}
            glyph="›"
            body={
              span.prompt_payload_ref ? (
                <PayloadRefDetails
                  runId={activeRunId}
                  payloadRef={span.prompt_payload_ref}
                  label={formatTraceLabel({
                    span,
                    refKind: "prompt",
                    ref: span.prompt_payload_ref,
                  })}
                  testIdPrefix="span-inspector-prompt-ref"
                />
              ) : (
                <div
                  className="text-[11px] font-mono text-text-2 break-all"
                  data-testid="span-inspector-prompt-placeholder"
                >
                  hash: <span className="text-text">{span.hash}</span>
                  <div
                    className="text-text-3 mt-1"
                    data-testid="span-inspector-prompt-placeholder-reason"
                  >
                    {payloadPlaceholderReason(retentionMode, "prompt")}
                  </div>
                </div>
              )
            }
          />
        ) : null}
        {isLiveStreamingModelCall ? (
          // Preempt the post-hoc RESPONSE fallback while the SSE feed is
          // still delivering `assistant_text_delta` frames. When the
          // wire carries `delta_text` (real streaming dispatchers, or
          // the post-hoc bridge in agent/execute.rs) we render the live
          // body directly. When the producer ships counts only we fall
          // back to a character-count indicator so the user still sees
          // motion. Once the span finishes the existing hash/ref
          // fallback below takes over.
          <PullQuote
            label="RESPONSE"
            accent="var(--info)"
            glyph={"“"}
            italic
            streaming
            body={
              streamingBody ? (
                <pre
                  data-testid="span-inspector-streaming-body"
                  className="m-0 whitespace-pre-wrap text-[11px] font-mono text-text"
                >
                  {streamingBody}
                </pre>
              ) : (
                <div
                  className="text-[11px] font-mono text-text-2"
                  data-testid="span-inspector-streaming"
                >
                  Streaming response… ({deltaChars.toLocaleString()} chars)
                </div>
              )
            }
          />
        ) : span.response ? (
          <PullQuote label="RESPONSE" body={span.response} accent="var(--gold)" glyph={"“"} italic />
        ) : streamingBody ? (
          // Post-hoc fallback: when the span has finished but we still
          // have a `bodiesBySpan` entry (from the engine's post-hoc
          // bridge delta or accumulated streaming chunks), surface it
          // here so the body persists in the inspector even after the
          // model call closed and the live STREAMING pull-quote
          // disappeared. Without this, the body flashed briefly during
          // the active-span window and was then replaced by the
          // hash/ref placeholder — defeating the purpose of plumbing
          // delta_text in the first place.
          <PullQuote
            label="RESPONSE"
            body={
              <pre
                data-testid="span-inspector-posthoc-body"
                className="m-0 whitespace-pre-wrap text-[11px] font-mono text-text"
              >
                {streamingBody}
              </pre>
            }
            accent="var(--gold)"
            glyph={"“"}
            italic
          />
        ) : span.kind === "model.call" && (span.response_payload_ref || span.response_hash) ? (
          <PullQuote
            label="RESPONSE"
            accent="var(--gold)"
            glyph={"“"}
            body={
              span.response_payload_ref ? (
                <PayloadRefDetails
                  runId={activeRunId}
                  payloadRef={span.response_payload_ref}
                  label={formatTraceLabel({
                    span,
                    refKind: "response",
                    ref: span.response_payload_ref,
                  })}
                  testIdPrefix="span-inspector-response-ref"
                />
              ) : (
                <div
                  className="text-[11px] font-mono text-text-2 break-all"
                  data-testid="span-inspector-response-placeholder"
                >
                  hash: <span className="text-text">{span.response_hash}</span>
                  <div
                    className="text-text-3 mt-1"
                    data-testid="span-inspector-response-placeholder-reason"
                  >
                    {payloadPlaceholderReason(retentionMode, "response")}
                  </div>
                </div>
              )
            }
          />
        ) : null}
        {span.response_partial ? (
          <PullQuote label="RESPONSE (PARTIAL)" body={span.response_partial} accent="var(--info)" glyph={"“"} italic streaming />
        ) : null}
        {span.args !== undefined ? (
          <PullQuote
            label="TOOL ARGS"
            accent={color.hex}
            glyph="›"
            body={
              <pre className="m-0 text-[11px] font-mono whitespace-pre-wrap text-text-2">
                {JSON.stringify(span.args, null, 2)}
              </pre>
            }
          />
        ) : null}
        {span.result !== undefined ? (
          <PullQuote
            label="TOOL RESULT"
            accent="var(--gold)"
            glyph="←"
            body={
              <pre className="m-0 text-[11px] font-mono whitespace-pre-wrap text-text">
                {JSON.stringify(span.result, null, 2)}
              </pre>
            }
          />
        ) : null}

        {/* Trajectory / replay section — only on agent.run spans and only when
            the run summary carries trajectory_mode. Rendered inline before the
            FIELDS grid so replay metrics are immediately visible without
            scrolling. Omitted on all other span kinds and on pre-migration runs
            that never set trajectory_mode. No popup / popover. */}
        {span.kind === "agent.run" && runSummary?.trajectory_mode ? (
          <div
            className="mb-3"
            data-testid="span-inspector-trajectory-section"
          >
            <div className="text-[9px] font-mono tracking-[0.18em] text-text-3 mb-1">
              TRAJECTORY
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <TrajectoryModePill mode={runSummary.trajectory_mode} />
              {runSummary.trajectory_mode === "replay" &&
                runSummary.replay_hit_ratio != null && (
                  <span
                    data-testid="span-inspector-hit-ratio"
                    className="text-[11px] font-mono text-text-2"
                    title="Fraction of model-call steps served from recorded frames"
                  >
                    hit {(runSummary.replay_hit_ratio * 100).toFixed(0)}%
                  </span>
                )}
              {runSummary.dropped_events != null &&
                runSummary.dropped_events > 0 && (
                  <span
                    data-testid="span-inspector-dropped-events"
                    className="text-[11px] font-mono"
                    style={{ color: "var(--danger)" }}
                    title="Events dropped due to buffer pressure"
                  >
                    {runSummary.dropped_events} dropped
                  </span>
                )}
              {runSummary.recovery_reason ? (
                <span
                  data-testid="span-inspector-recovery-reason"
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    padding: "1px 6px",
                    borderRadius: 999,
                    background: "rgba(219,146,48,0.10)",
                    border: "1px solid rgba(219,146,48,0.40)",
                    fontFamily:
                      "var(--font-mono, ui-monospace, monospace)",
                    fontSize: 10,
                    color: "var(--warn)",
                  }}
                >
                  <span aria-hidden="true" style={{ fontSize: 9 }}>!</span>
                  {runSummary.recovery_reason === "replay_divergence"
                    ? "replay diverged"
                    : runSummary.recovery_reason === "replay_frames_exhausted"
                      ? "frames exhausted"
                      : runSummary.recovery_reason}
                </span>
              ) : null}
            </div>
          </div>
        ) : null}

        {simpleMode ? (
          // Simple-mode collapse (F-7). One line derived from the
          // typed projection of F-2's SpanAttributes bag. Operators
          // triaging a run want agent · model · tool · decision at a
          // glance; the full FIELDS grid lives in Advanced.
          <div className="mt-4 pt-1" data-testid="span-inspector-fields-simple">
            <div className="text-[9px] font-mono tracking-[0.18em] text-text-3 mb-1">FIELDS</div>
            <div className="text-[11px] font-mono text-text break-all">
              {buildSimpleSummary(span)}
            </div>
            {hiddenInSimpleMode ? (
              <div className="mt-2 flex items-center gap-2 text-[11px] font-mono text-text-3">
                <span>
                  this span is hidden in Simple mode (kind: {span.kind})
                </span>
                <button
                  type="button"
                  data-testid="span-inspector-switch-advanced"
                  onClick={() => onRequestAdvanced?.()}
                  className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] rounded"
                  style={{
                    background: "var(--surface-elev)",
                    border: "1px solid var(--border)",
                    color: "var(--text)",
                  }}
                >
                  SWITCH TO ADVANCED
                </button>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="mt-4 pt-1" data-testid="span-inspector-fields-advanced">
            <div className="text-[9px] font-mono tracking-[0.18em] text-text-3 mb-1">FIELDS</div>
            <Row k="span.id" v={span.span_id} />
            <Row k="kind" v={span.kind} />
            <Row k="duration" v={ms != null ? formatDuration(ms) : "—"} />
            <Row k="start" v={span.started_at} />
            {span.provider ? <Row k="provider" v={span.provider} /> : null}
            {span.model ? <Row k="model" v={span.model} tone="gold" /> : null}
            {span.tokens_in !== undefined ? (
              <Row k="tokens.in" v={span.tokens_in.toLocaleString()} />
            ) : null}
            {span.tokens_out !== undefined ? (
              <Row k="tokens.out" v={span.tokens_out.toLocaleString()} />
            ) : null}
            <Row
              k="cost"
              v={
                <span title={formatCostUsdPrecise(span.cost ?? 0)}>
                  {formatCostUsd(span.cost ?? 0)}
                </span>
              }
            />
            {span.hash ? <Row k="prompt.hash" v={span.hash} /> : null}
            {span.response_hash ? <Row k="response.hash" v={span.response_hash} /> : null}
            {span.prompt_payload_ref ? <Row k="prompt.ref" v={span.prompt_payload_ref} /> : null}
            {span.response_payload_ref ? (
              <Row k="response.ref" v={span.response_payload_ref} />
            ) : null}
            {span.decision_idx !== undefined ? (
              <Row k="decision" v={`#${span.decision_idx}`} tone="gold" />
            ) : null}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="p-2 grid grid-cols-1 gap-1 border-t border-border">
        {span.decision_idx !== undefined ? (
          <button
            type="button"
            className="h-7 px-2 text-[11px] font-mono text-left text-text rounded flex items-center gap-2"
            style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}
            onClick={() => onJumpToDecision(span.span_id, span.decision_idx)}
          >
            <span style={{ color: "var(--gold)" }}>↧</span>
            {` Jump to decision #${span.decision_idx}`}
          </button>
        ) : null}
        <button
          type="button"
          className="h-7 px-2 text-[11px] font-mono text-left rounded flex items-center gap-2"
          style={
            isLive
              ? { background: "transparent", color: "var(--text-4)", cursor: "not-allowed", border: "1px solid var(--border)" }
              : { background: "var(--surface-elev)", border: "1px solid var(--border)", color: "var(--text)" }
          }
          disabled={isLive}
          title={isLive ? "Disabled — strategy is currently executing" : undefined}
          onClick={() => onRerun(span.span_id)}
        >
          ↻ Rerun from here
          {isLive ? (
            <span className="ml-auto text-[9px] text-text-4 tracking-wider">LOCKED · LIVE</span>
          ) : null}
        </button>
        <button
          type="button"
          className="h-7 px-2 text-[11px] font-mono text-left text-text rounded flex items-center gap-2"
          style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}
          onClick={() => {
            onCopyJson?.(span);
            navigator.clipboard?.writeText(JSON.stringify(span, null, 2));
          }}
        >
          ⧉ Copy span JSON
        </button>
      </div>
    </div>
  );
}

/**
 * WS-11b — the navigable eval-run drill-link rendered on an `opti.eval-run`
 * span in the optimizer cycle trace. Links to the candidate's persisted eval
 * run trace at `/agent-runs/:runId`. Falls back to muted copy when no run id is
 * present (defensive — the reducer only emits this node when a run id exists).
 * No popup: a plain in-flow `<Link>`, consistent with the project UI rule.
 */
function EvalRunDrillLink({
  evalRunId,
  accent,
}: {
  evalRunId: string | null;
  accent: string;
}) {
  if (!evalRunId) {
    return (
      <div
        data-testid="span-inspector-eval-run-missing"
        className="mb-3 text-[11px] font-mono text-text-3"
      >
        eval run id not available for this candidate
      </div>
    );
  }
  return (
    <div className="mb-3" data-testid="span-inspector-eval-run">
      <div className="text-[9px] font-mono tracking-[0.18em] text-text-3 mb-1">
        EVAL RUN
      </div>
      <Link
        to={`/agent-runs/${encodeURIComponent(evalRunId)}`}
        data-testid="span-inspector-eval-run-link"
        className="inline-flex items-center gap-2 h-7 px-2 text-[11px] font-mono rounded"
        style={{
          background: withAlpha(accent, 0.1),
          border: `1px solid ${withAlpha(accent, 0.4)}`,
          color: "var(--text)",
        }}
      >
        <span aria-hidden style={{ color: accent }}>
          ↗
        </span>
        View eval-run trace
        <span className="text-text-3 break-all">{evalRunId}</span>
      </Link>
    </div>
  );
}

function BrokerCallDetailRows({ detail }: { detail: BrokerCallDetail }) {
  const fmt = (n: number | null | undefined, digits = 4) =>
    n == null || !Number.isFinite(n) ? "—" : n.toFixed(digits);
  const outcomeColor =
    detail.outcome === "filled"
      ? "var(--gold)"
      : detail.outcome === "cancelled"
        ? "var(--text-3)"
        : detail.outcome != null
          ? "var(--danger)"
          : "var(--info)";
  return (
    <dl
      data-testid="span-inspector-broker-call"
      className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-[11px] font-mono"
    >
      <dt className="text-text-3">side</dt>
      <dd className="text-text uppercase tracking-[0.12em]">{detail.side}</dd>
      <dt className="text-text-3">symbol</dt>
      <dd className="text-text">{detail.symbol}</dd>
      <dt className="text-text-3">qty</dt>
      <dd className="text-text">{fmt(detail.qty, 6)}</dd>
      <dt className="text-text-3">intended</dt>
      <dd className="text-text">{fmt(detail.intended_price, 4)}</dd>
      <dt className="text-text-3">type</dt>
      <dd className="text-text-2">{detail.order_type}</dd>
      <dt className="text-text-3">venue</dt>
      <dd className="text-text-2">{detail.venue}</dd>
      {detail.idempotency_key ? (
        <>
          <dt className="text-text-3">key</dt>
          <dd className="text-text-2 break-all">{detail.idempotency_key}</dd>
        </>
      ) : null}
      <dt className="text-text-3">outcome</dt>
      <dd style={{ color: outcomeColor }}>
        {detail.outcome ?? "in_progress"}
      </dd>
      {detail.severity ? (
        <>
          <dt className="text-text-3">severity</dt>
          <dd
            data-testid="span-inspector-broker-severity"
            style={{
              color:
                detail.severity === "warn"
                  ? "var(--warn)"
                  : "var(--danger)",
            }}
          >
            {detail.severity === "warn"
              ? "warn — agent received feedback"
              : "error — run terminated"}
          </dd>
        </>
      ) : null}
      {detail.outcome === "filled" || detail.fill_price != null ? (
        <>
          <dt className="text-text-3">fill px</dt>
          <dd className="text-text">{fmt(detail.fill_price, 4)}</dd>
          <dt className="text-text-3">fill qty</dt>
          <dd className="text-text">{fmt(detail.fill_qty, 6)}</dd>
          <dt className="text-text-3">fee</dt>
          <dd className="text-text-2">{fmt(detail.fee, 6)}</dd>
        </>
      ) : null}
      {detail.broker_order_id ? (
        <>
          <dt className="text-text-3">order id</dt>
          <dd className="text-text-2 break-all">{detail.broker_order_id}</dd>
        </>
      ) : null}
      {detail.error_class ? (
        <>
          <dt className="text-text-3">err class</dt>
          <dd style={{ color: "var(--danger)" }}>{detail.error_class}</dd>
        </>
      ) : null}
      {detail.error_message ? (
        <>
          <dt className="text-text-3">err msg</dt>
          <dd className="text-text-2 break-all">{detail.error_message}</dd>
        </>
      ) : null}
    </dl>
  );
}

/**
 * QA30 — render the `agent.decision` span's pre-decision snapshot off
 * its `attributes_json` bag: asset, bar timestamp, mark price, and the
 * position the trader was holding when the cycle opened. The
 * post-decision payload (action, fill, position_post, realized_pnl)
 * lives in the matching `decision_completed` engine event and is shown
 * by the trace dock's event timeline; this component is the inspector
 * surface for what the span itself carries.
 */
function DecisionSpanDetailRows({
  attrs,
}: {
  attrs: Record<string, unknown>;
}) {
  const fmt = (n: unknown, digits = 4): string => {
    if (typeof n !== "number" || !Number.isFinite(n)) return "—";
    return n.toFixed(digits);
  };
  const asset = typeof attrs.asset === "string" ? attrs.asset : null;
  const barTs = typeof attrs.bar_ts === "string" ? attrs.bar_ts : null;
  const markPrice = attrs.mark_price;
  const positionPre = attrs.position_pre;
  const decisionIndex =
    typeof attrs.decision_index === "number" ? attrs.decision_index : null;
  return (
    <dl
      data-testid="span-inspector-decision"
      className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-[11px] font-mono"
    >
      {decisionIndex != null ? (
        <>
          <dt className="text-text-3">cycle</dt>
          <dd className="text-text">#{decisionIndex}</dd>
        </>
      ) : null}
      {asset ? (
        <>
          <dt className="text-text-3">asset</dt>
          <dd className="text-text">{asset}</dd>
        </>
      ) : null}
      {barTs ? (
        <>
          <dt className="text-text-3">bar</dt>
          <dd className="text-text-2 break-all">{barTs}</dd>
        </>
      ) : null}
      <dt className="text-text-3">mark px</dt>
      <dd className="text-text">{fmt(markPrice, 4)}</dd>
      <dt className="text-text-3">position pre</dt>
      <dd className="text-text">{fmt(positionPre, 6)}</dd>
    </dl>
  );
}
