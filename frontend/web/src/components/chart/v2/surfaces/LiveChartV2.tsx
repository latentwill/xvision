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
import type { LiveChartV2Payload } from "../types";

type Props = {
  payload: LiveChartV2Payload;
  /**
   * Live follow/freeze toggle, threaded into the candle pane's scroll behavior.
   * When true (following), the candle pane pins the latest bar to the right edge
   * after each data load and snaps to realtime on resume; when false (frozen),
   * new streaming bars no longer yank the view to realtime — the prior window
   * stays put. Passed straight through to `<KlineCandlePane follow={...} />`.
   */
  follow?: boolean;
};

export function LiveChartV2({ payload, follow }: Props) {
  const [range, setRange] = useState<RangePreset>("1d");
  const syncKey = useChart2Sync("live");

  // Last bar time → ms, surfaced as the ConnectionStatus tick timestamp.
  const t = payload.candles.time;
  const lastTickMs = t.length ? t[t.length - 1] * 1000 : null;

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
        title={`Live · ${payload.asset} · ${payload.granularity} · ${range}`}
        range={range}
        onRange={setRange}
      >
        <PaneStack syncKey={syncKey}>
          <KlineCandlePane
            candles={payload.candles}
            markers={payload.markers}
            follow={follow}
            height={300}
          />
          <UplotEquityPane points={payload.equity} height={100} />
        </PaneStack>
        <div className="px-3 py-2 border-t border-border flex items-center gap-3">
          <ConnectionStatus state={payload.connection} lastTickMs={lastTickMs} />
          <CacheStatusBadge state={payload.cache} />
        </div>
      </ChartFrame>
      <MarkerDock markers={payload.markers} />
    </div>
  );
}
