import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  ColorType,
  CrosshairMode,
  createChart,
  type IChartApi,
  type LogicalRange,
  type MouseEventParams,
  type UTCTimestamp,
} from "lightweight-charts";
import type { RunChartPayload, IndicatorPoint } from "@/api/types.gen";
import type { ResolvedTheme } from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { ChartContainer, type RangePreset } from "./ChartContainer";
import { chartTheme, normalizeChartTheme } from "./chart-theme";
import { useChartLayers } from "./use-chart-layers";
import { ChartLayersPanel } from "./ChartLayersPanel";
import { type LayerKey } from "./chart-layers";
import { applyVerticalAutoScale, fitChartContent } from "./chart-fit";
import { MarkerSidePanel } from "./MarkerSidePanel";

type ActiveMarker = { kind: "trade" | "veto" | "hold"; decision_index: number };
type MarkerKind = ActiveMarker["kind"];
type Props = {
  payload: RunChartPayload;
  themeMode?: "dark" | "light";
  theme?: ResolvedTheme;
  follow?: boolean;
};

const POSITION_BAND_SCALE_ID = "position-band";
const POSITION_BAND_VALUE = 1;

function toLine(p: IndicatorPoint) {
  return { time: p.time as UTCTimestamp, value: p.value };
}

type SetDataSeries = {
  setData: (data: any[]) => void;
  setMarkers?: (markers: any[]) => void;
};

type RunChartSeries = {
  candle?: SetDataSeries;
  sma20?: SetDataSeries;
  sma30?: SetDataSeries;
  sma50?: SetDataSeries;
  sma60?: SetDataSeries;
  sma90?: SetDataSeries;
  sma200?: SetDataSeries;
  ema20?: SetDataSeries;
  ema30?: SetDataSeries;
  ema50?: SetDataSeries;
  ema60?: SetDataSeries;
  ema90?: SetDataSeries;
  ema200?: SetDataSeries;
  bollUpper?: SetDataSeries;
  bollMiddle?: SetDataSeries;
  bollLower?: SetDataSeries;
  donchianUpper?: SetDataSeries;
  donchianLower?: SetDataSeries;
  markerHost?: SetDataSeries;
  longPosition?: SetDataSeries;
  shortPosition?: SetDataSeries;
  rsi?: SetDataSeries;
  macdLine?: SetDataSeries;
  macdSignal?: SetDataSeries;
  macdHistogram?: SetDataSeries;
  atr?: SetDataSeries;
  equity?: SetDataSeries;
  drawdown?: SetDataSeries;
  volume?: SetDataSeries;
};

function buildOpts(theme: ReturnType<typeof chartTheme>) {
  return {
    layout: {
      background: { type: ColorType.Solid, color: theme.background },
      textColor: theme.text,
    },
    grid: {
      vertLines: { color: theme.grid },
      horzLines: { color: theme.grid },
    },
    crosshair: { mode: CrosshairMode.Normal },
    timeScale: { rightOffset: 12, secondsVisible: false },
  };
}

function applyLogicalRange(charts: IChartApi[], range: LogicalRange) {
  charts.forEach((chart) => {
    chart.timeScale().setVisibleLogicalRange(range);
  });
}

function applyAnchorLogicalRangeToPeers(
  charts: IChartApi[],
  anchorChart: IChartApi,
  range: LogicalRange,
) {
  charts.forEach((chart) => {
    if (chart !== anchorChart) {
      chart.timeScale().setVisibleLogicalRange(range);
    }
  });
}

function sameLogicalRange(a: LogicalRange | null, b: LogicalRange | null) {
  return a?.from === b?.from && a?.to === b?.to;
}

function enterFollowMode(charts: IChartApi[]) {
  const anchorChart = charts[0];
  if (!anchorChart) return;
  const anchorTimeScale = anchorChart.timeScale();
  anchorTimeScale.scrollToRealTime();

  const anchorRange = anchorTimeScale.getVisibleLogicalRange();
  if (anchorRange) {
    applyAnchorLogicalRangeToPeers(charts, anchorChart, anchorRange);
  }
}

function markerId(kind: MarkerKind, decisionIndex: number) {
  return `${kind}:${decisionIndex}`;
}

function parseMarkerId(id: unknown): ActiveMarker | null {
  if (typeof id !== "string") return null;
  const [kind, rawDecisionIndex] = id.split(":");
  if (kind !== "trade" && kind !== "veto" && kind !== "hold") return null;
  const decisionIndex = Number(rawDecisionIndex);
  if (!Number.isInteger(decisionIndex)) return null;
  return { kind, decision_index: decisionIndex };
}

