// frontend/web/src/features/agent-runs/live-stream-reducer.test.ts
//
// GOLDEN never-go-dark test for WS-8 Part 2 B2.
//
// B2 converges the LIVE agent-run stream onto the `UnifiedEvent` envelope and
// makes the dock fully stream-incremental — DROPPING the per-frame
// `invalidateQueries` refetch that the old raw-`RunEvent`-frame path used to
// reconstruct model tokens/cost/body, broker fill, tool I/O, and errors.
//
// The contract: a representative LIVE sequence driven as `UnifiedEvent` frames
// through `applyUnifiedToDetail` (the reducer the dock runs on every `unified`
// frame) must produce a cached `AgentRunDetail` whose spans render IDENTICAL
// rows + inspector detail to what the OLD raw-frame + export-refetch path
// produced for the same logical events — with NO refetch round-trip. If any
// inspector field can't be reconstructed from the event payload, B2 is blocked
// (see the WU spec's fallbacks).

import { describe, expect, test } from "vitest";

import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";
import type { AgentRunDetail } from "@/api/types-agent-runs";
import {
  applyUnifiedToDetail,
  type LiveStreamState,
} from "./live-stream-reducer";

const RUN = "run_live_golden";

// ─── Minimal seed snapshot (what the SSE `snapshot` frame would carry) ───────
//
// A live run starts with a root span open; the decision/model/tool/broker spans
// arrive as live frames. This mirrors the export the dock seeds from.
function seedDetail(): AgentRunDetail {
  return {
    summary: {
      run_id: RUN,
      objective: "golden parity",
      strategy_id: "strat_1",
      agent_id: null,
      started_at: "2026-06-14T10:00:00.000Z",
      finished_at: null,
      status: "running",
      span_count: 1,
      model_call_count: 0,
      tool_call_count: 0,
      error_count: 0,
      total_cost_usd: 0,
      total_input_tokens: 0,
      total_output_tokens: 0,
      duration_ms: null,
      financial_eval_id: null,
      retention_mode: "full_debug",
    },
    spans: [
      {
        span_id: "span_root",
        parent_span_id: null,
        name: "agent.run",
        kind: "agent.run",
        started_at: "2026-06-14T10:00:00.000Z",
        finished_at: null,
        status: "in_progress",
        attributes: {},
      },
    ],
    model_calls: [],
    tool_calls: [],
  };
}

// ─── UnifiedEvent frame builder ──────────────────────────────────────────────

function frame(
  seq: number,
  span_id: string | null,
  ts: string,
  payload: UnifiedPayload,
): UnifiedEvent {
  return {
    event_id: `ev_${seq}`,
    run_id: RUN,
    span_id: span_id ?? undefined,
    seq,
    ts,
    scope: { kind: "run", id: RUN },
    actor: "agent",
    source: "agent_run",
    payload,
  };
}

/**
 * The representative live sequence the WU calls for:
 *   run_started → span_started(decision) → model_call_finished(tokens/cost/body)
 *   → tool_call_finished → tool_call_failed → broker_call_finished(fill)
 *   → engine_event(risk_veto) → span_finished → run_finished.
 *
 * Plus the broker_call_started + tool_requested frames that carry the submit
 * detail / tool args the inspector reconstructs from (the wire emits these too).
 */
