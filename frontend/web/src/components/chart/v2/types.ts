// Columnar v2 payload shapes shared by surfaces, adapters, fixtures, and the
// (forthcoming M1) /api/v2/charts/* endpoints. Parallel arrays — i-th value of
// every column refers to the same bar/sample.

export type CandleColumns = {
  time: number[];
  open: number[];
  high: number[];
  low: number[];
  close: number[];
  volume: number[];
};

export type LineSeries = {
  time: number[];
  value: number[];
};

export type IndicatorMap = {
  sma20?: LineSeries;
  sma30?: LineSeries;
  sma50?: LineSeries;
  sma60?: LineSeries;
  sma90?: LineSeries;
  sma200?: LineSeries;
  ema20?: LineSeries;
  ema30?: LineSeries;
  ema50?: LineSeries;
  ema60?: LineSeries;
  ema90?: LineSeries;
  ema200?: LineSeries;
  bollUpper?: LineSeries;
  bollMiddle?: LineSeries;
  bollLower?: LineSeries;
  donchianUpper?: LineSeries;
  donchianLower?: LineSeries;
  rsi?: LineSeries;
  macdLine?: LineSeries;
  macdSignal?: LineSeries;
  macdHist?: LineSeries;
  atr?: LineSeries;
};

export type V2Marker = {
  kind: "buy" | "sell" | "veto" | "hold";
  time: number;
  price?: number;
  text?: string;
  decision_index?: number;
};

export type PositionSpan = {
  side: "long" | "short";
  start: number;
  end: number;
};

export type EquityPoint = { time: number; value: number };
export type DrawdownPoint = { time: number; value: number };

export type RunChartV2Payload = {
  kind: "run";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  indicators: IndicatorMap;
  equity: EquityPoint[];
  drawdown: DrawdownPoint[];
  markers: V2Marker[];
  positions: PositionSpan[];
};

export type CompareArm = {
  id: string;
  label: string;
  equity: EquityPoint[];
  drawdown: DrawdownPoint[];
};

export type CompareChartV2Payload = {
  kind: "compare";
  granularity: string;
  arms: CompareArm[];
};

export type ScenarioChartV2Payload = {
  kind: "scenario";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  markers: V2Marker[];
  positions: PositionSpan[];
  equity: EquityPoint[];
};

export type StrategyChartV2Payload = {
  kind: "strategy";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  liveEquity: EquityPoint[];
  paperEquity: EquityPoint[];
  drawdown: DrawdownPoint[];
};

export type LiveChartV2Payload = {
  kind: "live";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  equity: EquityPoint[];
  markers: V2Marker[];
  live_index: number;
  connection: "connected" | "reconnecting" | "offline";
  cache: "fresh" | "cached" | "stale";
};

export type WizardPreviewV2Payload = {
  kind: "wizard";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  equity: EquityPoint[];
};

// ── Track B (Charts dashboard section, 2026-05-23) ────────────────────────
// See docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md
// §6.1, §6.2, §6.3. Payload types shared across B1–B4 + the F-CHART-LIQHEAT
// followup. Defined here so B1–B4 consumers and the followup all see one
// source of truth.

// /api/v2/charts/dashboards/overview — multi-strategy equity bundle.
// Consumers: B1 (DarkMinimalDashboard), B2 (ComparisonABDashboard),
// B4 (GradientHeroDashboard).
export type MultiStrategyBundleEntry = {
  id: string;
  name: string;
  short: string;
  /** Hex string, e.g. "#D4A547". Resolved server-side from
   *  PublicManifest.color or the strategyRotation fallback. */
  color: string;
  kind: string;
  dashed?: boolean;
  equity: number[];
  drawdown: number[];
  monthly: Array<{ year: number; month: number; value: number }>;
  metrics: {
    return: number;
    sharpe: number;
    mdd: number;
    win: number;
    pf: number;
  };
};

export type MultiStrategyEquityBundle = {
  kind: "multi_strategy_equity";
  /** unix seconds */
  generatedAt: number;
  granularity: string;
  /** shared timeline, unix seconds, length = each strategy.equity.length */
  time: number[];
  strategies: MultiStrategyBundleEntry[];
  /** defaults to strategies[0].id when omitted */
  lead?: string;
};

// /api/v2/charts/annotated/{:run_id|live/:symbol} — AI annotations.
// Consumer: B3 (AIAnnotationDashboard).
export type Annotation = {
  /** index into the candle column array this is anchored to */
  idx: number;
  /** which row of callouts to render in */
  side: "top" | "bottom";
  type: "PATTERN" | "FLOW" | "RISK" | "REVERSION" | "STRUCTURE";
  title: string;
  /** 12–25 words */
  body: string;
  /** 0..1 */
  conf: number;
  action: "WATCH" | "LONG" | "SHORT" | "CAUTION";
  /** tints callout red */
  danger?: boolean;
  /** unix seconds, used by the insight log timestamp */
  ts?: number;
};

export type AnnotatedChartPayload = {
  kind: "annotated";
  /** provenance per spec §11.2 resolution */
  source: "run" | "live";
  run_id?: string;
  symbol?: string;
  asset: string;
  granularity: string;
  candles: CandleColumns;
  ema?: LineSeries;
  /** may be [] when source = "live" and the producer is not wired
   *  (see spec §9 "out of scope") — UI renders EmptyState */
  annotations: Annotation[];
};

// /api/v2/charts/market-context — B4 follow-up. Replaces STUB_MARKET_CONTEXT +
// STUB_REGIMES literals in GradientHeroDashboard.
// MarketContextData + RegimeWeight are defined on the primitive and
// re-exported here so API consumers have a single import point.
import type {
  MarketContextData,
  RegimeWeight,
} from "./primitives/MarketContextCard";

export type { MarketContextData, RegimeWeight };

export type MarketContextPayload = {
  data: MarketContextData;
  regimes: RegimeWeight[];
};

// /api/v2/charts/heatmap/:symbol — RESERVED for the F-CHART-LIQHEAT
// followup (Chart 04). Type defined now so the followup picks up
// rework-free, but no producer/consumer in this wave.
export type LiquidationLevel = {
  price: number;
  /** 0..1 */
  heat: number;
  /** millions USD */
  notional: number;
  side: "long" | "short";
};

export type LiquidationHeatmapPayload = {
  kind: "liquidation_heatmap";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  ema?: LineSeries;
  /** top-N levels */
  levels: LiquidationLevel[];
  cascade: {
    longExposure: number;
    shortExposure: number;
    nearestWall: number;
    cascadeRisk: number;
  };
};

export type AnyChartV2Payload =
  | RunChartV2Payload
  | CompareChartV2Payload
  | ScenarioChartV2Payload
  | StrategyChartV2Payload
  | LiveChartV2Payload
  | WizardPreviewV2Payload
  | AnnotatedChartPayload;
// NB: MultiStrategyEquityBundle and LiquidationHeatmapPayload are not
// part of AnyChartV2Payload — they have their own endpoints and
// consumers and don't compose into Track A surfaces.

export type FixtureKey =
  | "run"
  | "compare"
  | "scenario"
  | "strategy"
  | "live"
  | "wizard";

// Track B dashboard fixtures. Files live alongside the Track A
// fixtures under __fixtures__/.
export type DashboardFixtureKey =
  | "multi-strategy-equity"
  | "annotations"
  | "monthly-returns";
