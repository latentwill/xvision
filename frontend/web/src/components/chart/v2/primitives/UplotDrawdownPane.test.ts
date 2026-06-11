// Sign-convention guard for the drawdown pane: chart-v2 fixtures store
// drawdown as <= 0 while the server's RunChartPayload emits positive depth
// percentages. The pane pins its y-range to [min, 0], so un-normalized
// positive input lands above the ceiling and renders as a flat line.

import { describe, expect, it } from "vitest";
import { toUnderwaterDrawdown } from "./UplotDrawdownPane";

describe("toUnderwaterDrawdown", () => {
  it("negates positive (server drawdown_pct) depths", () => {
    expect(
      toUnderwaterDrawdown([
        { time: 1, value: 0 },
        { time: 2, value: 4.2 },
      ]),
    ).toEqual([
      { time: 1, value: 0 },
      { time: 2, value: -4.2 },
    ]);
  });

  it("passes already-underwater (fixture) values through unchanged", () => {
    const pts = [
      { time: 1, value: 0 },
      { time: 2, value: -1.4 },
    ];
    expect(toUnderwaterDrawdown(pts)).toEqual(pts);
  });
});