function liveSequence(): UnifiedEvent[] {
  return [
    // run_started — lifecycle only.
    frame(0, null, "2026-06-14T10:00:00.100Z", {
      kind: "run_started",
      data: {
        run_id: RUN,
        objective: "golden parity",
        strategy_id: "strat_1",
        eval_run_id: null,
        source_cli_job_id: null,
        started_at: "2026-06-14T10:00:00.000Z",
        retention_mode: "full_debug",
        sidecar_version: null,
        cline_sdk_version: null,
        protocol_version: null,
        skills_json: null,
        mcp_servers_json: null,
      },
    }),
    // span_started(decision) — the model decision span.
    frame(1, "span_decision", "2026-06-14T10:00:00.200Z", {
      kind: "span_started",
      data: {
        span_id: "span_decision",
        run_id: RUN,
        parent_span_id: "span_root",
        kind: "decision.model",
        name: "anthropic/claude",
        started_at: "2026-06-14T10:00:00.200Z",
        otel_trace_id: null,
        otel_span_id: null,
        attributes_json: null,
      },
    }),
    // model_call_finished — tokens/cost/body/hashes/refs.
    frame(2, "span_decision", "2026-06-14T10:00:01.000Z", {
      kind: "model_call_finished",
      data: {
        span_id: "span_decision",
        provider: "anthropic",
        model: "claude-opus",
        input_token_count: 1234,
        output_token_count: 56,
        cost_usd: 0.0421,
        prompt_hash: "sha256:prompt",
        response_hash: "sha256:resp",
        prompt_text: "decide whether to trade BTC",
        response_text: '{"action":"buy","qty":0.1}',
        prompt_payload_ref: "blob://prompts/p1",
        response_payload_ref: "blob://responses/r1",
        tool_calls_requested: null,
        capability_path: null,
      },
    }),
    // tool_requested — carries the tool args body (input_text).
    frame(3, "span_tool_ok", "2026-06-14T10:00:01.100Z", {
      kind: "tool_requested",
      data: {
        span_id: "span_tool_ok",
        tool_name: "fetch_price",
        origin: { Mcp: "xvn" },
        tool_version: null,
        tool_hash: null,
        side_effect_level: "read_only",
        risk_level: "none",
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:in",
        input_payload_ref: null,
        input_text: '{"symbol":"BTC"}',
      },
    }),
    // tool_call_finished — carries the tool result body (output_text).
    frame(4, "span_tool_ok", "2026-06-14T10:00:01.300Z", {
      kind: "tool_finished",
      data: {
        span_id: "span_tool_ok",
        output_hash: "sha256:out",
        output_payload_ref: null,
        exit_code: 0,
        output_text: '{"price":60000}',
      },
    }),
    // tool_call_failed — carries the error.
    frame(5, "span_tool_bad", "2026-06-14T10:00:01.400Z", {
      kind: "tool_failed",
      data: {
        span_id: "span_tool_bad",
        error_json: JSON.stringify({ message: "tool timed out after 30s" }),
      },
    }),
    // broker_call_started — seeds the broker submit detail + idempotency key.
    frame(6, "span_broker", "2026-06-14T10:00:01.500Z", {
      kind: "broker_call_started",
      data: {
        span_id: "span_broker",
        run_id: RUN,
        side: "buy",
        symbol: "BTC/USD",
        qty: 0.1,
        intended_price: 60000,
        order_type: "market",
        venue: "paper",
        idempotency_key: `${RUN}-7`,
      },
    }),
    // broker_call_finished — the fill.
    frame(7, "span_broker", "2026-06-14T10:00:01.700Z", {
      kind: "broker_call_finished",
      data: {
        span_id: "span_broker",
        outcome: "filled",
        fill_price: 60010,
        fill_qty: 0.1,
        fee: 0.02,
        broker_order_id: "ord_live_1",
        error_class: null,
        error_message: null,
        severity: null,
      },
    }),
    // engine_event(risk_veto) — scoped under the broker span.
    frame(8, "span_broker", "2026-06-14T10:00:01.800Z", {
      kind: "engine_event",
      data: {
        run_id: RUN,
        span_id: "span_broker",
        kind: "risk_veto",
        payload_json: JSON.stringify({ reason: "max_drawdown" }),
        created_at: "2026-06-14T10:00:01.800Z",
      },
    }),
    // span_finished(decision).
    frame(9, "span_decision", "2026-06-14T10:00:02.000Z", {
      kind: "span_finished",
      data: {
        span_id: "span_decision",
        ended_at: "2026-06-14T10:00:02.000Z",
        status: "ok",
        error_json: null,
      },
    }),
    // run_finished — terminal.
    frame(10, null, "2026-06-14T10:00:02.100Z", {
      kind: "run_finished",
      data: {
        run_id: RUN,
        finished_at: "2026-06-14T10:00:02.100Z",
        status: "completed",
        final_artifact_id: null,
        error: null,
      },
    }),
  ];
}

function driveSequence(): { detail: AgentRunDetail; refetched: boolean } {
  let detail = seedDetail();
  let state: LiveStreamState = { projection: [] };
  let refetched = false;
  for (const ev of liveSequence()) {
    const out = applyUnifiedToDetail(detail, state, ev);
    // The sequence always seeds a non-null detail, so the reducer never
    // returns null here.
    detail = out.detail!;
    state = out.state;
    if (out.requestRefetch) refetched = true;
  }
  return { detail, refetched };
}

