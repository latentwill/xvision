import { describe, expect, it } from "vitest";

import type { ScenarioPreviewPayload } from "@/api/types.gen";
import { scenarioPreviewToWizardV2 } from "./scenario-preview-payload";

function preview(): ScenarioPreviewPayload {
  return {
    cache_key: "k",
    asset: "ETH",
    granularity: "1h",
    bars: [{ time: 1, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 }],
    cache_status: { type: "FullyCached", bar_count: 1 } as never,
    baseline_equity: [{ time: 1, equity_usd: 1000 }],
  };
}

describe("scenarioPreviewToWizardV2", () => {
  it("maps bars to candles, baseline_equity to equity, and sets kind/asset/granularity", () => {
    const result = scenarioPreviewToWizardV2(preview());

    expect(result.kind).toBe("wizard");
    expect(result.asset).toBe("ETH");
    expect(result.granularity).toBe("1h");

    expect(result.candles.time).toEqual([1]);
    expect(result.candles.open).toEqual([1]);
    expect(result.candles.high).toEqual([2]);
    expect(result.candles.low).toEqual([0.5]);
    expect(result.candles.close).toEqual([1.5]);
    expect(result.candles.volume).toEqual([10]);

    expect(result.equity).toEqual([{ time: 1, value: 1000 }]);
  });

  it("maps equity_usd to value", () => {
    const p = preview();
    p.baseline_equity = [
      { time: 1, equity_usd: 500 },
      { time: 2, equity_usd: 750 },
    ];
    const result = scenarioPreviewToWizardV2(p);

    expect(result.equity).toEqual([
      { time: 1, value: 500 },
      { time: 2, value: 750 },
    ]);
  });

  it("returns equity: [] when baseline_equity is null", () => {
    const p = preview();
    p.baseline_equity = null;
    const result = scenarioPreviewToWizardV2(p);

    expect(result.equity).toEqual([]);
  });
});
