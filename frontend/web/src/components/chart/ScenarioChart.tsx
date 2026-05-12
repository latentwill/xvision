import { useEffect, useRef, useState } from "react";
import {
  ColorType,
  CrosshairMode,
  createChart,
  type UTCTimestamp,
} from "lightweight-charts";
import type { ScenarioChartPayload } from "@/api/types.gen/ScenarioChartPayload";
import { chartTheme } from "./chart-theme";
import { ChartContainer, type RangePreset } from "./ChartContainer";
import { CacheStatusBadge } from "@/components/scenario/CacheStatusBadge";

const REGIME_BG: Record<string, string> = {
  "regime:bull": "rgba(34,197,94,0.05)",
  "regime:bear": "rgba(239,68,68,0.05)",
  "regime:chop": "rgba(148,163,184,0.05)",
  "regime:event": "rgba(245,158,11,0.05)",
};

export function ScenarioChart({
  payload,
  themeMode = "dark",
  onFetch,
}: {
  payload: ScenarioChartPayload;
  themeMode?: "dark" | "light";
  onFetch?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>("All");
  const [showVolume, setShowVolume] = useState(false);

  const regime = payload.scenario.tags.find((t) => t.startsWith("regime:"));
  const bg = regime ? REGIME_BG[regime] : undefined;

  const assetSymbol =
    payload.scenario.asset.length > 0
      ? payload.scenario.asset[0].symbol
      : "—";

  useEffect(() => {
    if (!ref.current) return;
    const theme = chartTheme(themeMode);
    const c = createChart(ref.current, {
      layout: {
        background: { type: ColorType.Solid, color: theme.background },
        textColor: theme.text,
      },
      grid: {
        vertLines: { color: theme.grid },
        horzLines: { color: theme.grid },
      },
      crosshair: { mode: CrosshairMode.Normal },
      timeScale: { rightOffset: 6, secondsVisible: false },
    });

    if (payload.bars.length > 0) {
      const candle = c.addCandlestickSeries({
        upColor: theme.series.candleUp,
        downColor: theme.series.candleDown,
        wickUpColor: theme.series.candleUp,
        wickDownColor: theme.series.candleDown,
        borderVisible: false,
      });
      candle.setData(
        payload.bars.map((b) => ({
          time: b.time as UTCTimestamp,
          open: b.open,
          high: b.high,
          low: b.low,
          close: b.close,
        })),
      );

      if (showVolume) {
        const vol = c.addHistogramSeries({ priceScaleId: "volume" });
        vol.setData(
          payload.bars.map((b) => ({
            time: b.time as UTCTimestamp,
            value: b.volume,
            color:
              b.close >= b.open
                ? theme.series.candleUp
                : theme.series.candleDown,
          })),
        );
        c.priceScale("volume").applyOptions({
          scaleMargins: { top: 0.8, bottom: 0 },
        });
      }
    }

    return () => c.remove();
  }, [payload, themeMode, showVolume]);

  return (
    <div style={{ background: bg }}>
      <div className="flex items-center justify-between mb-2">
        <span className="text-text-3 text-[12px]">
          {assetSymbol} · {payload.scenario.granularity}
        </span>
        <CacheStatusBadge status={payload.cache_status} onFetch={onFetch} />
      </div>
      <ChartContainer
        range={range}
        onRange={setRange}
        layersPanel={
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={showVolume}
              onChange={(e) => setShowVolume(e.target.checked)}
            />{" "}
            Volume histogram
          </label>
        }
      >
        {payload.bars.length === 0 ? (
          <div className="flex items-center justify-center h-[360px] text-text-3 text-[13px]">
            No bars cached — run{" "}
            <code className="mx-1 font-mono text-text-2">xvn bars fetch</code>{" "}
            to populate.
          </div>
        ) : (
          <div ref={ref} style={{ height: 360 }} />
        )}
      </ChartContainer>
    </div>
  );
}
