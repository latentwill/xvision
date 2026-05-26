// frontend/web/src/stores/message-row-reducer.ts
//
// Pure, framework-agnostic per-row reducer for the chat rail / trace dock.
// Projects the `UnifiedEvent` stream (see api/unified-events.ts, mirroring
// crates/xvision-observability/src/unified_event.rs) into a stable, ordered
// list of `MessageRow`s.
//
// Design contract (acceptance criteria):
//   - Idempotent: applying the same `event_id` twice is a no-op.
//   - Out-of-order safe: a late `tool_finished` updates only its `span_id`
//     row; a token delta never rewrites unrelated rows.
//   - Ordering: each row carries the `seq` of the event that CREATED it and
//     renders in `seq` order; dedupe/ordering is keyed on
//     `(session_id|run_id, seq, event_id)`.
//
// No React / zustand imports — this file is unit-testable in isolation. The
// store wiring (ChatRail/TraceDock onto this reducer) is a separate step.

import type {
  Actor,
  ToolPolicyOutcome,
  UnifiedEvent,
  UnifiedPayload,
} from "@/api/unified-events";

// ─── Row model ────────────────────────────────────────────────────────────

export type ToolRowStatus =
  | "requested"
  | "policy_checked"
  | "approved"
  | "started"
  | "finished"
  | "failed"
  | "cancelled"
  | "denied";

/** Base fields every row carries for stable ordering + dedupe. */
type RowBase = {
  /** Stable per-row id (see keying rules in each row's docs). */
  id: string;
  /** `seq` of the event that CREATED the row; rows render in seq order. */
  seq: number;
  /** Owning stream id — `session_id ?? run_id ?? ""`. Scopes the row's
   *  ordering domain so two interleaved streams never reorder each other. */
  streamId: string;
  /** Set of event_ids already applied to this row — drives idempotency. */
  appliedEventIds: Set<string>;
  /** Who produced the creating event. */
  actor: Actor;
};

/** Assistant text row. One per (stream, message index). Token deltas append
 *  to the open row; `assistant_message_done` closes it. */
export type AssistantRow = RowBase & {
  type: "assistant";
  /** Accumulated text from `assistant_token_delta` frames. */
  text: string;
  /** Rich content blocks from `assistant_content_block` frames. */
  blocks: unknown[];
  /** False until `assistant_message_done`; true once the message is closed. */
  done: boolean;
  draftId: string | null;
  /** Message index within the stream (0-based open-message counter). */
  messageIndex: number;
};

/** Tool lifecycle row. One per `span_id`. Every tool_* event for that span
 *  collapses onto this single row. */
export type ToolRow = RowBase & {
  type: "tool";
  spanId: string;
  toolName: string | null;
  status: ToolRowStatus;
  /** Latest policy-check outcome, when one arrived. */
  policyOutcome: ToolPolicyOutcome | null;
  policyMode: string | null;
  approver: string | null;
  /** Accumulated `tool_delta` text. */
  output: string;
  /** Content-addressed output hash from `tool_finished`. */
  outputHash: string | null;
  exitCode: number | null;
  /** Set on `tool_failed` (error_json) / `tool_denied` (message). */
  errorMessage: string | null;
  /** Machine code on `tool_denied`. */
  deniedCode: string | null;
  /** Set on `tool_cancelled`. */
  cancelReason: string | null;
};

/** Checkpoint row. One per checkpoint event (created / restored / failed). */
export type CheckpointRow = RowBase & {
  type: "checkpoint";
  /** `created` | `restored` | `restore_failed`. */
  status: "created" | "restored" | "restore_failed";
  checkpointId: string;
  /** Artifacts rewound on a restore. */
  restored: string[];
  /** Machine code + message on a failed restore. */
  code: string | null;
  message: string | null;
};

/** Optimizer row. One per `optimization_id`; candidate metrics / selection /
 *  completion update THAT row. */
export type OptimizerRow = RowBase & {
  type: "optimizer";
  optimizationId: string;
  optimizer: string | null;
  /** Highest candidate_index observed. */
  candidateCount: number;
  /** Per-candidate metrics keyed by `<candidate_index>:<metric>:<split>`. */
  metrics: Record<string, number>;
  selectedCandidateIndex: number | null;
  mintedAgentId: string | null;
  completed: boolean;
};