function buildMarkers(
  payload: RunChartPayload,
  layers: Record<LayerKey, boolean>,
  theme: ReturnType<typeof chartTheme>,
) {
  const allMarkers: {
    time: UTCTimestamp;
    position: "belowBar" | "aboveBar" | "inBar";
    color: string;
    shape: "arrowUp" | "arrowDown" | "circle";
    text: string;
    id: string;
  }[] = [];
  if (layers.markerBuy)
    payload.markers.trades
      .filter((t) => t.side === "Buy")
      .forEach((t) =>
        allMarkers.push({ time: t.time as UTCTimestamp, position: "belowBar", color: theme.series.markerBuy, shape: "arrowUp", text: `Buy ${t.size}`, id: markerId("trade", t.decision_index) }),
      );
  if (layers.markerSell)
    payload.markers.trades
      .filter((t) => t.side === "Sell")
      .forEach((t) =>
        allMarkers.push({ time: t.time as UTCTimestamp, position: "aboveBar", color: theme.series.markerSell, shape: "arrowDown", text: `Sell ${t.size}`, id: markerId("trade", t.decision_index) }),
      );
  if (layers.markerVeto)
    payload.markers.vetoes.forEach((v) =>
      allMarkers.push({ time: v.time as UTCTimestamp, position: "aboveBar", color: theme.series.markerVeto, shape: "circle", text: `Veto: ${v.reason}`, id: markerId("veto", v.decision_index) }),
    );
  if (layers.markerHold)
    payload.markers.holds.forEach((h) =>
      allMarkers.push({ time: h.time as UTCTimestamp, position: "inBar", color: theme.series.markerHold, shape: "circle", text: "Hold", id: markerId("hold", h.decision_index) }),
    );
  return allMarkers.sort((a, b) => (a.time as number) - (b.time as number));
}

function applySeriesData(
  series: RunChartSeries,
  payload: RunChartPayload,
  layers: Record<LayerKey, boolean>,
  theme: ReturnType<typeof chartTheme>,
) {
  series.candle?.setData(
    payload.bars.map((b) => ({
      time: b.time as UTCTimestamp,
      open: b.open,
      high: b.high,
      low: b.low,
      close: b.close,
    })),
  );
  series.sma20?.setData(payload.indicators.sma_20.map(toLine));
  series.sma30?.setData(payload.indicators.sma_30.map(toLine));
  series.sma50?.setData(payload.indicators.sma_50.map(toLine));
  series.sma60?.setData(payload.indicators.sma_60.map(toLine));
  series.sma90?.setData(payload.indicators.sma_90.map(toLine));
  series.sma200?.setData(payload.indicators.sma_200.map(toLine));
  series.ema20?.setData(payload.indicators.ema_20.map(toLine));
  series.ema30?.setData(payload.indicators.ema_30.map(toLine));
  series.ema50?.setData(payload.indicators.ema_50.map(toLine));
  series.ema60?.setData(payload.indicators.ema_60.map(toLine));
  series.ema90?.setData(payload.indicators.ema_90.map(toLine));
  series.ema200?.setData(payload.indicators.ema_200.map(toLine));
  series.bollUpper?.setData(payload.indicators.bollinger.upper.map(toLine));
  series.bollMiddle?.setData(payload.indicators.bollinger.middle.map(toLine));
  series.bollLower?.setData(payload.indicators.bollinger.lower.map(toLine));
  series.donchianUpper?.setData(payload.indicators.donchian.upper.map(toLine));
  series.donchianLower?.setData(payload.indicators.donchian.lower.map(toLine));
  series.markerHost?.setMarkers?.(buildMarkers(payload, layers, theme));
  series.longPosition?.setData(
    payload.position
      .filter((p) => p.side === "Long")
      .map((p) => ({ time: p.time as UTCTimestamp, value: POSITION_BAND_VALUE })),
  );
  series.shortPosition?.setData(
    payload.position
      .filter((p) => p.side === "Short")
      .map((p) => ({ time: p.time as UTCTimestamp, value: POSITION_BAND_VALUE })),
  );
  series.rsi?.setData(payload.indicators.rsi_14.map(toLine));
  series.macdLine?.setData(payload.indicators.macd.line.map(toLine));
  series.macdSignal?.setData(payload.indicators.macd.signal.map(toLine));
  series.macdHistogram?.setData(
    payload.indicators.macd.histogram.map((p) => ({ time: p.time as UTCTimestamp, value: p.value })),
  );
  series.atr?.setData(payload.indicators.atr_14.map(toLine));
  const startingEquity = payload.equity[0]?.equity_usd ?? 0;
  series.equity?.setData(
    payload.equity.map((p) => ({ time: p.time as UTCTimestamp, value: p.equity_usd - startingEquity })),
  );
  series.drawdown?.setData(payload.drawdown.map((p) => ({ time: p.time as UTCTimestamp, value: -p.drawdown_pct })));
  series.volume?.setData(
    payload.bars.map((b) => ({
      time: b.time as UTCTimestamp,
      value: b.volume,
      color: b.close >= b.open ? theme.series.candleUp : theme.series.candleDown,
    })),
  );
}

