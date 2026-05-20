// frontend/web/src/features/agent-runs/decision-idx.test.ts
import { describe, expect, test } from "vitest";
import {
  decisionIdxFromAttributes,
  decisionIdxFromIdempotencyKey,
} from "./decision-idx";

describe("decisionIdxFromIdempotencyKey", () => {
  test("parses the trailing integer from <run_id>-<decision_idx>", () => {
    expect(decisionIdxFromIdempotencyKey("run_xyz-42")).toBe(42);
    expect(decisionIdxFromIdempotencyKey("run_xyz-0")).toBe(0);
  });

  test("handles run_ids that themselves contain dashes (split on last)", () => {
    // The paper executor formats as `format!("{}-{}", run.id, decision_idx)`
    // and run ids in the wild include dashes (eval runs, dated ids, etc).
    // We must not greedy-match the first dash.
    expect(decisionIdxFromIdempotencyKey("run_eval_2026-05-20-abc-7")).toBe(7);
    expect(decisionIdxFromIdempotencyKey("a-b-c-d-99")).toBe(99);
  });

  test("returns undefined for missing / non-string / empty input", () => {
    expect(decisionIdxFromIdempotencyKey(null)).toBeUndefined();
    expect(decisionIdxFromIdempotencyKey(undefined)).toBeUndefined();
    expect(decisionIdxFromIdempotencyKey("")).toBeUndefined();
  });

  test("returns undefined when no dash is present", () => {
    expect(decisionIdxFromIdempotencyKey("standalone")).toBeUndefined();
  });

  test("returns undefined when the trailing segment is not a non-negative integer", () => {
    // Floats / signed / hex / mixed alphanumeric all fail loud (return
    // undefined) rather than silently coercing to 0 / NaN / partial values.
    expect(decisionIdxFromIdempotencyKey("run-3.5")).toBeUndefined();
    expect(decisionIdxFromIdempotencyKey("run-7abc")).toBeUndefined();
    expect(decisionIdxFromIdempotencyKey("run-0x7")).toBeUndefined();
    expect(decisionIdxFromIdempotencyKey("run-")).toBeUndefined();
  });

  test("splits on the LAST dash even when run_id ends in a dash", () => {
    // The carrier contract (paper.rs format!("{}-{}", run.id, decision_idx))
    // appends `-<int>` to whatever the run_id is — including pathological
    // run_ids that themselves end in `-`. We accept the cycle here rather
    // than refuse: the trailing integer is unambiguous.
    expect(decisionIdxFromIdempotencyKey("run--7")).toBe(7);
  });
});

describe("decisionIdxFromAttributes", () => {
  test("extracts decision_idx from attributes_json.broker_call.idempotency_key", () => {
    const attrs = {
      run_id: "run_xyz",
      broker_call: {
        side: "buy",
        symbol: "AAPL",
        qty: 1,
        idempotency_key: "run_xyz-14",
      },
    };
    expect(decisionIdxFromAttributes(attrs)).toBe(14);
  });

  test("returns undefined when broker_call is missing", () => {
    expect(decisionIdxFromAttributes({})).toBeUndefined();
    expect(decisionIdxFromAttributes({ broker_call: null })).toBeUndefined();
    expect(
      decisionIdxFromAttributes({ broker_call: "not-an-object" }),
    ).toBeUndefined();
  });

  test("returns undefined when idempotency_key is null / wrong type", () => {
    expect(
      decisionIdxFromAttributes({ broker_call: { idempotency_key: null } }),
    ).toBeUndefined();
    expect(
      decisionIdxFromAttributes({ broker_call: { idempotency_key: 42 } }),
    ).toBeUndefined();
    expect(decisionIdxFromAttributes({ broker_call: {} })).toBeUndefined();
  });

  test("returns undefined when idempotency_key has no trailing integer", () => {
    expect(
      decisionIdxFromAttributes({
        broker_call: { idempotency_key: "no-trailing-int-abc" },
      }),
    ).toBeUndefined();
  });
});
