import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  ColorType,
  CrosshairMode,
  createChart,
  type IChartApi,
  type LogicalRange,
  type UTCTimestamp,
} from "lightweight-charts";
import type { RunChartPayload, IndicatorPoint } from "@/api/types.gen";
import { ChartContainer, type RangePreset } from "./ChartContainer";
import { chartTheme } from "./chart-theme";
import { useChartLayers } from "./use-chart-layers";
import { ChartLayersPanel } from "./ChartLayersPanel";
import { MarkerSidePanel } from "./MarkerSidePanel";

type ActiveMarker = { kind: "trade" | "veto" | "hold"; decision_index: number };
type Props = {
  payload: RunChartPayload;
  themeMode?: "dark" | "light";
  follow?: boolean;
};

function toLine(p: IndicatorPoint) {
  return { time: p.time as UTCTimestamp, value: p.value };
}

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
    timeScale: { rightOffset: 6, secondsVisible: false },
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

export function RunChart({
  payload,
  themeMode = "dark",
  follow = false,
}: Props) {
  const priceRef = useRef<HTMLDivElement>(null);
  const chartSetRef = useRef<IChartApi[]>([]);
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

  useEffect(() => {
    if (!priceRef.current) return;
    const buildVersion = buildVersionRef.current + 1;
    buildVersionRef.current = buildVersion;
    const theme = chartTheme(themeMode);
    const opts = buildOpts(theme);

    const priceChart = createChart(priceRef.current, opts);
    const subChart = subRef.current ? createChart(subRef.current, opts) : null;
    const eqChart = eqRef.current ? createChart(eqRef.current, opts) : null;
    const ddChart = ddRef.current ? createChart(ddRef.current, opts) : null;
    const volChart = volRef.current && layers.volume ? createChart(volRef.current, opts) : null;

    // --- Price pane ---
    if (layers.candles) {
      const candle = priceChart.addCandlestickSeries({
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
    }
    if (layers.sma20)
      priceChart.addLineSeries({ color: theme.series.sma20, lineWidth: 1 }).setData(payload.indicators.sma_20.map(toLine));
    if (layers.sma30 && payload.indicators.sma_30)
      priceChart.addLineSeries({ color: "#a7f3d0", lineWidth: 1 }).setData(payload.indicators.sma_30.map(toLine));
    if (layers.sma50)
      priceChart.addLineSeries({ color: theme.series.sma50, lineWidth: 1 }).setData(payload.indicators.sma_50.map(toLine));
    if (layers.sma60 && payload.indicators.sma_60)
      priceChart.addLineSeries({ color: "#6ee7b7", lineWidth: 1 }).setData(payload.indicators.sma_60.map(toLine));
    if (layers.sma90 && payload.indicators.sma_90)
      priceChart.addLineSeries({ color: "#34d399", lineWidth: 1 }).setData(payload.indicators.sma_90.map(toLine));
    if (layers.sma200)
      priceChart.addLineSeries({ color: theme.series.sma200, lineWidth: 1 }).setData(payload.indicators.sma_200.map(toLine));
    if (layers.ema20)
      priceChart.addLineSeries({ color: theme.series.ema20, lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_20.map(toLine));
    if (layers.ema30 && payload.indicators.ema_30)
      priceChart.addLineSeries({ color: "#bae6fd", lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_30.map(toLine));
    if (layers.ema50)
      priceChart.addLineSeries({ color: theme.series.ema50, lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_50.map(toLine));
    if (layers.ema60 && payload.indicators.ema_60)
      priceChart.addLineSeries({ color: "#7dd3fc", lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_60.map(toLine));
    if (layers.ema90 && payload.indicators.ema_90)
      priceChart.addLineSeries({ color: "#38bdf8", lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_90.map(toLine));
    if (layers.ema200)
      priceChart.addLineSeries({ color: theme.series.ema200, lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_200.map(toLine));
    if (layers.bollinger) {
      priceChart.addLineSeries({ color: theme.series.bollUpper, lineWidth: 1 }).setData(payload.indicators.bollinger.upper.map(toLine));
      priceChart.addLineSeries({ color: theme.series.bollMiddle, lineWidth: 1 }).setData(payload.indicators.bollinger.middle.map(toLine));
      priceChart.addLineSeries({ color: theme.series.bollLower, lineWidth: 1 }).setData(payload.indicators.bollinger.lower.map(toLine));
    }
    if (layers.donchian) {
      priceChart.addLineSeries({ color: theme.series.donchianUpper, lineWidth: 1 }).setData(payload.indicators.donchian.upper.map(toLine));
      priceChart.addLineSeries({ color: theme.series.donchianLower, lineWidth: 1 }).setData(payload.indicators.donchian.lower.map(toLine));
    }

    // --- Markers on price pane ---
    const allMarkers: {
      time: UTCTimestamp;
      position: "belowBar" | "aboveBar" | "inBar";
      color: string;
      shape: "arrowUp" | "arrowDown" | "circle";
      text: string;
    }[] = [];
    if (layers.markerBuy)
      payload.markers.trades
        .filter((t) => t.side === "Buy")
        .forEach((t) =>
          allMarkers.push({ time: t.time as UTCTimestamp, position: "belowBar", color: theme.series.markerBuy, shape: "arrowUp", text: `Buy ${t.size}` }),
        );
    if (layers.markerSell)
      payload.markers.trades
        .filter((t) => t.side === "Sell")
        .forEach((t) =>
          allMarkers.push({ time: t.time as UTCTimestamp, position: "aboveBar", color: theme.series.markerSell, shape: "arrowDown", text: `Sell ${t.size}` }),
        );
    if (layers.markerVeto)
      payload.markers.vetoes.forEach((v) =>
        allMarkers.push({ time: v.time as UTCTimestamp, position: "aboveBar", color: theme.series.markerVeto, shape: "circle", text: `Veto: ${v.reason}` }),
      );
    if (layers.markerHold)
      payload.markers.holds.forEach((h) =>
        allMarkers.push({ time: h.time as UTCTimestamp, position: "inBar", color: theme.series.markerHold, shape: "circle", text: "Hold" }),
      );

    if (allMarkers.length > 0) {
      const markerHost = priceChart.addLineSeries({ visible: false });
      markerHost.setMarkers(
        allMarkers.sort((a, b) => (a.time as number) - (b.time as number)),
      );
    }

    // TODO(M2): wire marker click via chart.subscribeCrosshairMove to set activeMarker
    void setActiveMarker; // referenced to satisfy noUnusedLocals

    // --- Position band ---
    if (layers.positionBand) {
      const longSeries = priceChart.addAreaSeries({
        topColor: theme.series.positionLong,
        bottomColor: "transparent",
        lineColor: "transparent",
      });
      longSeries.setData(
        payload.position
          .filter((p) => p.side === "Long")
          .map((p) => ({ time: p.time as UTCTimestamp, value: 0 })),
      );
      const shortSeries = priceChart.addAreaSeries({
        topColor: theme.series.positionShort,
        bottomColor: "transparent",
        lineColor: "transparent",
      });
      shortSeries.setData(
        payload.position
          .filter((p) => p.side === "Short")
          .map((p) => ({ time: p.time as UTCTimestamp, value: 0 })),
      );
    }

    // --- Subpane ---
    if (subChart) {
      if (layers.subpaneRsi) {
        const rsi = subChart.addLineSeries({ color: "#a78bfa", lineWidth: 1 });
        rsi.setData(payload.indicators.rsi_14.map(toLine));
        rsi.createPriceLine({ price: 30, color: "#475569", lineWidth: 1, lineStyle: 2 });
        rsi.createPriceLine({ price: 70, color: "#475569", lineWidth: 1, lineStyle: 2 });
      } else if (layers.subpaneMacd) {
        subChart.addLineSeries({ color: "#22d3ee", lineWidth: 1 }).setData(payload.indicators.macd.line.map(toLine));
        subChart.addLineSeries({ color: "#f97316", lineWidth: 1 }).setData(payload.indicators.macd.signal.map(toLine));
        subChart.addHistogramSeries({ color: "#94a3b8" }).setData(
          payload.indicators.macd.histogram.map((p) => ({ time: p.time as UTCTimestamp, value: p.value })),
        );
      } else if (layers.subpaneAtr) {
        subChart.addLineSeries({ color: "#fbbf24", lineWidth: 1 }).setData(payload.indicators.atr_14.map(toLine));
      }
    }

    // --- Equity + drawdown ---
    if (eqChart && layers.equity) {
      const eq = eqChart.addAreaSeries({
        lineColor: theme.series.equity,
        topColor: "rgba(34,211,238,0.3)",
        bottomColor: "rgba(34,211,238,0.0)",
      });
      eq.setData(payload.equity.map((p) => ({ time: p.time as UTCTimestamp, value: p.equity_usd })));
    }
    if (ddChart && layers.drawdown) {
      const dd = ddChart.addAreaSeries({
        lineColor: theme.series.drawdown,
        topColor: "rgba(239,68,68,0.3)",
        bottomColor: "rgba(239,68,68,0.0)",
      });
      dd.setData(payload.drawdown.map((p) => ({ time: p.time as UTCTimestamp, value: -p.drawdown_pct })));
    }

    // --- Volume ---
    if (volChart) {
      volChart.addHistogramSeries({ color: theme.series.candleUp }).setData(
        payload.bars.map((b) => ({
          time: b.time as UTCTimestamp,
          value: b.volume,
          color: b.close >= b.open ? theme.series.candleUp : theme.series.candleDown,
        })),
      );
    }

    // --- Time-scale sync ---
    const all = [priceChart, subChart, eqChart, ddChart, volChart].filter(
      (c): c is IChartApi => c !== null,
    );
    chartSetRef.current = all;

    all.forEach((c) =>
      c.timeScale().subscribeVisibleLogicalRangeChange((r: LogicalRange | null) => {
        if (!r) return;
        if (sameLogicalRange(r, lastSynchronizedRangeRef.current)) return;
        lastSynchronizedRangeRef.current = r;
        all.forEach((other) => {
          if (other !== c) other.timeScale().setVisibleLogicalRange(r);
        });
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
    }

    return () => {
      const viewportChart = all[0];
      frozenLogicalRangeRef.current = followRef.current
        ? null
        : viewportChart?.timeScale().getVisibleLogicalRange() ?? null;
      if (chartSetRef.current === all) {
        chartSetRef.current = [];
      }
      all.forEach((c) => c.remove());
    };
  }, [payload, layers, themeMode]);

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
      <div ref={subRef} style={{ height: 100 }} />
      <div ref={eqRef} style={{ height: 100 }} />
      <div ref={ddRef} style={{ height: 70 }} />
      {layers.volume && <div ref={volRef} style={{ height: 70 }} />}
      <MarkerSidePanel
        payload={payload}
        active={activeMarker}
        onClose={() => setActiveMarker(null)}
      />
    </ChartContainer>
  );
}