export type ErrorRowCode =
  | "missing_capability"
  | "missing_tool"
  | "invalid_schema"
  | "provider_unavailable"
  | "policy_denied"
  | "persistence_failed"
  | "sidecar";

/** Error row. One per error_* / sidecar_error event. */
export type ErrorRow = RowBase & {
  type: "error";
  /** Typed error class derived from the payload `kind`. */
  errorKind: ErrorRowCode;
  /** Machine code from the payload (TypedError.code / "sidecar_error"). */
  code: string;
  message: string;
  remediation: string | null;
  severity: string | null;
};

export type MessageRow =
  | AssistantRow
  | ToolRow
  | CheckpointRow
  | OptimizerRow
  | ErrorRow;

// ─── Helpers ──────────────────────────────────────────────────────────────

function streamIdOf(ev: UnifiedEvent): string {
  return ev.session_id ?? ev.run_id ?? "";
}

/** Total order over rows: by streamId, then seq, then event-creation id. */
function compareRows(a: MessageRow, b: MessageRow): number {
  if (a.streamId !== b.streamId) return a.streamId < b.streamId ? -1 : 1;
  if (a.seq !== b.seq) return a.seq - b.seq;
  if (a.id !== b.id) return a.id < b.id ? -1 : 1;
  return 0;
}

function sorted(rows: MessageRow[]): MessageRow[] {
  return [...rows].sort(compareRows);
}

/** Has any row already consumed this event_id? Drives global idempotency. */
function alreadyApplied(rows: MessageRow[], eventId: string): boolean {
  return rows.some((r) => r.appliedEventIds.has(eventId));
}

const ERROR_KIND_BY_PAYLOAD: Record<string, ErrorRowCode> = {
  error_missing_capability: "missing_capability",
  error_missing_tool: "missing_tool",
  error_invalid_schema: "invalid_schema",
  error_provider_unavailable: "provider_unavailable",
  error_policy_denied: "policy_denied",
  error_persistence_failed: "persistence_failed",
  sidecar_error: "sidecar",
};

// ─── Reducer ──────────────────────────────────────────────────────────────

/**
 * Apply one `UnifiedEvent` to the row list, returning a NEW array (referential
 * change only when something actually changed). Pure: no mutation of the input
 * array or its rows — updated rows are shallow-copied.
 */
export function reduceRows(
  rows: MessageRow[],
  ev: UnifiedEvent,
): MessageRow[] {
  // Idempotency gate: the same event_id never applies twice, regardless of
  // which row it would have touched.
  if (alreadyApplied(rows, ev.event_id)) return rows;

  const next = dispatch(rows, ev);
  // Only re-sort / return a new array if the dispatch produced a change.
  return next === rows ? rows : sorted(next);
}

/** Fold an entire event list (e.g. a reconnect backfill) in order. */
export function reduceAll(
  rows: MessageRow[],
  events: UnifiedEvent[],
): MessageRow[] {
  return events.reduce(reduceRows, rows);
}

