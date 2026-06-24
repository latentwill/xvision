import { describe, expect, test } from "vitest";

import type { DecisionRowDto } from "@/api/types.gen";

import {
  derivePositionsByDecision,
  derivePriorSideByDecision,
} from "./positions";

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
    delayed: false,
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

  test("backtest reverse from long to short: new qty = fill_size - |prev_long|", () => {
    // Engine's simulate_fill records traded_units = |old| + |new|
    // for reversals (backtest.rs:761-767). For a long-1 → short-0.5
    // reverse, fill_size = 1.5 and the resulting short leg is 0.5.
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "short_open", 1.5, 60000),
    ]);
    expect(m.get(1)).toEqual([
      { asset: "BTC", side: "short", qty: 0.5, entry_price: 60000 },
    ]);
  });

  test("paper-mode Alpaca crypto: short_open while long collapses to a sell-to-close → flat (PR #284 review)", () => {
    // Alpaca crypto is long-only. paper.rs:472-486 turns
    // `short_open` while long into a sell sized exactly to the open
    // long. The persisted row carries action="short_open" and
    // fill_size == prev_long_qty. The derivation must therefore
    // resolve to FLAT, not "active short", or the trace dock
    // misrepresents what the broker actually holds.
    // Anchored by the engine test
    // `crates/xvision-engine/tests/eval_executor_paper.rs:265`
    // (`paper_executor_crypto_short_open_closes_existing_long`).
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "short_open", 1, 60000), // fill_size == prior long size
    ]);
    expect(m.get(0)).toEqual([
      { asset: "BTC", side: "long", qty: 1, entry_price: 50000 },
    ]);
    expect(m.get(1)).toEqual([]);
  });

  test("paper-mode: subsequent short_open ticks while flat stay no-ops (skip-broker semantics)", () => {
    // After the close-out, the LLM keeps emitting short_open while
    // the broker is flat. paper.rs:482-486 skips the submission;
    // fill_size on the row is null. Derivation: no state change.
    const m = derivePositionsByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "short_open", 1, 60000), // close-out
      row(2, "short_open", null, null), // skip-broker, no fill
      row(3, "short_open", null, null), // skip-broker, no fill
    ]);
    expect(m.get(1)).toEqual([]);
    expect(m.get(2)).toEqual([]);
    expect(m.get(3)).toEqual([]);
  });

  test("backtest reverse from short to long: new qty = fill_size - |prev_short|", () => {
    // Mirror of the long→short reverse for completeness.
    const m = derivePositionsByDecision([
      row(0, "short_open", 0.4, 60000),
      row(1, "long_open", 0.9, 50000), // |0.4| + |0.5| = 0.9
    ]);
    expect(m.get(1)).toEqual([
      { asset: "BTC", side: "long", qty: 0.5, entry_price: 50000 },
    ]);
  });

  test("reverse with fill_size equal to prev (within tolerance) collapses to flat", () => {
    // Floating-point safety: if the engine reports a fill that
    // exactly closes the leg with no remainder, treat as flat
    // rather than an infinitesimal opposite position.
    const m = derivePositionsByDecision([
      row(0, "long_open", 0.5, 50000),
      row(1, "short_open", 0.5, 60000),
    ]);
    expect(m.get(1)).toEqual([]);
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

describe("derivePriorSideByDecision", () => {
  test("first row is always 'flat' (no prior state)", () => {
    const m = derivePriorSideByDecision([row(0, "long_open", 1, 50000)]);
    expect(m.get(0)).toBe("flat");
  });

  test("a flat-after-long row sees prior_side='long'", () => {
    // The on-the-wire action is `"flat"` on both legs of a close;
    // the prior-side walk is what lets the action-pill render SELL
    // instead of a generic CLOSE.
    const m = derivePriorSideByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "flat", 1, 51000),
    ]);
    expect(m.get(0)).toBe("flat");
    expect(m.get(1)).toBe("long");
  });

  test("a flat-after-short row sees prior_side='short'", () => {
    const m = derivePriorSideByDecision([
      row(0, "short_open", 0.5, 60000),
      row(1, "flat", 0.5, 59000),
    ]);
    expect(m.get(1)).toBe("short");
  });

  test("hold preserves the prior side for the *next* row's prior-state", () => {
    // HOLD doesn't mutate state, so a subsequent close still sees
    // the original direction.
    const m = derivePriorSideByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "hold", null, null),
      row(2, "flat", 1, 51000),
    ]);
    expect(m.get(1)).toBe("long");
    expect(m.get(2)).toBe("long");
  });

  test("reverse via short_open while long: the short_open row sees prior_side='long'", () => {
    // Sim semantics: a short_open from a long collapses the long,
    // then the short_open row itself reports prior_side=long (the
    // state before its own action).
    const m = derivePriorSideByDecision([
      row(0, "long_open", 1, 50000),
      row(1, "short_open", 1, 51000),
    ]);
    expect(m.get(1)).toBe("long");
  });

  test("per-asset tracking: a flat on one asset doesn't blank prior_side for the other", () => {
    const m = derivePriorSideByDecision([
      row(0, "long_open", 1, 50000, "BTC"),
      row(1, "short_open", 1, 2500, "ETH"),
      row(2, "flat", 1, 51000, "BTC"),
      row(3, "flat", 1, 2400, "ETH"),
    ]);
    expect(m.get(2)).toBe("long");   // BTC was long before the close
    expect(m.get(3)).toBe("short");  // ETH was short before the close
  });
});
