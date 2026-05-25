// frontend/web/src/stores/message-row-reducer.test.ts
import { describe, expect, it } from "vitest";

import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";
import {
  type AssistantRow,
  type ErrorRow,
  type ToolRow,
  reduceAll,
  reduceRows,
} from "./message-row-reducer";

// ─── Event builders ───────────────────────────────────────────────────────

let seqCounter = 0;

function ev(
  payload: UnifiedPayload,
  overrides: Partial<UnifiedEvent> = {},
): UnifiedEvent {
  const seq = overrides.seq ?? seqCounter++;
  return {
    event_id: overrides.event_id ?? `ev_${seq}`,
    session_id: overrides.session_id ?? "sess_1",
    run_id: overrides.run_id ?? "run_1",
    span_id: overrides.span_id ?? null,
    parent_event_id: null,
    seq,
    ts: "2026-05-24T12:00:00Z",
    scope: { kind: "strategy", id: "strat_abc" },
    actor: overrides.actor ?? "agent",
    source: overrides.source ?? "chat_rail",
    blob_hash: null,
    payload,
  };
}

function assistantRows(rows: ReturnType<typeof reduceRows>): AssistantRow[] {
  return rows.filter((r): r is AssistantRow => r.type === "assistant");
}
function toolRows(rows: ReturnType<typeof reduceRows>): ToolRow[] {
  return rows.filter((r): r is ToolRow => r.type === "tool");
}
function errorRows(rows: ReturnType<typeof reduceRows>): ErrorRow[] {
  return rows.filter((r): r is ErrorRow => r.type === "error");
}

// ─── Tests ────────────────────────────────────────────────────────────────

