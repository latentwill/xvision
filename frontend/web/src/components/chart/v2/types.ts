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

export type AnyChartV2Payload =
  | RunChartV2Payload
  | CompareChartV2Payload
  | ScenarioChartV2Payload
  | StrategyChartV2Payload
  | LiveChartV2Payload
  | WizardPreviewV2Payload;

export type FixtureKey =
  | "run"
  | "compare"
  | "scenario"
  | "strategy"
  | "live"
  | "wizard";