describe("applyUnifiedToDetail — golden never-go-dark parity (WS-8 B2)", () => {
  test("the model decision span reconstructs tokens/cost/body/hashes/refs with NO refetch", () => {
    const { detail } = driveSequence();
    const span = detail.spans.find((s) => s.span_id === "span_decision");
    expect(span).toBeDefined();
    // Inspector model rows.
    expect(span?.provider).toBe("anthropic");
    expect(span?.model).toBe("claude-opus");
    expect(span?.tokens_in).toBe(1234);
    expect(span?.tokens_out).toBe(56);
    expect(span?.cost).toBeCloseTo(0.0421);
    expect(span?.hash).toBe("sha256:prompt");
    expect(span?.response_hash).toBe("sha256:resp");
    // Inspector PROMPT / RESPONSE pull-quotes (full bodies, no refetch).
    expect(span?.prompt).toBe("decide whether to trade BTC");
    expect(span?.response).toBe('{"action":"buy","qty":0.1}');
    expect(span?.prompt_payload_ref).toBe("blob://prompts/p1");
    expect(span?.response_payload_ref).toBe("blob://responses/r1");
    // The span closed.
    expect(span?.status).toBe("ok");
    expect(span?.finished_at).toBe("2026-06-14T10:00:02.000Z");
  });

  test("the tool span reconstructs TOOL ARGS + TOOL RESULT bodies", () => {
    const { detail } = driveSequence();
    const span = detail.spans.find((s) => s.span_id === "span_tool_ok");
    expect(span).toBeDefined();
    expect(span?.args).toEqual({ symbol: "BTC" });
    expect(span?.result).toEqual({ price: 60000 });
    expect(span?.status).toBe("ok");
  });

  test("the failed tool span surfaces error_message", () => {
    const { detail } = driveSequence();
    const span = detail.spans.find((s) => s.span_id === "span_tool_bad");
    expect(span).toBeDefined();
    expect(span?.status).toBe("error");
    expect(span?.error_message).toBe("tool timed out after 30s");
  });

  test("the broker span reconstructs the full fill + decision_idx", () => {
    const { detail } = driveSequence();
    const span = detail.spans.find((s) => s.span_id === "span_broker");
    expect(span).toBeDefined();
    expect(span?.kind).toBe("broker.call");
    expect(span?.broker_call?.side).toBe("buy");
    expect(span?.broker_call?.symbol).toBe("BTC/USD");
    expect(span?.broker_call?.qty).toBe(0.1);
    expect(span?.broker_call?.outcome).toBe("filled");
    expect(span?.broker_call?.fill_price).toBe(60010);
    expect(span?.broker_call?.fill_qty).toBe(0.1);
    expect(span?.broker_call?.fee).toBe(0.02);
    expect(span?.broker_call?.broker_order_id).toBe("ord_live_1");
    expect(span?.broker_call?.idempotency_key).toBe(`${RUN}-7`);
    // decision_idx parsed off the idempotency key (the FilterBar / DecisionJump
    // carrier) — the same value the raw export path projects.
    expect(span?.decision_idx).toBe(7);
  });

  test("the engine risk_veto event projects an engine.event row scoped under the broker span", () => {
    const { detail } = driveSequence();
    const engineRows = detail.spans.filter((s) => s.kind === "engine.event");
    expect(engineRows).toHaveLength(1);
    const risk = engineRows[0];
    expect(risk?.attributes.engine_event_kind).toBe("risk_veto");
    expect(risk?.parent_span_id).toBe("span_broker");
    expect(
      (risk?.attributes.engine_event_payload as { reason?: string })?.reason,
    ).toBe("max_drawdown");
  });

  test("NO per-frame refetch is requested for model/tool/broker/engine frames — only the terminal run_finished refetch", () => {
    let detail = seedDetail();
    let state: LiveStreamState = { projection: [] };
    const refetchAfter: number[] = [];
    liveSequence().forEach((ev, i) => {
      const out = applyUnifiedToDetail(detail, state, ev);
      // The sequence always seeds a non-null detail, so the reducer never
    // returns null here.
    detail = out.detail!;
      state = out.state;
      if (out.requestRefetch) refetchAfter.push(i);
    });
    // The ONLY refetch is the terminal run_finished (index 10 in the sequence)
    // for canonical run-level aggregates — never a per-detail-frame refetch.
    expect(refetchAfter).toEqual([10]);
  });

  test("every live span lands in the cached detail (nothing dropped)", () => {
    const { detail } = driveSequence();
    const ids = new Set(detail.spans.map((s) => s.span_id));
    expect(ids.has("span_root")).toBe(true);
    expect(ids.has("span_decision")).toBe(true);
    expect(ids.has("span_tool_ok")).toBe(true);
    expect(ids.has("span_tool_bad")).toBe(true);
    expect(ids.has("span_broker")).toBe(true);
    // engine.event row present.
    expect(detail.spans.some((s) => s.kind === "engine.event")).toBe(true);
  });

  test("idempotent: re-applying the same frames does not duplicate spans", () => {
    let detail = seedDetail();
    let state: LiveStreamState = { projection: [] };
    const seq = liveSequence();
    for (const ev of [...seq, ...seq]) {
      const out = applyUnifiedToDetail(detail, state, ev);
      // The sequence always seeds a non-null detail, so the reducer never
    // returns null here.
    detail = out.detail!;
      state = out.state;
    }
    const counts = new Map<string, number>();
    for (const s of detail.spans) {
      counts.set(s.span_id, (counts.get(s.span_id) ?? 0) + 1);
    }
    for (const [, n] of counts) expect(n).toBe(1);
  });

  test("run_finished flips the summary status off running", () => {
    const { detail } = driveSequence();
    expect(detail.summary.status).toBe("completed");
    expect(detail.summary.finished_at).toBe("2026-06-14T10:00:02.100Z");
  });

  test("a LIVE-created agent.decision span (no seed entry) reconstructs the entry-state snapshot off span_started.attributes_json — DecisionSpanDetailRows renders mark px / position pre with NO refetch", () => {
    // The regression B2 introduced: dropping the per-frame export refetch left
    // the decision span's entry-state snapshot (asset/bar_ts/mark_price/
    // position_pre/decision_input) carried ONLY by
    // `SpanStartedEvent.attributes_json` (events.rs:137). A decision span that
    // OPENS AFTER the connect-time snapshot (the normal multi-cycle live case)
    // has no seed attribute bag, so `mark px —` / `position pre —` would render
    // from span-open until the terminal refetch unless `projectSpan` folds
    // attributes_json onto the projection.
    let detail = seedDetail();
    let state: LiveStreamState = { projection: [] };
    const decisionStart = frame(20, "span_live_decision", "2026-06-14T10:05:00.000Z", {
      kind: "span_started",
      data: {
        span_id: "span_live_decision",
        run_id: RUN,
        parent_span_id: "span_root",
        kind: "agent.decision",
        name: "trader",
        started_at: "2026-06-14T10:05:00.000Z",
        otel_trace_id: null,
        otel_span_id: null,
        attributes_json: JSON.stringify({
          asset: "ETH",
          bar_ts: "2026-06-14T10:05:00.000Z",
          mark_price: 3456.78,
          position_pre: -0.5,
          decision_index: 3,
          decision_input: { regime: "trend_up" },
        }),
      },
    });
    const out = applyUnifiedToDetail(detail, state, decisionStart);
    detail = out.detail!;
    // The live decision span MUST NOT trigger a refetch (B2 contract).
    expect(out.requestRefetch).toBeFalsy();

    const span = detail.spans.find((s) => s.span_id === "span_live_decision");
    expect(span).toBeDefined();
    expect(span?.kind).toBe("agent.decision");
    // The exact attribute bag DecisionSpanDetailRows reads — the never-go-dark
    // fields that B2 must keep rendering without the dropped refetch.
    expect(span?.attributes.asset).toBe("ETH");
    expect(span?.attributes.bar_ts).toBe("2026-06-14T10:05:00.000Z");
    expect(span?.attributes.mark_price).toBe(3456.78);
    expect(span?.attributes.position_pre).toBe(-0.5);
    expect(span?.attributes.decision_index).toBe(3);
    expect(span?.attributes.decision_input).toEqual({ regime: "trend_up" });
  });
});