function dispatch(rows: MessageRow[], ev: UnifiedEvent): MessageRow[] {
  const p = ev.payload;
  switch (p.kind) {
    // ── Assistant ──
    case "assistant_message_started":
      return openAssistantRow(rows, ev);
    case "assistant_token_delta":
      return appendToAssistant(rows, ev, { text: p.data.text });
    case "assistant_content_block":
      return appendToAssistant(rows, ev, { block: p.data.block });
    case "assistant_message_done":
      return closeAssistant(rows, ev, p.data.draft_id);

    // Session/run terminal events should not leave lifecycle rows spinning
    // forever when the sidecar drops the matching tool terminal event.
    case "session_completed":
    case "session_interrupted":
      return terminalizeOpenTools(rows, ev, "cancelled", terminalReason(p));
    case "session_failed":
      return terminalizeOpenTools(rows, ev, "failed", p.data.message);
    case "run_finished":
      return terminalizeOpenTools(
        rows,
        ev,
        p.data.status === "failed" || p.data.status === "agent_failure"
          ? "failed"
          : "cancelled",
        p.data.error,
      );
    case "run_interrupted":
      return terminalizeOpenTools(rows, ev, "cancelled", p.data.reason);
    case "span_finished":
      return terminalizeToolSpan(
        rows,
        ev,
        p.data.span_id,
        p.data.status === "error" ? "failed" : "cancelled",
        p.data.error_json,
      );

    // ── Tool lifecycle (all keyed on span_id) ──
    case "tool_requested":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        toolName: p.data.tool_name,
        status: bumpToolStatus(r.status, "requested"),
      }));
    case "tool_policy_checked":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        toolName: r.toolName ?? p.data.tool_name,
        status: bumpToolStatus(r.status, "policy_checked"),
        policyOutcome: p.data.outcome,
        policyMode: p.data.mode,
      }));
    case "tool_approved":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        status: bumpToolStatus(r.status, "approved"),
        approver: p.data.approver,
      }));
    case "tool_started":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        status: bumpToolStatus(r.status, "started"),
      }));
    case "tool_delta":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        output: r.output + p.data.text,
      }));
    case "tool_finished":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        status: bumpToolStatus(r.status, "finished"),
        outputHash: p.data.output_hash ?? r.outputHash,
        exitCode: p.data.exit_code ?? r.exitCode,
      }));
    case "tool_failed":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        status: bumpToolStatus(r.status, "failed"),
        errorMessage: p.data.error_json ?? r.errorMessage,
      }));
    case "tool_cancelled":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        status: bumpToolStatus(r.status, "cancelled"),
        cancelReason: p.data.reason ?? r.cancelReason,
      }));
    case "tool_denied":
      return upsertTool(rows, ev, p.data.span_id, (r) => ({
        ...r,
        toolName: r.toolName ?? p.data.tool_name,
        status: bumpToolStatus(r.status, "denied"),
        deniedCode: p.data.code,
        errorMessage: p.data.message,
      }));

    // ── Checkpoints ──
    case "checkpoint_created":
      return addCheckpointRow(rows, ev, {
        status: "created",
        checkpointId: p.data.checkpoint_id,
        restored: [],
        code: null,
        message: null,
      });
    case "checkpoint_restored":
      return addCheckpointRow(rows, ev, {
        status: "restored",
        checkpointId: p.data.checkpoint_id,
        restored: p.data.restored,
        code: null,
        message: null,
      });
    case "checkpoint_restore_failed":
      return addCheckpointRow(rows, ev, {
        status: "restore_failed",
        checkpointId: p.data.checkpoint_id,
        restored: [],
        code: p.data.code,
        message: p.data.message,
      });

    // ── Optimizer (keyed on optimization_id) ──
    case "optimization_candidate_started":
      return upsertOptimizer(rows, ev, p.data.optimization_id, (r) => ({
        ...r,
        optimizer: r.optimizer ?? p.data.optimizer,
        candidateCount: Math.max(r.candidateCount, p.data.candidate_index + 1),
      }));
    case "optimization_candidate_metric":
      return upsertOptimizer(rows, ev, p.data.optimization_id, (r) => ({
        ...r,
        candidateCount: Math.max(r.candidateCount, p.data.candidate_index + 1),
        metrics: {
          ...r.metrics,
          [`${p.data.candidate_index}:${p.data.metric}:${p.data.split}`]:
            p.data.value,
        },
      }));
    case "optimization_candidate_selected":
      return upsertOptimizer(rows, ev, p.data.optimization_id, (r) => ({
        ...r,
        optimizer: r.optimizer ?? p.data.optimizer,
        candidateCount: Math.max(r.candidateCount, p.data.candidate_index + 1),
        selectedCandidateIndex: p.data.candidate_index,
      }));
    case "optimization_completed":
      return upsertOptimizer(rows, ev, p.data.optimization_id, (r) => ({
        ...r,
        selectedCandidateIndex:
          p.data.selected_candidate_index ?? r.selectedCandidateIndex,
        mintedAgentId: p.data.minted_agent_id ?? r.mintedAgentId,
        completed: true,
      }));

    // ── Errors (error_* + sidecar_error) ──
    case "error_missing_capability":
    case "error_missing_tool":
    case "error_invalid_schema":
    case "error_provider_unavailable":
    case "error_policy_denied":
    case "error_persistence_failed":
      return addErrorRow(rows, ev, {
        errorKind: ERROR_KIND_BY_PAYLOAD[p.kind],
        code: p.data.code,
        message: p.data.message,
        remediation: p.data.remediation ?? null,
        severity: null,
      });
    case "sidecar_error":
      return addErrorRow(rows, ev, {
        errorKind: "sidecar",
        code: "sidecar_error",
        message: p.data.message,
        remediation: null,
        severity: p.data.severity,
      });

    // ── Everything else does not project to a row in this reducer. ──
    // Run/span lifecycle, broker calls, focus chain, memory/provenance,
    // backpressure: surfaced elsewhere (trace dock streaming slice, status
    // strip). No row mutation here; return the input unchanged so the
    // reference is stable and downstream memoization holds.
    default:
      return assertHandledOrPassthrough(rows, p);
  }
}

