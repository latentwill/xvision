// frontend/web/src/features/agent-runs/live-stream-reducer.ts
//
// WS-8 Part 2 B2 — the LIVE agent-run stream reducer.
//
// Folds one `UnifiedEvent` frame onto the cached `AgentRunDetail` so the trace
// dock stays fully stream-incremental. This REPLACES the old raw-`RunEvent`-
// frame consumer that, on every terminal model/tool/broker frame, fired
// `qc.invalidateQueries` to refetch the canonical export (because it could not
// reconstruct tokens/cost/body/fill/error from the frame alone).
//
// The convergence: the backend now projects each `RunEvent` into a
// `UnifiedEvent` (`RunEventProjector`) and emits it on the wire. This reducer
// runs the SAME fidelity-complete projection the chat-session path uses
// (`stores/session-events.ts::projectSpan` → `TraceDock::projectionToRunSpan`),
// so the model body/tokens/cost, broker fill, tool args/result, decision index,
// engine-event rows, and error all reconstruct from the event payload — NO
// refetch per frame.
//
// The ONLY refetch this reducer asks for is on the terminal `run_finished` /
// `run_interrupted` frame, to pull canonical run-level aggregates
// (span_count / model_call_count / total_cost / retention) the stream does not
// carry. Span DETAIL is never refetched.

import type { UnifiedEvent } from "@/api/unified-events";
import type { AgentRunDetail, RunSpan } from "@/api/types-agent-runs";
import { projectSpan, type SpanProjection } from "@/stores/session-events";
import { projectionToRunSpan } from "./TraceDock";

/**
 * Per-stream accumulator. One instance per open stream connection (reset on
 * run switch / reconnect-snapshot). Holds the `SpanProjection[]` the unified
 * projection folds onto — the dock's `AgentRunDetail.spans` is then derived
 * from it + the snapshot's seed spans.
 */
export type LiveStreamState = {
  /** Folded span projection across all unified frames seen on this stream. */
  projection: SpanProjection[];
};

export type ApplyResult = {
  /**
   * The next cached detail (reference-stable when nothing changed). `null`
   * when there's no cached detail yet (a frame raced ahead of the snapshot);
   * the caller skips the cache write in that case.
   */
  detail: AgentRunDetail | null;
  /** The next accumulator. */
  state: LiveStreamState;
  /**
   * `true` only on a terminal frame, signalling the caller to refetch the
   * canonical run-level aggregates ONCE. Never set for per-span detail frames.
   */
  requestRefetch: boolean;
};

/** Empty per-stream accumulator. */
export function freshLiveStreamState(): LiveStreamState {
  return { projection: [] };
}

/**
 * Payload kinds that close the stream and warrant the single aggregate refetch.
 */
function isTerminal(ev: UnifiedEvent): boolean {
  return (
    ev.payload.kind === "run_finished" || ev.payload.kind === "run_interrupted"
  );
}

/**
 * Merge the unified projection's `RunSpan` rows into the cached detail's
 * `spans`. Seed (snapshot) spans are preserved; a projected span with the same
 * `span_id` REPLACES the matching seed span (reconstructed detail wins — it
 * carries the live tokens/cost/fill the seed didn't have yet); new projected
 * spans (model/tool/broker/engine rows that arrived live) are appended.
 *
 * Engine-event rows have synthetic ids that never collide with real span ids,
 * so they always append. Re-delivered frames project to the same synthetic id
 * (the projection dedupes), so the merge stays idempotent.
 */
function mergeProjectedSpans(
  seedSpans: RunSpan[],
  projection: SpanProjection[],
): RunSpan[] {
  if (projection.length === 0) return seedSpans;
  const projectedById = new Map<string, RunSpan>();
  for (const p of projection) {
    projectedById.set(p.spanId, projectionToRunSpan(p));
  }
  const out: RunSpan[] = [];
  const used = new Set<string>();
  // Preserve seed order; replace in-place where the live projection has detail.
  for (const seed of seedSpans) {
    const projected = projectedById.get(seed.span_id);
    if (projected) {
      out.push(mergeSpan(seed, projected));
      used.add(seed.span_id);
    } else {
      out.push(seed);
    }
  }
  // Append spans the projection produced that weren't in the seed (live-only
  // model/tool/broker/engine rows).
  for (const p of projection) {
    if (used.has(p.spanId)) continue;
    out.push(projectedById.get(p.spanId)!);
  }
  return out;
}

/**
 * Merge a live-projected span onto a seed span. The projection is authoritative
 * for everything it carries (status/finished_at/model body/fill/error/…); the
 * seed contributes only fields the live projection legitimately doesn't know
 * about (e.g. an export-only attribute bag on a pre-seeded span). We prefer the
 * projected value field-by-field, falling back to the seed when the projected
 * field is absent — so a snapshot-seeded span never loses detail, and a live
 * frame never blanks a field it didn't observe.
 */
function mergeSpan(seed: RunSpan, projected: RunSpan): RunSpan {
  return {
    ...seed,
    ...projected,
    // The projection seeds `attributes` to `{}` for lifecycle spans; keep the
    // seed's richer bag when the projection didn't carry one.
    attributes:
      projected.attributes && Object.keys(projected.attributes).length > 0
        ? projected.attributes
        : seed.attributes,
  };
}

/**
 * Fold one `UnifiedEvent` frame onto the cached detail.
 *
 * @param detail current cached `AgentRunDetail` (snapshot-seeded).
 * @param state  per-stream projection accumulator.
 * @param ev     the unified frame.
 */
export function applyUnifiedToDetail(
  detail: AgentRunDetail | null,
  state: LiveStreamState,
  ev: UnifiedEvent,
): ApplyResult {
  // Advance the shared fidelity-complete projection FIRST — even if the
  // snapshot hasn't seeded the cache yet, we must not lose the frame. The
  // accumulated projection is replayed onto the detail once it arrives (the
  // caller re-folds on the next frame after the snapshot resets state). When
  // there's no cached detail we still return the advanced state so nothing is
  // dropped silently.
  const nextProjection = projectSpan(state.projection, ev);
  const projectionChanged = nextProjection !== state.projection;

  if (!detail) {
    return {
      detail: null,
      state: projectionChanged ? { projection: nextProjection } : state,
      requestRefetch: isTerminal(ev),
    };
  }

  let nextDetail: AgentRunDetail = detail;

  // Run-terminal frames flip the summary status off `running` immediately so
  // the header / strip stop showing LIVE before the aggregate refetch lands.
  if (ev.payload.kind === "run_finished") {
    const d = ev.payload.data;
    nextDetail = {
      ...nextDetail,
      summary: {
        ...nextDetail.summary,
        status: d.status,
        finished_at: d.finished_at ?? nextDetail.summary.finished_at,
      },
    };
  } else if (ev.payload.kind === "run_interrupted") {
    const d = ev.payload.data;
    nextDetail = {
      ...nextDetail,
      summary: {
        ...nextDetail.summary,
        status: "interrupted",
        finished_at: d.finished_at ?? nextDetail.summary.finished_at,
      },
    };
  }

  if (projectionChanged) {
    const spans = mergeProjectedSpans(nextDetail.spans, nextProjection);
    if (spans !== nextDetail.spans) {
      nextDetail = { ...nextDetail, spans };
    }
  }

  return {
    detail: nextDetail,
    state: projectionChanged ? { projection: nextProjection } : state,
    requestRefetch: isTerminal(ev),
  };
}
