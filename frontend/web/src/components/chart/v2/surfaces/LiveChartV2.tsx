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
   * Advisory. When false, the operator has frozen the live view. The
   * candle/equity panes still re-render from the latest `payload` (the
   * container re-adapts on each stream tick); `follow` is currently
   * visual-only — there is no clean way to thread it into
   * KlineCandlePane's auto-scroll this wave without inventing a fragile
   * follow API, so we accept it and leave the auto-scroll behavior to the
   * pane. See Task 9.
   */
  follow?: boolean;
};

export function LiveChartV2({ payload }: Props) {
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
          <ConnectionStatus state={payload.connection} lastTickMs={lastTickMs} />
          <CacheStatusBadge state={payload.cache} />
        </div>
      </ChartFrame>
      <MarkerDock markers={payload.markers} />
    </div>
  );
}
