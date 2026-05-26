import type { ScenarioPreviewPayload } from "@/api/types.gen";
import type { EquityPoint, WizardPreviewV2Payload } from "../types";

export function scenarioPreviewToWizardV2(
  p: ScenarioPreviewPayload,
): WizardPreviewV2Payload {
  return {
    kind: "wizard",
    asset: p.asset,
    granularity: p.granularity,
    candles: {
      time: p.bars.map((b) => b.time),
      open: p.bars.map((b) => b.open),
      high: p.bars.map((b) => b.high),
      low: p.bars.map((b) => b.low),
      close: p.bars.map((b) => b.close),
      volume: p.bars.map((b) => b.volume),
    },
    equity: (p.baseline_equity ?? []).map(
      (e): EquityPoint => ({ time: e.time, value: e.equity_usd }),
    ),
  };
}
