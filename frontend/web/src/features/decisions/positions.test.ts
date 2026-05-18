import { describe, expect, test } from "vitest";

import type { DecisionRowDto } from "@/api/types.gen";

import { derivePositionsByDecision } from "./positions";

function row(
  decision_index: number,
  action: string,
  fill_size: number | null,
  fill_price: number | null,
  asset = "BTC",
): DecisionRowDto {
  return {
    decision_index,
    timestamp: `2026-05-18T00:0${decision_index}:00Z`,
    asset,
    action,
    conviction: 0.5,
    justification: null,
    reasoning: null,
    order_size: fill_size,
    fill_price,
    fill_size,
    fee: null,
    pnl_realized: null,
  };
}

describe("derivePositionsByDecision", () => {
  test("flat start → empty positions on the first HOLD row", () => {
    const m = derivePositionsByDecision([row(0, "hold", null, null)]);
    expect(m.get(0)).toEqual([]);
  });

  test("short_open then CLOSE — the close row carries an empty positions list (operator-repro)", () => {
    // 2026-05-18 operator-reported: "a short_open then the next bar
    // is CLOSE flat — operator can't tell from the row whether the
    // short is still on". After this derivation the CLOSE row shows
    // [] explicitly, removing the ambiguity.
    const m = derivePositionsByDecision([
      row(0, "short_open", 0.5, 60000),
      row(1, "flat", 0.5, 61000),
    ]);
    expect(m.get(0)).toEqual([
      { asset: "BTC", side: "short", qty: 0.5, entry_price: 60000 },
    ]);
    expect(m.get(1)).toEqual([]);
  });

  test("CLOSE then HOLD — HOLD row doesn't re-introduce the closed position", () => {
    // Defends against the bug where the HOLD row after a close could
    // look like the position was "still on" because the table had
    // no positions cell. After fix: empty list on both rows.
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "flat", 1, 51000),
      row(2, "hold", null, null),
    ]);
    expect(m.get(0)).toEqual([
      { asset: "BTC", side: "long", qty: 1, entry_price: 50000 },
    ]);
    expect(m.get(1)).toEqual([]);
    expect(m.get(2)).toEqual([]);
  });

  test("re-enter after close shows the new position with the new entry price", () => {
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "flat", 1, 51000),
      row(2, "short_open", 0.5, 49000),
    ]);
    expect(m.get(2)).toEqual([
      { asset: "BTC", side: "short", qty: 0.5, entry_price: 49000 },
    ]);
  });

  test("HOLD while in a position preserves the active leg", () => {
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "hold", null, null),
      row(2, "hold", null, null),
    ]);
    expect(m.get(0)).toEqual(m.get(1));
    expect(m.get(1)).toEqual(m.get(2));
    expect(m.get(2)).toEqual([
      { asset: "BTC", side: "long", qty: 1, entry_price: 50000 },
    ]);
  });

  test("long_open while already long is a no-op (engine's simulate_fill semantics)", () => {
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "long_open", null, null), // engine emits the row but no fill
    ]);
    expect(m.get(1)).toEqual(m.get(0));
  });

  test("reverse from long to short replaces the position", () => {
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "short_open", 0.8, 60000),
    ]);
    expect(m.get(1)).toEqual([
      { asset: "BTC", side: "short", qty: 0.8, entry_price: 60000 },
    ]);
  });

  test("multi-asset state stays separate per asset, alphabetically sorted", () => {
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000, "ETH"),
      row(1, "short_open", 0.5, 60000, "BTC"),
    ]);
    expect(m.get(1)).toEqual([
      { asset: "BTC", side: "short", qty: 0.5, entry_price: 60000 },
      { asset: "ETH", side: "long", qty: 1, entry_price: 50000 },
    ]);
  });

  test("input order doesn't matter — derivation sorts by decision_index", () => {
    const sortedFirst = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "flat", 1, 51000),
    ]);
    const unsorted = derivePositionsByDecision([
      row(1, "flat", 1, 51000),
      row(0, "long_open", 1, 50000),
    ]);
    expect(unsorted).toEqual(sortedFirst);
  });

  test("zero or null fill_size on an open is ignored (defensive against degenerate rows)", () => {
    const m = derivePositionsByDecision([
      row(0, "long_open", 0, 50000),
      row(1, "long_open", null, 50000),
      row(2, "long_open", 1, 0),
    ]);
    expect(m.get(0)).toEqual([]);
    expect(m.get(1)).toEqual([]);
    expect(m.get(2)).toEqual([]);
  });

  test("unknown action is treated like HOLD (state-preserving)", () => {
    // Defends against future trader-output additions: rather than
    // silently misinterpret an unknown verb, leave state unchanged
    // so the visual stays correct until the table learns the new
    // action.
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "rebalance" as string, null, null),
    ]);
    expect(m.get(1)).toEqual(m.get(0));
  });
});
