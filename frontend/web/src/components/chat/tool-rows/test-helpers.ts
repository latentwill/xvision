// frontend/web/src/components/chat/tool-rows/test-helpers.ts
//
// Test-only builders for tool-row tests. Construct sample `UnifiedEvent`s and
// fold them through the real reducer so component tests assert against the
// SAME row shape the rail renders at runtime (no hand-rolled ToolRow fakes
// that could drift from the reducer's projection).

import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";
import { reduceAll } from "@/stores/message-row-reducer";
import type { ToolRow } from "@/stores/message-row-reducer";

let seq = 0;

function ev(payload: UnifiedPayload, over: Partial<UnifiedEvent> = {}): UnifiedEvent {
  seq += 1;
  return {
    event_id: `ev-${seq}-${Math.random().toString(36).slice(2, 8)}`,
    session_id: "sess-1",
    span_id: null,
    seq,
    ts: "2026-05-24T00:00:00Z",
    scope: { kind: "workspace" },
    actor: "agent",
    source: "chat_rail",
    payload,
    ...over,
  };
}

/** A baseline `tool_requested` event for a tool name + side-effect level. */
export function toolRequested(
  spanId: string,
  toolName: string,
  sideEffect: string = "external_write",
): UnifiedEvent {
  return ev({
    kind: "tool_requested",
    data: {
      span_id: spanId,
      tool_name: toolName,
      origin: "Native",
      tool_version: null,
      tool_hash: null,
      side_effect_level: sideEffect,
      risk_level: "strategy_mutation",
      requires_approval: sideEffect === "external_write",
      is_run_terminator: false,
      input_hash: "ih",
      input_payload_ref: null,
    },
  });
}

/** Fold a list of events into the single tool row for `spanId`. */
export function rowFor(spanId: string, events: UnifiedEvent[]): ToolRow {
  const rows = reduceAll([], events);
  const row = rows.find((r) => r.type === "tool" && r.spanId === spanId);
  if (!row || row.type !== "tool") {
    throw new Error(`no tool row for span ${spanId}`);
  }
  return row;
}

/**
 * Build a finished tool row for a tool: requested → started → delta → finished.
 * `output` is the accumulated tool delta text.
 */
export function finishedRow(
  toolName: string,
  opts: {
    spanId?: string;
    sideEffect?: string;
    output?: string;
    outputHash?: string | null;
    exitCode?: number | null;
  } = {},
): ToolRow {
  const spanId = opts.spanId ?? `span-${toolName}`;
  const sideEffect = opts.sideEffect ?? "external_write";
  const events: UnifiedEvent[] = [
    toolRequested(spanId, toolName, sideEffect),
    ev({ kind: "tool_started", data: { span_id: spanId } }),
  ];
  if (opts.output) {
    events.push(
      ev({ kind: "tool_delta", data: { span_id: spanId, text: opts.output } }),
    );
  }
  events.push(
    ev({
      kind: "tool_finished",
      data: {
        span_id: spanId,
        output_hash: opts.outputHash ?? "outhash123456",
        output_payload_ref: null,
        exit_code: opts.exitCode ?? 0,
      },
    }),
  );
  return rowFor(spanId, events);
}

/** Build a tool row stuck at NeedsApproval (requested → policy_checked). */
export function needsApprovalRow(
  toolName: string,
  opts: { spanId?: string; mode?: string } = {},
): ToolRow {
  const spanId = opts.spanId ?? `span-approve-${toolName}`;
  const events: UnifiedEvent[] = [
    toolRequested(spanId, toolName, "external_write"),
    ev({
      kind: "tool_policy_checked",
      data: {
        span_id: spanId,
        tool_name: toolName,
        outcome: "needs_approval",
        mode: opts.mode ?? "act",
      },
    }),
  ];
  return rowFor(spanId, events);
}

/** Build a denied tool row (requested → denied with a code + message). */
export function deniedRow(
  toolName: string,
  opts: { spanId?: string; code?: string; message?: string } = {},
): ToolRow {
  const spanId = opts.spanId ?? `span-deny-${toolName}`;
  const events: UnifiedEvent[] = [
    toolRequested(spanId, toolName, "external_write"),
    ev({
      kind: "tool_denied",
      data: {
        span_id: spanId,
        tool_name: toolName,
        code: opts.code ?? "write_tool_in_research_mode",
        message: opts.message ?? "Write tools are blocked in research mode.",
      },
    }),
  ];
  return rowFor(spanId, events);
}
