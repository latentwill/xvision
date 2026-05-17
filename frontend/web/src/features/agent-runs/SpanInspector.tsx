// frontend/web/src/features/agent-runs/SpanInspector.tsx
import { useCallback, useState, type ReactNode } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { fetchAgentRunBlob } from "@/api/agent-runs";
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
  const isLiveStreamingModelCall =
    isLive && span.kind === "model.call" && isActiveSseSpan;
  // The header pill keeps showing on the legacy `span.streaming`
  // attribute path (mock fixtures + per-span streaming flag) AND on
  // the new active-SSE-span path. Either is enough to badge it.
  const isStreaming = isLive && (span.streaming || isActiveSseSpan);

  return (
    <div className="w-[400px] shrink-0 flex flex-col">
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
        ) : isStreaming ? (
          <span
            className="ml-auto px-1.5 py-0.5 text-[9px] tracking-[0.16em] font-mono rounded animate-pulse shrink-0"
            style={{
              color: "var(--info)",
              background: withAlpha("#6f8fb8", 0.12),
              border: `1px solid ${withAlpha("#6f8fb8", 0.5)}`,
            }}
          >
            STREAMING
          </span>
        ) : null}
      </div>

      {/* Body */}
      <div className="flex-1 overflow-auto px-3 py-3">
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
                <div className="text-[11px] font-mono text-text-2 break-all">
                  hash: <span className="text-text">{span.hash}</span>
                  <div className="text-text-3 mt-1">
                    hash-only retention — prompt body not stored on disk
                  </div>
                </div>
              )
            }
          />
        ) : null}
        {isLiveStreamingModelCall ? (
          // Preempt the post-hoc RESPONSE fallback while the SSE feed is
          // still delivering `assistant_text_delta` frames. The delta
          // wire carries only `delta_len`, not text — so the indicator
          // shows accumulated character count, not the body. When the
          // stream finishes (span removed from `activeSpanIds`), this
          // block disappears and the existing hash/ref fallback below
          // takes over.
          <PullQuote
            label="RESPONSE"
            accent="var(--info)"
            glyph={"“"}
            italic
            streaming
            body={
              <div
                className="text-[11px] font-mono text-text-2"
                data-testid="span-inspector-streaming"
              >
                Streaming response… ({deltaChars.toLocaleString()} chars)
                <div className="text-text-3 mt-1">
                  body not stored on disk while in-flight — completion
                  appears below when the model call finishes
                </div>
              </div>
            }
          />
        ) : span.response ? (
          <PullQuote label="RESPONSE" body={span.response} accent="var(--gold)" glyph={"“"} italic />
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
                <div className="text-[11px] font-mono text-text-2 break-all">
                  hash: <span className="text-text">{span.response_hash}</span>
                  <div className="text-text-3 mt-1">
                    hash-only retention — completion body not stored on disk
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
          <Row k="cost" v={`$${(span.cost ?? 0).toFixed(4)}`} />
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