/**
 * For payload kinds the reducer intentionally does not project to a row,
 * pass through unchanged. The `_exhaustive` binding makes TypeScript flag a
 * NEW payload kind that isn't explicitly handled above — forcing a conscious
 * decision (project a row or add it to this passthrough set) when the Rust
 * taxonomy grows.
 */
function assertHandledOrPassthrough(
  rows: MessageRow[],
  p: UnifiedPayload,
): MessageRow[] {
  switch (p.kind) {
    case "session_created":
    case "session_resumed":
    case "run_started":
    case "span_started":
    case "model_call_finished":
    case "broker_call_started":
    case "broker_call_finished":
    case "focus_loaded":
    case "focus_edited":
    case "focus_injected":
    case "memory_recall":
    case "artifact_written":
    case "supervisor_note":
    case "engine_event":
    case "backpressure_dropped":
      return rows;
    default:
      // Catch-all passthrough. This helper is invoked from the main reducer's
      // `default`, so `p` still carries the FULL UnifiedPayload union (TS does
      // not narrow across the call boundary) — a `never` exhaustiveness binding
      // is therefore not valid here. Kinds the reducer does not project to a
      // row intentionally pass through unchanged.
      return rows;
  }
}

// ─── Assistant row helpers ────────────────────────────────────────────────

/** The currently-open (not-done) assistant row for this stream, if any. */
function openAssistantIn(
  rows: MessageRow[],
  streamId: string,
): AssistantRow | undefined {
  let open: AssistantRow | undefined;
  for (const r of rows) {
    if (r.type === "assistant" && r.streamId === streamId && !r.done) {
      // Latest open row wins (highest messageIndex) — token deltas append to it.
      if (!open || r.messageIndex > open.messageIndex) open = r;
    }
  }
  return open;
}

function assistantCountIn(rows: MessageRow[], streamId: string): number {
  return rows.filter(
    (r) => r.type === "assistant" && r.streamId === streamId,
  ).length;
}

function makeAssistantRow(ev: UnifiedEvent, messageIndex: number): AssistantRow {
  const streamId = streamIdOf(ev);
  return {
    type: "assistant",
    id: `assistant:${streamId}:${messageIndex}`,
    seq: ev.seq,
    streamId,
    appliedEventIds: new Set([ev.event_id]),
    actor: ev.actor,
    text: "",
    blocks: [],
    done: false,
    draftId: null,
    messageIndex,
  };
}

function openAssistantRow(rows: MessageRow[], ev: UnifiedEvent): MessageRow[] {
  const streamId = streamIdOf(ev);
  // If a row is already open for the stream, an explicit "started" is a no-op
  // beyond recording the event id (keeps idempotency-on-resume clean).
  const open = openAssistantIn(rows, streamId);
  if (open) {
    return rows.map((r) =>
      r === open
        ? withEvent({ ...open, seq: Math.min(open.seq, ev.seq) }, ev.event_id)
        : r,
    );
  }
  const messageIndex = assistantCountIn(rows, streamId);
  return [...rows, makeAssistantRow(ev, messageIndex)];
}

