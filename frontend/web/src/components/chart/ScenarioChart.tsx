import { useEffect, useRef, useState } from "react";
import {
  ColorType,
  CrosshairMode,
  createChart,
  type UTCTimestamp,
} from "lightweight-charts";
import type { ScenarioChartPayload } from "@/api/types.gen/ScenarioChartPayload";
import type { IndicatorPoint } from "@/api/types.gen/IndicatorPoint";
import type { ResolvedTheme } from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { chartTheme, normalizeChartTheme } from "./chart-theme";
import { ChartContainer, type RangePreset } from "./ChartContainer";
import { CacheStatusBadge } from "@/components/scenario/CacheStatusBadge";
import { useChartLayers } from "./use-chart-layers";
import { ChartLayersPanel } from "./ChartLayersPanel";

const REGIME_BG: Record<string, string> = {
  "regime:bull": "rgba(34,197,94,0.05)",
  "regime:bear": "rgba(239,68,68,0.05)",
  "regime:chop": "rgba(148,163,184,0.05)",
  "regime:event": "rgba(245,158,11,0.05)",
};

export function ScenarioChart({
  payload,
  theme,
  themeMode,
  onFetch,
  fetchStatus,
  fetchDisabled,
}: {
  payload: ScenarioChartPayload;
  themeMode?: "dark" | "light";
  theme?: ResolvedTheme;
  onFetch?: () => void;
  fetchStatus?: string | null;
  fetchDisabled?: boolean;
}) {
  const appTheme = useTheme();
  const activeTheme = theme ?? normalizeChartTheme(themeMode, appTheme.resolvedTheme);
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>("All");
  const { layers, toggle, set } = useChartLayers("scenario");

  const regime = payload.scenario.tags.find((t) => t.startsWith("regime:"));
  const bg = regime ? REGIME_BG[regime] : undefined;

  const assetSymbol =
    payload.scenario.asset.length > 0
      ? payload.scenario.asset[0].symbol
      : "—";

  useEffect(() => {
    if (!ref.current) return;
    const palette = chartTheme(activeTheme);
    const c = createChart(ref.current, {
      layout: {
        background: { type: ColorType.Solid, color: palette.background },
        textColor: palette.text,
      },
      grid: {
        vertLines: { color: palette.grid },
        horzLines: { color: palette.grid },
      },
      crosshair: { mode: CrosshairMode.Normal },
      timeScale: { rightOffset: 6, secondsVisible: false },
    });

    if (payload.bars.length > 0) {
      const indicators = payload.indicators ?? emptyIndicators();
      const candle = c.addCandlestickSeries({
        upColor: palette.series.candleUp,
        downColor: palette.series.candleDown,
        wickUpColor: palette.series.candleUp,
        wickDownColor: palette.series.candleDown,
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

      if (layers.sma20)
        c.addLineSeries({ color: palette.series.sma20, lineWidth: 1 }).setData(indicators.sma_20.map(toLine));
      if (layers.sma30 && indicators.sma_30)
        c.addLineSeries({ color: palette.series.sma30, lineWidth: 1 }).setData(indicators.sma_30.map(toLine));
      if (layers.sma50)
        c.addLineSeries({ color: palette.series.sma50, lineWidth: 1 }).setData(indicators.sma_50.map(toLine));
      if (layers.sma60 && indicators.sma_60)
        c.addLineSeries({ color: palette.series.sma60, lineWidth: 1 }).setData(indicators.sma_60.map(toLine));
      if (layers.sma90 && indicators.sma_90)
        c.addLineSeries({ color: palette.series.sma90, lineWidth: 1 }).setData(indicators.sma_90.map(toLine));
      if (layers.sma200)
        c.addLineSeries({ color: palette.series.sma200, lineWidth: 1 }).setData(indicators.sma_200.map(toLine));
      if (layers.ema20)
        c.addLineSeries({ color: palette.series.ema20, lineWidth: 1, lineStyle: 2 }).setData(indicators.ema_20.map(toLine));
      if (layers.ema30 && indicators.ema_30)
        c.addLineSeries({ color: palette.series.ema30, lineWidth: 1, lineStyle: 2 }).setData(indicators.ema_30.map(toLine));
      if (layers.ema50)
        c.addLineSeries({ color: palette.series.ema50, lineWidth: 1, lineStyle: 2 }).setData(indicators.ema_50.map(toLine));
      if (layers.ema60 && indicators.ema_60)
        c.addLineSeries({ color: palette.series.ema60, lineWidth: 1, lineStyle: 2 }).setData(indicators.ema_60.map(toLine));
      if (layers.ema90 && indicators.ema_90)
        c.addLineSeries({ color: palette.series.ema90, lineWidth: 1, lineStyle: 2 }).setData(indicators.ema_90.map(toLine));
      if (layers.ema200)
        c.addLineSeries({ color: palette.series.ema200, lineWidth: 1, lineStyle: 2 }).setData(indicators.ema_200.map(toLine));
      if (layers.bollinger) {
        c.addLineSeries({ color: palette.series.bollUpper, lineWidth: 1 }).setData(indicators.bollinger.upper.map(toLine));
        c.addLineSeries({ color: palette.series.bollMiddle, lineWidth: 1 }).setData(indicators.bollinger.middle.map(toLine));
        c.addLineSeries({ color: palette.series.bollLower, lineWidth: 1 }).setData(indicators.bollinger.lower.map(toLine));
      }
      if (layers.donchian) {
        c.addLineSeries({ color: palette.series.donchianUpper, lineWidth: 1 }).setData(indicators.donchian.upper.map(toLine));
        c.addLineSeries({ color: palette.series.donchianLower, lineWidth: 1 }).setData(indicators.donchian.lower.map(toLine));
      }

      applyRange(c, range, payload.bars.length);

      if (layers.volume) {
        const vol = c.addHistogramSeries({ priceScaleId: "volume" });
        vol.setData(
          payload.bars.map((b) => ({
            time: b.time as UTCTimestamp,
            value: b.volume,
            color:
              b.close >= b.open
                ? palette.series.candleUp
                : palette.series.candleDown,
          })),
        );
        c.priceScale("volume").applyOptions({
          scaleMargins: { top: 0.8, bottom: 0 },
        });
      }
    }

    return () => c.remove();
  }, [payload, activeTheme, layers, range]);

  return (
    <div style={{ background: bg }}>
      <div className="flex items-center justify-between mb-2">
        <span className="text-text-3 text-[12px]">
          {assetSymbol} · {payload.scenario.granularity}
        </span>
        <CacheStatusBadge
          status={payload.cache_status}
          onFetch={onFetch}
          fetchStatus={fetchStatus}
          disabled={!!fetchDisabled}
        />
      </div>
      <ChartContainer
        range={range}
        onRange={setRange}
        layersPanel={
          <ChartLayersPanel
            layers={layers}
            toggle={toggle}
            set={set}
            radioName="scenario-chart-subpane"
          />
        }
      >
        {payload.bars.length === 0 ? (
          <div className="flex items-center justify-center h-[360px] text-text-3 text-[13px]">
            No bars cached yet. Use Fetch bars to populate this chart.
          </div>
        ) : (
          <div ref={ref} style={{ height: 360 }} />
        )}
      </ChartContainer>
    </div>
  );
}

function toLine(p: IndicatorPoint) {
  return { time: p.time as UTCTimestamp, value: p.value };
}

function emptyIndicators() {
  const empty: IndicatorPoint[] = [];
  return {
    sma_20: empty, sma_30: empty, sma_50: empty, sma_60: empty, sma_90: empty, sma_200: empty,
    ema_20: empty, ema_30: empty, ema_50: empty, ema_60: empty, ema_90: empty, ema_200: empty,
    bollinger: { upper: empty, middle: empty, lower: empty },
    donchian: { upper: empty, lower: empty },
    rsi_14: empty,
    macd: { line: empty, signal: empty, histogram: empty },
    atr_14: empty,
  };
}

function applyRange(chart: ReturnType<typeof createChart>, range: RangePreset, len: number) {
  if (len <= 0) return;
  if (range === "All") {
    chart.timeScale().fitContent();
    return;
  }
  const count =
    range === "1d" ? 24 :
    range === "1w" ? 24 * 7 :
    range === "1m" ? 24 * 30 :
    24 * 90;
  chart.timeScale().setVisibleLogicalRange({
    from: Math.max(0, len - count),
    to: len + 2,
  });
}
