import { useState } from "react";
import {
  CacheStatusBadge,
  ChartFrame,
  ConnectionStatus,
  EmptyState,
  KlineCandlePane,
  MarkerDock,
  PaneStack,
  UplotEquityPane,
  type RangePreset,
} from "../primitives";
import { useChart2Sync } from "../hooks/useChart2Sync";
import { useChart2Streaming } from "../hooks/useChart2Streaming";
import { columnarToKLineData } from "../adapters/columnar-to-klinedata";
import type { LiveChartV2Payload } from "../types";

type Props = { payload: LiveChartV2Payload };

export function LiveChartV2({ payload }: Props) {
  const [range, setRange] = useState<RangePreset>("1d");
  const syncKey = useChart2Sync("live");

  // M0 stub: streaming hook returns the initial bars unchanged.
  const { connection, lastTickMs } = useChart2Streaming({
    surface: "live",
    initial: columnarToKLineData(payload.candles),
  });

  if (payload.candles.time.length === 0) {
    return (
      <EmptyState
        title="Waiting for first bar"
        message={`Connecting to ${payload.asset} · ${payload.granularity} stream…`}
      />
    );
  }

  return (
    <div className="grid grid-cols-[1fr_240px] gap-3">
      <ChartFrame
        title={`Live · ${payload.asset} · ${payload.granularity}`}
        range={range}
        onRange={setRange}
      >
        <PaneStack syncKey={syncKey}>
          <KlineCandlePane
            candles={payload.candles}
            markers={payload.markers}
            height={300}
          />
          <UplotEquityPane points={payload.equity} height={100} />
        </PaneStack>
        <div className="px-3 py-2 border-t border-border flex items-center gap-3">
          <ConnectionStatus state={connection ?? payload.connection} lastTickMs={lastTickMs} />
          <CacheStatusBadge state={payload.cache} />
        </div>
      </ChartFrame>
      <MarkerDock markers={payload.markers} />
    </div>
  );
}