export function RunChart({
  payload,
  theme,
  themeMode,
  follow = false,
}: Props) {
  const appTheme = useTheme();
  const activeTheme = theme ?? normalizeChartTheme(themeMode, appTheme.resolvedTheme);
  const priceRef = useRef<HTMLDivElement>(null);
  const chartSetRef = useRef<IChartApi[]>([]);
  const seriesRef = useRef<RunChartSeries>({});
  const payloadRef = useRef(payload);
  const previousPayloadRef = useRef(payload);
  const followRef = useRef(follow);
  const layoutFollowRef = useRef(follow);
  const effectFollowRef = useRef(follow);
  const frozenLogicalRangeRef = useRef<LogicalRange | null>(null);
  const buildVersionRef = useRef(0);
  const followTransitionBuildVersionRef = useRef<number | null>(null);
  const lastSynchronizedRangeRef = useRef<LogicalRange | null>(null);
  const subRef = useRef<HTMLDivElement>(null);
  const eqRef = useRef<HTMLDivElement>(null);
  const ddRef = useRef<HTMLDivElement>(null);
  const volRef = useRef<HTMLDivElement>(null);

  const [range, setRange] = useState<RangePreset>("All");
  const { layers, toggle, set } = useChartLayers("run-detail");
  const [activeMarker, setActiveMarker] = useState<ActiveMarker | null>(null);
  const showSubpane =
    !layers.subpaneOff &&
    (layers.subpaneRsi || layers.subpaneMacd || layers.subpaneAtr);
  const showEquity = layers.equity;
  const showDrawdown = layers.drawdown;

  useEffect(() => {
    const previousPayload = previousPayloadRef.current;
    const palette = chartTheme(activeTheme);
    applySeriesData(seriesRef.current, payload, layers, palette);
    payloadRef.current = payload;
    previousPayloadRef.current = payload;
    if (previousPayload !== payload && followRef.current) {
      enterFollowMode(chartSetRef.current);
    }
  }, [payload, layers, activeTheme]);

  useEffect(() => {
    if (!priceRef.current) return;
    const buildVersion = buildVersionRef.current + 1;
    buildVersionRef.current = buildVersion;
    const palette = chartTheme(activeTheme);
    const opts = buildOpts(palette);

    const priceChart = createChart(priceRef.current, opts);
    const subChart = subRef.current && showSubpane ? createChart(subRef.current, opts) : null;
    const eqChart = eqRef.current && showEquity ? createChart(eqRef.current, opts) : null;
    const ddChart = ddRef.current && showDrawdown ? createChart(ddRef.current, opts) : null;
    const volChart = volRef.current && layers.volume ? createChart(volRef.current, opts) : null;
    const series: RunChartSeries = {};

    // --- Price pane ---
    if (layers.candles) {
      const candle = priceChart.addCandlestickSeries({
        upColor: palette.series.candleUp,
        downColor: palette.series.candleDown,
        wickUpColor: palette.series.candleUp,
        wickDownColor: palette.series.candleDown,
        borderVisible: false,
      });
      series.candle = candle as SetDataSeries;
    }
    if (layers.sma20)
      series.sma20 = priceChart.addLineSeries({ color: palette.series.sma20, lineWidth: 1 }) as SetDataSeries;
    if (layers.sma30)
      series.sma30 = priceChart.addLineSeries({ color: palette.series.sma30, lineWidth: 1 }) as SetDataSeries;
    if (layers.sma50)
      series.sma50 = priceChart.addLineSeries({ color: palette.series.sma50, lineWidth: 1 }) as SetDataSeries;
    if (layers.sma60)
      series.sma60 = priceChart.addLineSeries({ color: palette.series.sma60, lineWidth: 1 }) as SetDataSeries;
    if (layers.sma90)
      series.sma90 = priceChart.addLineSeries({ color: palette.series.sma90, lineWidth: 1 }) as SetDataSeries;
    if (layers.sma200)
      series.sma200 = priceChart.addLineSeries({ color: palette.series.sma200, lineWidth: 1 }) as SetDataSeries;
    if (layers.ema20)
      series.ema20 = priceChart.addLineSeries({ color: palette.series.ema20, lineWidth: 1, lineStyle: 2 }) as SetDataSeries;
    if (layers.ema30)
      series.ema30 = priceChart.addLineSeries({ color: palette.series.ema30, lineWidth: 1, lineStyle: 2 }) as SetDataSeries;
    if (layers.ema50)
      series.ema50 = priceChart.addLineSeries({ color: palette.series.ema50, lineWidth: 1, lineStyle: 2 }) as SetDataSeries;
    if (layers.ema60)
      series.ema60 = priceChart.addLineSeries({ color: palette.series.ema60, lineWidth: 1, lineStyle: 2 }) as SetDataSeries;
    if (layers.ema90)
      series.ema90 = priceChart.addLineSeries({ color: palette.series.ema90, lineWidth: 1, lineStyle: 2 }) as SetDataSeries;
    if (layers.ema200)
      series.ema200 = priceChart.addLineSeries({ color: palette.series.ema200, lineWidth: 1, lineStyle: 2 }) as SetDataSeries;
    if (layers.bollinger) {
      series.bollUpper = priceChart.addLineSeries({ color: palette.series.bollUpper, lineWidth: 1 }) as SetDataSeries;
      series.bollMiddle = priceChart.addLineSeries({ color: palette.series.bollMiddle, lineWidth: 1 }) as SetDataSeries;
      series.bollLower = priceChart.addLineSeries({ color: palette.series.bollLower, lineWidth: 1 }) as SetDataSeries;
    }
    if (layers.donchian) {
      series.donchianUpper = priceChart.addLineSeries({ color: palette.series.donchianUpper, lineWidth: 1 }) as SetDataSeries;
      series.donchianLower = priceChart.addLineSeries({ color: palette.series.donchianLower, lineWidth: 1 }) as SetDataSeries;
    }

    // --- Markers on price pane ---
    if (layers.markerBuy || layers.markerSell || layers.markerVeto || layers.markerHold) {
      series.markerHost = priceChart.addLineSeries({ visible: false }) as SetDataSeries;
    }

    const handleMarkerClick = (param: MouseEventParams) => {
      const marker = parseMarkerId(param.hoveredObjectId);
      if (marker) setActiveMarker(marker);
    };
    priceChart.subscribeClick(handleMarkerClick);

    // --- Position band ---
    if (layers.positionBand) {
      const positionBandScaleOptions = {
        priceScaleId: POSITION_BAND_SCALE_ID,
        lastValueVisible: false,
        priceLineVisible: false,
        crosshairMarkerVisible: false,
        autoscaleInfoProvider: () => ({
          priceRange: {
            minValue: 0,
            maxValue: POSITION_BAND_VALUE,
          },
        }),
      };
      const longSeries = priceChart.addAreaSeries({
        ...positionBandScaleOptions,
        topColor: palette.series.positionLong,
        bottomColor: "transparent",
        lineColor: "transparent",
      });
      series.longPosition = longSeries as SetDataSeries;
      const shortSeries = priceChart.addAreaSeries({
        ...positionBandScaleOptions,
        topColor: palette.series.positionShort,
        bottomColor: "transparent",
        lineColor: "transparent",
      });
      series.shortPosition = shortSeries as SetDataSeries;
    }

    // --- Subpane ---
    if (subChart) {
      if (layers.subpaneRsi) {
        const rsi = subChart.addLineSeries({ color: palette.series.rsi, lineWidth: 1 });
        series.rsi = rsi as SetDataSeries;
        rsi.createPriceLine({ price: 30, color: palette.series.guide, lineWidth: 1, lineStyle: 2 });
        rsi.createPriceLine({ price: 70, color: palette.series.guide, lineWidth: 1, lineStyle: 2 });
      } else if (layers.subpaneMacd) {
        series.macdLine = subChart.addLineSeries({ color: palette.series.macdLine, lineWidth: 1 }) as SetDataSeries;
        series.macdSignal = subChart.addLineSeries({ color: palette.series.macdSignal, lineWidth: 1 }) as SetDataSeries;
        series.macdHistogram = subChart.addHistogramSeries({ color: palette.series.macdHistogram }) as SetDataSeries;
      } else if (layers.subpaneAtr) {
        series.atr = subChart.addLineSeries({ color: palette.series.atr, lineWidth: 1 }) as SetDataSeries;
      }
    }

    // --- Earnings (P&L delta from starting balance) + drawdown ---
    if (eqChart && layers.equity) {
      const eq = eqChart.addBaselineSeries({
        baseValue: { type: "price", price: 0 },
        topLineColor: palette.series.candleUp,
        topFillColor1: palette.series.candleUp + "44",
        topFillColor2: palette.series.candleUp + "00",
        bottomLineColor: palette.series.candleDown,
        bottomFillColor1: palette.series.candleDown + "44",
        bottomFillColor2: palette.series.candleDown + "00",
      });
      series.equity = eq as SetDataSeries;
    }
    if (ddChart && layers.drawdown) {
      const dd = ddChart.addAreaSeries({
        lineColor: palette.series.drawdown,
        topColor: palette.series.drawdownTop,
        bottomColor: palette.series.drawdownBottom,
      });
      series.drawdown = dd as SetDataSeries;
    }

    // --- Volume ---
    if (volChart) {
      series.volume = volChart.addHistogramSeries({ color: palette.series.candleUp }) as SetDataSeries;
    }

    // --- Time-scale sync ---
    const all = [priceChart, subChart, eqChart, ddChart, volChart].filter(
      (c): c is IChartApi => c !== null,
    );
    chartSetRef.current = all;
    seriesRef.current = series;
    applySeriesData(series, payloadRef.current, layers, palette);

    all.forEach((c) =>
      c.timeScale().subscribeVisibleLogicalRangeChange((r: LogicalRange | null) => {
        if (!r) return;
        if (sameLogicalRange(r, lastSynchronizedRangeRef.current)) return;
        lastSynchronizedRangeRef.current = r;
        all.forEach((other) => {
          if (other !== c) other.timeScale().setVisibleLogicalRange(r);
          // F-5: re-fit the vertical price axis on every visible-range
          // change so zooming into a 10-bar slice rescales the price
          // scale to those 10 bars instead of leaving it on the prior
          // range.
          applyVerticalAutoScale(other);
        });
        applyVerticalAutoScale(c);
      }),
    );

    if (follow) {
      frozenLogicalRangeRef.current = null;
      enterFollowMode(all);
      followTransitionBuildVersionRef.current = effectFollowRef.current
        ? null
        : buildVersion;
    }

    const frozenLogicalRange = frozenLogicalRangeRef.current;
    if (!follow && frozenLogicalRange) {
      applyLogicalRange(all, frozenLogicalRange);
      // F-5: a restored frozen range still needs the price axis to
      // refit to that window's data.
      all.forEach((c) => applyVerticalAutoScale(c));
    } else if (!follow) {
      all.forEach((c) => fitChartContent(c));
    }

    return () => {
      const viewportChart = all[0];
      frozenLogicalRangeRef.current = followRef.current
        ? null
        : viewportChart?.timeScale().getVisibleLogicalRange() ?? null;
      if (chartSetRef.current === all) {
        chartSetRef.current = [];
      }
      if (seriesRef.current === series) {
        seriesRef.current = {};
      }
      priceChart.unsubscribeClick(handleMarkerClick);
      all.forEach((c) => c.remove());
    };
  }, [layers, activeTheme]);

  useLayoutEffect(() => {
    const wasFollowing = layoutFollowRef.current;
    followRef.current = follow;

    if (wasFollowing && !follow) {
      frozenLogicalRangeRef.current =
        chartSetRef.current[0]?.timeScale().getVisibleLogicalRange() ??
        frozenLogicalRangeRef.current;
    }

    layoutFollowRef.current = follow;
  }, [follow]);

  useEffect(() => {
    const wasFollowing = effectFollowRef.current;
    effectFollowRef.current = follow;

    if (!follow || wasFollowing) return;

    frozenLogicalRangeRef.current = null;
    if (followTransitionBuildVersionRef.current === buildVersionRef.current) {
      followTransitionBuildVersionRef.current = null;
      return;
    }
    enterFollowMode(chartSetRef.current);
  }, [follow]);

  return (
    <ChartContainer
      range={range}
      onRange={setRange}
      layersPanel={
        <ChartLayersPanel
          layers={layers}
          toggle={toggle}
          set={set}
          markers
          equity
          radioName="run-chart-subpane"
        />
      }
    >
      <div ref={priceRef} style={{ height: 380 }} />
      {showSubpane && <div ref={subRef} data-testid="run-chart-subpane" style={{ height: 100 }} />}
      {showEquity && <div ref={eqRef} data-testid="run-chart-equity-pane" style={{ height: 100 }} />}
      {showDrawdown && <div ref={ddRef} data-testid="run-chart-drawdown-pane" style={{ height: 70 }} />}
      {layers.volume && <div ref={volRef} style={{ height: 70 }} />}
      <MarkerSidePanel
        payload={payload}
        active={activeMarker}
        onClose={() => setActiveMarker(null)}
      />
    </ChartContainer>
  );
}
