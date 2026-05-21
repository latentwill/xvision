import { useState } from "react";
import {
  ChartFrame,
  DataTable,
  LayerPanel,
  Legend,
  PaneStack,
  UplotCompareOverlayPane,
  UplotDrawdownPane,
  type RangePreset,
} from "../primitives";
import { useChart2Layers } from "../hooks/useChart2Layers";
import { useChart2Sync } from "../hooks/useChart2Sync";
import { useChart2Theme } from "../hooks/useChart2Theme";
import type { CompareChartV2Payload } from "../types";

type Props = { payload: CompareChartV2Payload };

export function CompareChartV2({ payload }: Props) {
  const [range, setRange] = useState<RangePreset>("All");
  const { layers, toggle } = useChart2Layers("compare");
  const syncKey = useChart2Sync("compare");
  const theme = useChart2Theme();

  const legendItems = payload.arms.map((arm, i) => ({
    label: arm.label,
    color: theme.compare.palette[i % theme.compare.palette.length],
  }));

  // Aggregate drawdown across arms — take min (worst) at each time index.
  const drawdown = payload.arms[0]
    ? payload.arms[0].drawdown.map((p, i) => ({
        time: p.time,
        value: payload.arms.reduce(
          (worst, arm) => Math.min(worst, arm.drawdown[i]?.value ?? 0),
          0,
        ),
      }))
    : [];

  const dataTableRows = payload.arms.map((arm) => {
    const final = arm.equity[arm.equity.length - 1]?.value ?? 0;
    const peak = Math.max(...arm.equity.map((p) => p.value));
    const trough = Math.min(...arm.equity.map((p) => p.value));
    return {
      arm: arm.label,
      final: Math.round(final * 100) / 100,
      peak: Math.round(peak * 100) / 100,
      trough: Math.round(trough * 100) / 100,
    };
  });

  return (
    <ChartFrame
      title={`Compare · ${payload.arms.length} arms · ${payload.granularity}`}
      range={range}
      onRange={setRange}
      layersPanel={
        <LayerPanel
          groups={[
            {
              title: "Panes",
              items: [
                { key: "compareOverlay", label: "Equity overlay", on: layers.compareOverlay },
                { key: "drawdown", label: "Drawdown", on: layers.drawdown },
              ],
            },
          ]}
          onToggle={(k) => toggle(k as Parameters<typeof toggle>[0])}
        />
      }
      dataTable={
        <DataTable
          columns={[
            { key: "arm", header: "Arm" },
            { key: "final", header: "Final", align: "right" },
            { key: "peak", header: "Peak", align: "right" },
            { key: "trough", header: "Trough", align: "right" },
          ]}
          rows={dataTableRows}
        />
      }
    >
      <PaneStack syncKey={syncKey}>
        {layers.compareOverlay ? (
          <UplotCompareOverlayPane arms={payload.arms} height={220} />
        ) : null}
        {layers.drawdown ? <UplotDrawdownPane points={drawdown} height={100} /> : null}
      </PaneStack>
      <div className="px-3 py-2 border-t border-border">
        <Legend items={legendItems} />
      </div>
    </ChartFrame>
  );
}