describe("reduceRows", () => {
  it("is idempotent: applying the same event_id twice is a no-op", () => {
    const e = ev(
      { kind: "assistant_token_delta", text: "hi" },
      { event_id: "ev_dup", seq: 0 },
    );
    const once = reduceRows([], e);
    const twice = reduceRows(once, e);

    // Reference unchanged (the gate short-circuits before any allocation).
    expect(twice).toBe(once);
    const arows = assistantRows(twice);
    expect(arows).toHaveLength(1);
    expect(arows[0].text).toBe("hi");
  });

  it("idempotency holds across a full tool lifecycle replay", () => {
    const events: UnifiedEvent[] = [
      ev({ kind: "tool_requested", span_id: "sp_x", tool_name: "create_strategy", origin: "Native", tool_version: null, tool_hash: null, side_effect_level: "external_write", risk_level: "strategy_mutation", requires_approval: true, is_run_terminator: false, input_hash: "h", input_payload_ref: null }, { event_id: "t_req", seq: 0, span_id: "sp_x" }),
      ev({ kind: "tool_finished", span_id: "sp_x", output_hash: "oh", output_payload_ref: null, exit_code: 0 }, { event_id: "t_fin", seq: 1, span_id: "sp_x" }),
    ];
    const first = reduceAll([], events);
    const replayed = reduceAll(first, events);

    expect(toolRows(replayed)).toHaveLength(1);
    const t = toolRows(replayed)[0];
    expect(t.status).toBe("finished");
    expect(t.outputHash).toBe("oh");
  });

  it("token deltas accumulate into one assistant row", () => {
    let rows = reduceRows([], ev({ kind: "assistant_message_started" }, { event_id: "a0", seq: 0 }));
    rows = reduceRows(rows, ev({ kind: "assistant_token_delta", text: "Hel" }, { event_id: "a1", seq: 1 }));
    rows = reduceRows(rows, ev({ kind: "assistant_token_delta", text: "lo " }, { event_id: "a2", seq: 2 }));
    rows = reduceRows(rows, ev({ kind: "assistant_token_delta", text: "world" }, { event_id: "a3", seq: 3 }));
    rows = reduceRows(rows, ev({ kind: "assistant_message_done", draft_id: "draft_9" }, { event_id: "a4", seq: 4 }));

    const arows = assistantRows(rows);
    expect(arows).toHaveLength(1);
    expect(arows[0].text).toBe("Hello world");
    expect(arows[0].done).toBe(true);
    expect(arows[0].draftId).toBe("draft_9");
    // Row keeps the seq of the creating event.
    expect(arows[0].seq).toBe(0);
  });

  it("out-of-order tool_finished updates only the right span; token deltas never rewrite it", () => {
    // tool_requested for sp_A and sp_B
    let rows = reduceRows([], ev({ kind: "tool_requested", span_id: "sp_A", tool_name: "fetch", origin: "Native", tool_version: null, tool_hash: null, side_effect_level: "read_only", risk_level: "safe_read", requires_approval: false, is_run_terminator: false, input_hash: "h", input_payload_ref: null }, { event_id: "ra", seq: 0, span_id: "sp_A" }));
    rows = reduceRows(rows, ev({ kind: "tool_requested", span_id: "sp_B", tool_name: "write", origin: "Native", tool_version: null, tool_hash: null, side_effect_level: "external_write", risk_level: "file_write", requires_approval: false, is_run_terminator: false, input_hash: "h", input_payload_ref: null }, { event_id: "rb", seq: 1, span_id: "sp_B" }));

    // A LATE tool_finished for sp_A arrives, then an unrelated token delta.
    rows = reduceRows(rows, ev({ kind: "tool_finished", span_id: "sp_A", output_hash: "ohA", output_payload_ref: null, exit_code: 0 }, { event_id: "fa", seq: 2, span_id: "sp_A" }));
    rows = reduceRows(rows, ev({ kind: "assistant_token_delta", text: "thinking" }, { event_id: "d0", seq: 3 }));

    const tA = toolRows(rows).find((t) => t.spanId === "sp_A")!;
    const tB = toolRows(rows).find((t) => t.spanId === "sp_B")!;
    expect(tA.status).toBe("finished");
    expect(tA.outputHash).toBe("ohA");
    // sp_B untouched by the sp_A finish or the token delta.
    expect(tB.status).toBe("requested");
    expect(tB.outputHash).toBeNull();
    // Token delta produced its own assistant row, not a rewrite of a tool row.
    expect(assistantRows(rows)).toHaveLength(1);
    expect(assistantRows(rows)[0].text).toBe("thinking");
  });

  it("an error_* event produces an error row with the typed code", () => {
    const rows = reduceRows(
      [],
      ev(
        {
          kind: "error_missing_capability",
          code: "missing_capability_optimizer",
          message: "agent has no trader capability",
          remediation: "add a trader-capability slot before optimizing",
        },
        { event_id: "err0", seq: 0, actor: "system" },
      ),
    );
    const errs = errorRows(rows);
    expect(errs).toHaveLength(1);
    expect(errs[0].errorKind).toBe("missing_capability");
    expect(errs[0].code).toBe("missing_capability_optimizer");
    expect(errs[0].remediation).toBe(
      "add a trader-capability slot before optimizing",
    );
  });

  it("sidecar_error maps to an error row with the sidecar kind + severity", () => {
    const rows = reduceRows(
      [],
      ev(
        { kind: "sidecar_error", run_id: "run_1", message: "sidecar crashed", severity: "error" },
        { event_id: "sc0", seq: 0, actor: "system" },
      ),
    );
    const errs = errorRows(rows);
    expect(errs).toHaveLength(1);
    expect(errs[0].errorKind).toBe("sidecar");
    expect(errs[0].code).toBe("sidecar_error");
    expect(errs[0].severity).toBe("error");
  });

  it("a tool lifecycle collapses onto ONE row keyed by span_id", () => {
    const lifecycle: UnifiedEvent[] = [
      ev({ kind: "tool_requested", span_id: "sp_1", tool_name: "create_strategy", origin: { Mcp: "xvn" }, tool_version: null, tool_hash: null, side_effect_level: "external_write", risk_level: "strategy_mutation", requires_approval: true, is_run_terminator: false, input_hash: "h", input_payload_ref: null }, { event_id: "l0", seq: 0, span_id: "sp_1" }),
      ev({ kind: "tool_policy_checked", span_id: "sp_1", tool_name: "create_strategy", outcome: "needs_approval", mode: "act" }, { event_id: "l1", seq: 1, span_id: "sp_1" }),
      ev({ kind: "tool_started", span_id: "sp_1" }, { event_id: "l2", seq: 2, span_id: "sp_1" }),
      ev({ kind: "tool_finished", span_id: "sp_1", output_hash: "oh1", output_payload_ref: null, exit_code: 0 }, { event_id: "l3", seq: 3, span_id: "sp_1" }),
    ];
    const rows = reduceAll([], lifecycle);

    const tools = toolRows(rows);
    expect(tools).toHaveLength(1);
    const t = tools[0];
    expect(t.spanId).toBe("sp_1");
    expect(t.id).toBe("tool:sp_1");
    expect(t.toolName).toBe("create_strategy");
    expect(t.policyOutcome).toBe("needs_approval");
    expect(t.policyMode).toBe("act");
    expect(t.status).toBe("finished");
    expect(t.outputHash).toBe("oh1");
    // Row keeps seq of the FIRST (creating) event.
    expect(t.seq).toBe(0);
  });

  it("out-of-order: a late tool_started does NOT regress a finished status", () => {
    let rows = reduceRows([], ev({ kind: "tool_finished", span_id: "sp_z", output_hash: "oh", output_payload_ref: null, exit_code: 0 }, { event_id: "z0", seq: 5, span_id: "sp_z" }));
    rows = reduceRows(rows, ev({ kind: "tool_started", span_id: "sp_z" }, { event_id: "z1", seq: 1, span_id: "sp_z" }));
    const t = toolRows(rows)[0];
    expect(t.status).toBe("finished");
  });

  it("renders rows in seq order regardless of arrival order", () => {
    // Errors arrive out of seq order; output must be seq-sorted.
    let rows = reduceRows([], ev({ kind: "error_missing_tool", code: "c2", message: "second" }, { event_id: "e2", seq: 2, actor: "system" }));
    rows = reduceRows(rows, ev({ kind: "error_invalid_schema", code: "c0", message: "first" }, { event_id: "e0", seq: 0, actor: "system" }));
    rows = reduceRows(rows, ev({ kind: "error_policy_denied", code: "c1", message: "middle" }, { event_id: "e1", seq: 1, actor: "system" }));

    const seqs = rows.map((r) => r.seq);
    expect(seqs).toEqual([0, 1, 2]);
  });

  it("optimizer events collapse onto one row keyed by optimization_id", () => {
    const opt: UnifiedEvent[] = [
      ev({ kind: "optimization_candidate_started", optimization_id: "opt_1", candidate_index: 0, optimizer: "mipro" }, { event_id: "o0", seq: 0, actor: "optimizer" }),
      ev({ kind: "optimization_candidate_metric", optimization_id: "opt_1", candidate_index: 0, metric: "sharpe", value: 1.2, split: "holdout" }, { event_id: "o1", seq: 1, actor: "optimizer" }),
      ev({ kind: "optimization_candidate_selected", optimization_id: "opt_1", candidate_index: 0, optimizer: "mipro" }, { event_id: "o2", seq: 2, actor: "optimizer" }),
      ev({ kind: "optimization_completed", optimization_id: "opt_1", selected_candidate_index: 0, minted_agent_id: "agent_42" }, { event_id: "o3", seq: 3, actor: "optimizer" }),
    ];
    const rows = reduceAll([], opt);
    const optimizers = rows.filter((r) => r.type === "optimizer");
    expect(optimizers).toHaveLength(1);
    const o = optimizers[0];
    if (o.type !== "optimizer") throw new Error("expected optimizer row");
    expect(o.optimizationId).toBe("opt_1");
    expect(o.completed).toBe(true);
    expect(o.selectedCandidateIndex).toBe(0);
    expect(o.mintedAgentId).toBe("agent_42");
    expect(o.metrics["0:sharpe:holdout"]).toBe(1.2);
  });

  it("two interleaved streams keep separate assistant rows", () => {
    let rows = reduceRows([], ev({ kind: "assistant_token_delta", text: "A" }, { event_id: "s1a", seq: 0, session_id: "sess_A", run_id: null }));
    rows = reduceRows(rows, ev({ kind: "assistant_token_delta", text: "B" }, { event_id: "s2a", seq: 0, session_id: "sess_B", run_id: null }));
    rows = reduceRows(rows, ev({ kind: "assistant_token_delta", text: "A2" }, { event_id: "s1b", seq: 1, session_id: "sess_A", run_id: null }));

    const arows = assistantRows(rows);
    expect(arows).toHaveLength(2);
    const a = arows.find((r) => r.streamId === "sess_A")!;
    const b = arows.find((r) => r.streamId === "sess_B")!;
    expect(a.text).toBe("AA2");
    expect(b.text).toBe("B");
  });
});