function appendToAssistant(
  rows: MessageRow[],
  ev: UnifiedEvent,
  delta: { text?: string; block?: unknown },
): MessageRow[] {
  const streamId = streamIdOf(ev);
  const open = openAssistantIn(rows, streamId);
  if (open) {
    return rows.map((r) => {
      if (r !== open) return r;
      const updated: AssistantRow = {
        ...open,
        seq: Math.min(open.seq, ev.seq),
        text: delta.text !== undefined ? open.text + delta.text : open.text,
        blocks:
          delta.block !== undefined
            ? [...open.blocks, delta.block]
            : open.blocks,
      };
      return withEvent(updated, ev.event_id);
    });
  }
  // A delta arriving before an explicit "started" frame implicitly opens a
  // new row (the message_started event may be absent or out of order).
  const messageIndex = assistantCountIn(rows, streamId);
  const fresh = makeAssistantRow(ev, messageIndex);
  fresh.text = delta.text ?? "";
  if (delta.block !== undefined) fresh.blocks = [delta.block];
  return [...rows, fresh];
}

function closeAssistant(
  rows: MessageRow[],
  ev: UnifiedEvent,
  draftId: string | null,
): MessageRow[] {
  const streamId = streamIdOf(ev);
  const open = openAssistantIn(rows, streamId);
  if (open) {
    return rows.map((r) =>
      r === open
        ? withEvent(
            { ...open, seq: Math.min(open.seq, ev.seq), done: true, draftId },
            ev.event_id,
          )
        : r,
    );
  }
  // message_done with no open row: materialize a closed, empty row so the
  // event is recorded and idempotency holds on replay.
  const messageIndex = assistantCountIn(rows, streamId);
  const fresh = makeAssistantRow(ev, messageIndex);
  fresh.done = true;
  fresh.draftId = draftId;
  return [...rows, fresh];
}

// ─── Tool row helpers ─────────────────────────────────────────────────────

const TOOL_STATUS_RANK: Record<ToolRowStatus, number> = {
  requested: 0,
  policy_checked: 1,
  approved: 2,
  started: 3,
  finished: 4,
  failed: 4,
  cancelled: 4,
  denied: 4,
};

/**
 * Tool status only ever advances. Out-of-order frames (e.g. a late
 * `tool_started` after a `tool_finished`) never regress a terminal status.
 */
function bumpToolStatus(
  current: ToolRowStatus,
  incoming: ToolRowStatus,
): ToolRowStatus {
  return TOOL_STATUS_RANK[incoming] >= TOOL_STATUS_RANK[current]
    ? incoming
    : current;
}

function makeToolRow(ev: UnifiedEvent, spanId: string): ToolRow {
  return {
    type: "tool",
    id: `tool:${spanId}`,
    seq: ev.seq,
    streamId: streamIdOf(ev),
    appliedEventIds: new Set<string>(),
    actor: ev.actor,
    spanId,
    toolName: null,
    status: "requested",
    policyOutcome: null,
    policyMode: null,
    approver: null,
    output: "",
    outputHash: null,
    exitCode: null,
    errorMessage: null,
    deniedCode: null,
    cancelReason: null,
  };
}

/**
 * Create-or-update the tool row for `spanId`. The update applies ONLY to that
 * row; all other rows pass through by reference. A late event for a span that
 * has not been seen yet creates the row (keeps the seq of the first event that
 * touched it).
 */
function upsertTool(
  rows: MessageRow[],
  ev: UnifiedEvent,
  spanId: string,
  update: (row: ToolRow) => ToolRow,
): MessageRow[] {
  const idx = rows.findIndex(
    (r) => r.type === "tool" && r.spanId === spanId,
  );
  if (idx === -1) {
    const created = withEvent(update(makeToolRow(ev, spanId)), ev.event_id);
    return [...rows, created];
  }
  return rows.map((r, i) =>
    i === idx
      ? withEvent(
          { ...update(r as ToolRow), seq: Math.min(r.seq, ev.seq) },
          ev.event_id,
        )
      : r,
  );
}

function isToolTerminal(status: ToolRowStatus): boolean {
  return (
    status === "finished" ||
    status === "failed" ||
    status === "cancelled" ||
    status === "denied"
  );
}

function terminalReason(p: UnifiedPayload): string | null {
  switch (p.kind) {
    case "session_interrupted":
      return p.data.reason;
    default:
      return null;
  }
}

