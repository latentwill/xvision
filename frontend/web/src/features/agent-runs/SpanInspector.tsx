// frontend/web/src/features/agent-runs/SpanInspector.tsx
import { useCallback, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type {
  AgentRunDetail,
  BrokerCallDetail,
  RetentionMode,
  RunSpan,
} from "@/api/types-agent-runs";
import { agentRunKeys, fetchAgentRunBlob } from "@/api/agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { useTraceDock } from "@/stores/trace-dock";
import { spanColor, withAlpha } from "./span-colors";
import { PullQuote } from "./PullQuote";

type SpanInspectorProps = {
  span: RunSpan;
  isLive: boolean;
  onRerun: (spanId: string) => void;
  onJumpToDecision: (spanId: string, decisionIdx?: number) => void;
  onCopyJson?: (span: RunSpan) => void;
};

function durationMs(span: RunSpan): number | null {
  if (!span.finished_at) return null;
  return new Date(span.finished_at).getTime() - new Date(span.started_at).getTime();
}

/**
 * Reason text for the prompt / response placeholder when no
 * `payload_ref` is available on the span. Keyed on the run's
 * retention mode so the inspector doesn't lie under `full_debug`
 * (operator 2026-05-18: "prompts still redacted despite full_debug
 * while responses appear").
 *
 * `full_debug` rows currently land here for two distinct reasons:
 *   1. The run pre-dates the producer-side payload-write fix
 *      (queue note `qa-retention-prompt-storage-bug__*__producer-
 *      never-writes-payload-refs`). Re-running captures the body.
 *   2. The producer-side fix has shipped but this specific span
 *      pre-dated the deploy. Same remediation: re-run.
 * The copy covers both with one operator-readable line.
 *
 * `kind` selects the right noun: "prompt body" vs "completion body".
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
      // Unknown / undefined retention mode (cache miss, older summary
      // shape): keep a neutral copy. Better than lying about a
      // specific mode we can't verify.
      return `${noun} not stored on disk`;
  }
}

/** Back-compat alias — `promptPlaceholderReason("full_debug")` is the
 * historical shape used by the unit test suite. */
export const promptPlaceholderReason = (mode: RetentionMode | undefined): string =>
  payloadPlaceholderReason(mode, "prompt");

/**
 * Inline `<details>` block that fetches the body bytes referenced by
 * `payloadRef` on first expand. The ref string is the visible summary
 * so operators can still see / copy / paste it. Errors land inline as
 * muted text — no popup, per project UI rule.
 */
function PayloadRefDetails({
  runId,
  payloadRef,
  testIdPrefix,
}: {
  runId: string | null;
  payloadRef: string;
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
      <summary className="cursor-pointer select-none">
        payload ref: <span className="text-text">{payloadRef}</span>
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
}: SpanInspectorProps) {
  const color = spanColor(span.kind);
  const ms = durationMs(span);
  // The blob-fetch route is keyed by run id; the inspector itself
  // doesn't carry it as a prop (parent always sets activeRunId on the
  // trace dock when navigating to the run detail page).
  const activeRunId = useTraceDock((s) => s.activeRunId);
  // Read the active run's retention mode out of the TanStack Query
  // cache so the prompt-placeholder copy below tells the truth about
  // *why* the body isn't on screen. Without this, the inspector
  // hardcoded "hash-only retention — prompt body not stored on disk"
  // even for runs configured with `full_debug`, which misled the
  // operator (2026-05-18: "prompts still redacted despite full_debug
  // while responses appear"). See queue note
  // `qa-retention-prompt-storage-bug__*__producer-never-writes-payload-refs`
  // for the underlying producer-side gap (out of this contract's
  // scope; tracked in the harness wave).
  const queryClient = useQueryClient();
  const retentionMode: RetentionMode | undefined = activeRunId
    ? queryClient.getQueryData<AgentRunDetail>(agentRunKeys.run(activeRunId))?.summary
        .retention_mode
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
    <div className="w-full min-w-0 flex flex-col h-full">
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
              background: withAlpha("#c8443a", 0.12),
              border: `1px solid ${withAlpha("#c8443a", 0.5)}`,
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

        <div className="mt-4 pt-1">
          <div className="text-[9px] font-mono tracking-[0.18em] text-text-3 mb-1">FIELDS</div>
          <Row k="span.id" v={span.span_id} />
          <Row k="kind" v={span.kind} />
          <Row k="duration" v={ms != null ? `${ms}ms` : "—"} />
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
