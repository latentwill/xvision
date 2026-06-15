// frontend/web/src/features/agent-runs/trace-labels.test.ts
import { describe, expect, test } from "vitest";
import type { RunSpan } from "@/api/types-agent-runs";
import { formatTraceLabel, optiSpanLabel } from "./trace-labels";

function baseSpan(over: Partial<RunSpan> = {}): RunSpan {
  return {
    span_id: "s_1",
    parent_span_id: null,
    name: "model.call",
    kind: "model.call",
    started_at: "2026-05-20T10:00:00.000Z",
    finished_at: "2026-05-20T10:00:01.000Z",
    status: "ok",
    attributes: {},
    ...over,
  };
}

describe("formatTraceLabel — F-5", () => {
  test("prompt label uses stage attribute + provider/model", () => {
    const span = baseSpan({
      attributes: { stage: "trader" },
      provider: "anthropic",
      model: "claude-opus-4-7",
    });
    expect(formatTraceLabel({ span, refKind: "prompt" })).toBe(
      "prompt — trader · anthropic/claude-opus-4-7",
    );
  });

  test("prompt label falls back to span name when attribute is missing", () => {
    const span = baseSpan({
      name: "model.call trader.v3",
      model: "gpt-5-mini",
    });
    // No attributes.stage, but the name carries `trader`.
    expect(formatTraceLabel({ span, refKind: "prompt" })).toBe(
      "prompt — trader · gpt-5-mini",
    );
  });

  test("prompt label uses bare placeholder when no signal at all", () => {
    const span = baseSpan({ name: "model.call" });
    expect(formatTraceLabel({ span, refKind: "prompt" })).toBe("prompt blob");
  });

  test("trader response renders as TraderDecision · <model>", () => {
    const span = baseSpan({
      attributes: { stage: "trader" },
      provider: "openai",
      model: "gpt-5",
    });
    expect(formatTraceLabel({ span, refKind: "response" })).toBe(
      "TraderDecision · openai/gpt-5",
    );
  });

  test("non-trader response keeps stage + model", () => {
    const span = baseSpan({
      attributes: { stage: "regime" },
      model: "claude-haiku-4",
    });
    expect(formatTraceLabel({ span, refKind: "response" })).toBe(
      "response — regime · claude-haiku-4",
    );
  });

  test("tool input renders tool_name(arg=value, …)", () => {
    const span = baseSpan({
      kind: "tool.call",
      name: "compute_indicator",
      args: { indicator: "rsi", window: 14, source: "close" },
    });
    expect(formatTraceLabel({ span, refKind: "tool_input" })).toBe(
      "compute_indicator(indicator=rsi, window=14, …)",
    );
  });

  test("tool input falls back to bare tool_name when args missing", () => {
    const span = baseSpan({
      kind: "tool.call",
      name: "fetch_bars",
    });
    expect(formatTraceLabel({ span, refKind: "tool_input" })).toBe("fetch_bars");
  });

  test("tool output summarises rows from result.bars + timeframe", () => {
    const span = baseSpan({
      kind: "tool.call",
      name: "fetch_bars",
      result: { timeframe: "1h", bars: new Array(480).fill({}) },
    });
    expect(formatTraceLabel({ span, refKind: "tool_output" })).toBe(
      "tool result — fetch_bars[1h, 480 rows]",
    );
  });

  test("broker.call short-sell renders side · qty · symbol · fill", () => {
    const span = baseSpan({
      kind: "broker.call",
      name: "broker.call",
      broker_call: {
        side: "short",
        symbol: "ETH",
        qty: 0.4,
        intended_price: 3200,
        order_type: "market",
        venue: "paper",
        idempotency_key: "run_xx-14",
        outcome: "filled",
        fill_price: 3210.55,
        fill_qty: 0.4,
        fee: 0.001,
        broker_order_id: "o_1",
        error_class: null,
        error_message: null,
        severity: null,
      },
    });
    // The broker_call short-circuit beats refKind switches.
    expect(
      formatTraceLabel({ span, refKind: "prompt" }),
    ).toBe("BrokerCall · SHORT 0.4 ETH @ 3210.55");
  });

  test("broker.call rejected renders outcome in parens", () => {
    const span = baseSpan({
      kind: "broker.call",
      broker_call: {
        side: "buy",
        symbol: "BTC",
        qty: 0.1234,
        intended_price: 60000,
        order_type: "market",
        venue: "paper",
        idempotency_key: null,
        outcome: "rejected",
        fill_price: null,
        fill_qty: null,
        fee: null,
        broker_order_id: null,
        error_class: "broker_insufficient_funds",
        error_message: "not enough cash",
        severity: "warn",
      },
    });
    expect(formatTraceLabel({ span, refKind: "prompt" })).toBe(
      "BrokerCall · BUY 0.1234 BTC (rejected)",
    );
  });
});

describe("optiSpanLabel — WS-11a operator-surface OPTI labels", () => {
  function optiSpan(kind: RunSpan["kind"], attrs: Record<string, unknown> = {}): RunSpan {
    return baseSpan({ kind, name: "x", attributes: attrs });
  }

  test("cycle / parent / experiment / honesty / judge / flywheel get plain-language labels", () => {
    expect(optiSpanLabel(optiSpan("opti.cycle"))).toBe("Optimizer cycle");
    expect(optiSpanLabel(optiSpan("opti.parent"))).toBe("Parent selected");
    expect(optiSpanLabel(optiSpan("opti.experiment"))).toBe("Experiment proposed");
    expect(optiSpanLabel(optiSpan("opti.honesty"))).toBe("Honesty check");
    expect(optiSpanLabel(optiSpan("opti.judge"))).toBe("Judge finding");
    expect(optiSpanLabel(optiSpan("opti.flywheel"))).toBe("Flywheel compiled");
  });

  test("opti.eval-run (WS-11b) gets the 'Eval run' operator label", () => {
    expect(optiSpanLabel(optiSpan("opti.eval-run"))).toBe("Eval run");
  });

  test("gate rows resolve the three-way outcome label (Active / Suspect / Rejected)", () => {
    expect(optiSpanLabel(optiSpan("opti.gate", { outcome: "kept" }))).toBe("Active");
    expect(optiSpanLabel(optiSpan("opti.gate", { outcome: "suspect" }))).toBe("Suspect");
    expect(optiSpanLabel(optiSpan("opti.gate", { outcome: "rejected" }))).toBe("Rejected");
  });

  test("gate row with an unknown/missing outcome falls back to a generic gate label", () => {
    expect(optiSpanLabel(optiSpan("opti.gate"))).toBe("Gate evaluated");
  });

  test("a non-opti span returns null (the caller keeps its existing label path)", () => {
    expect(optiSpanLabel(baseSpan({ kind: "model.call" }))).toBeNull();
  });
});