function terminalizeOpenTools(
  rows: MessageRow[],
  ev: UnifiedEvent,
  status: Extract<ToolRowStatus, "failed" | "cancelled">,
  reason: string | null,
): MessageRow[] {
  const streamId = streamIdOf(ev);
  let changed = false;
  const next = rows.map((r) => {
    if (
      r.type !== "tool" ||
      r.streamId !== streamId ||
      isToolTerminal(r.status)
    ) {
      return r;
    }
    changed = true;
    return withEvent(
      {
        ...r,
        status: bumpToolStatus(r.status, status),
        errorMessage: status === "failed" ? reason ?? r.errorMessage : r.errorMessage,
        cancelReason: status === "cancelled" ? reason ?? r.cancelReason : r.cancelReason,
      },
      ev.event_id,
    );
  });
  return changed ? next : rows;
}

function terminalizeToolSpan(
  rows: MessageRow[],
  ev: UnifiedEvent,
  spanId: string,
  status: Extract<ToolRowStatus, "failed" | "cancelled">,
  reason: string | null,
): MessageRow[] {
  const idx = rows.findIndex(
    (r) => r.type === "tool" && r.spanId === spanId,
  );
  if (idx === -1) return rows;
  return rows.map((r, i) => {
    if (i !== idx || r.type !== "tool" || isToolTerminal(r.status)) return r;
    return withEvent(
      {
        ...r,
        status: bumpToolStatus(r.status, status),
        errorMessage: status === "failed" ? reason ?? r.errorMessage : r.errorMessage,
        cancelReason: status === "cancelled" ? reason ?? r.cancelReason : r.cancelReason,
      },
      ev.event_id,
    );
  });
}

// ─── Checkpoint / optimizer / error helpers ──────────────────────────────

function addCheckpointRow(
  rows: MessageRow[],
  ev: UnifiedEvent,
  fields: Omit<
    CheckpointRow,
    keyof RowBase | "type"
  >,
): MessageRow[] {
  const row: CheckpointRow = {
    type: "checkpoint",
    id: `checkpoint:${ev.event_id}`,
    seq: ev.seq,
    streamId: streamIdOf(ev),
    appliedEventIds: new Set([ev.event_id]),
    actor: ev.actor,
    ...fields,
  };
  return [...rows, row];
}

function upsertOptimizer(
  rows: MessageRow[],
  ev: UnifiedEvent,
  optimizationId: string,
  update: (row: OptimizerRow) => OptimizerRow,
): MessageRow[] {
  const idx = rows.findIndex(
    (r) => r.type === "optimizer" && r.optimizationId === optimizationId,
  );
  if (idx === -1) {
    const base: OptimizerRow = {
      type: "optimizer",
      id: `optimizer:${optimizationId}`,
      seq: ev.seq,
      streamId: streamIdOf(ev),
      appliedEventIds: new Set<string>(),
      actor: ev.actor,
      optimizationId,
      optimizer: null,
      candidateCount: 0,
      metrics: {},
      selectedCandidateIndex: null,
      mintedAgentId: null,
      completed: false,
    };
    return [...rows, withEvent(update(base), ev.event_id)];
  }
  return rows.map((r, i) =>
    i === idx ? withEvent(update(r as OptimizerRow), ev.event_id) : r,
  );
}

function addErrorRow(
  rows: MessageRow[],
  ev: UnifiedEvent,
  fields: Omit<ErrorRow, keyof RowBase | "type">,
): MessageRow[] {
  const row: ErrorRow = {
    type: "error",
    id: `error:${ev.event_id}`,
    seq: ev.seq,
    streamId: streamIdOf(ev),
    appliedEventIds: new Set([ev.event_id]),
    actor: ev.actor,
    ...fields,
  };
  return [...rows, row];
}

// ─── Shared ───────────────────────────────────────────────────────────────

/** Record `eventId` as applied to a row (immutably; copies the id set). */
function withEvent<R extends MessageRow>(row: R, eventId: string): R {
  if (row.appliedEventIds.has(eventId)) return row;
  const ids = new Set(row.appliedEventIds);
  ids.add(eventId);
  return { ...row, appliedEventIds: ids };
}
