#!/usr/bin/env -S node --experimental-strip-types
// Deterministic generator for the five chart-v2 fixtures.
//
//   npm run gen:chart-v2-fixtures
//
// Writes JSON files into frontend/web/src/components/chart/v2/__fixtures__/.
// Same seed → same output (mulberry32 PRNG) so regenerating is idempotent.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "frontend",
  "web",
  "src",
  "components",
  "chart",
  "v2",
  "__fixtures__",
);

function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

type Candles = {
  time: number[];
  open: number[];
  high: number[];
  low: number[];
  close: number[];
  volume: number[];
};

function makeCandles(
  startSec: number,
  stepSec: number,
  count: number,
  startPrice: number,
  vol: number,
  seed: number,
): Candles {
  const rand = rng(seed);
  const candles: Candles = {
    time: [],
    open: [],
    high: [],
    low: [],
    close: [],
    volume: [],
  };
  let price = startPrice;
  for (let i = 0; i < count; i++) {
    const open = price;
    const drift = (rand() - 0.48) * vol;
    const close = Math.max(1, open + drift);
    const wick = vol * 0.6 * rand();
    const high = Math.max(open, close) + wick;
    const low = Math.max(0.5, Math.min(open, close) - wick);
    candles.time.push(startSec + i * stepSec);
    candles.open.push(round2(open));
    candles.high.push(round2(high));
    candles.low.push(round2(low));
    candles.close.push(round2(close));
    candles.volume.push(round2(50 + rand() * 250));
    price = close;
  }
  return candles;
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

function sma(values: number[], window: number): (number | null)[] {
  const out: (number | null)[] = new Array(values.length).fill(null);
  let s = 0;
  for (let i = 0; i < values.length; i++) {
    s += values[i];
    if (i >= window) s -= values[i - window];
    if (i >= window - 1) out[i] = s / window;
  }
  return out;
}

function ema(values: number[], window: number): (number | null)[] {
  const out: (number | null)[] = new Array(values.length).fill(null);
  const k = 2 / (window + 1);
  let prev: number | null = null;
  for (let i = 0; i < values.length; i++) {
    prev = prev === null ? values[i] : values[i] * k + prev * (1 - k);
    if (i >= window - 1) out[i] = prev;
  }
  return out;
}

function rsi(values: number[], window = 14): (number | null)[] {
  const out: (number | null)[] = new Array(values.length).fill(null);
  let gain = 0;
  let loss = 0;
  for (let i = 1; i < values.length; i++) {
    const ch = values[i] - values[i - 1];
    const g = Math.max(0, ch);
    const l = Math.max(0, -ch);
    if (i <= window) {
      gain += g;
      loss += l;
      if (i === window) {
        gain /= window;
        loss /= window;
        out[i] = 100 - 100 / (1 + gain / Math.max(loss, 1e-9));
      }
    } else {
      gain = (gain * (window - 1) + g) / window;
      loss = (loss * (window - 1) + l) / window;
      out[i] = 100 - 100 / (1 + gain / Math.max(loss, 1e-9));
    }
  }
  return out;
}

function asLineSeries(
  times: number[],
  values: (number | null)[],
): { time: number[]; value: number[] } {
  const t: number[] = [];
  const v: number[] = [];
  for (let i = 0; i < values.length; i++) {
    const val = values[i];
    if (val === null || Number.isNaN(val)) continue;
    t.push(times[i]);
    v.push(round2(val));
  }
  return { time: t, value: v };
}

function bollinger(values: number[], window = 20, k = 2) {
  const middle = sma(values, window);
  const upper: (number | null)[] = new Array(values.length).fill(null);
  const lower: (number | null)[] = new Array(values.length).fill(null);
  for (let i = window - 1; i < values.length; i++) {
    let sum = 0;
    for (let j = i - window + 1; j <= i; j++) sum += (values[j] - middle[i]!) ** 2;
    const sd = Math.sqrt(sum / window);
    upper[i] = middle[i]! + k * sd;
    lower[i] = middle[i]! - k * sd;
  }
  return { upper, middle, lower };
}

function donchian(highs: number[], lows: number[], window = 20) {
  const upper: (number | null)[] = new Array(highs.length).fill(null);
  const lower: (number | null)[] = new Array(lows.length).fill(null);
  for (let i = window - 1; i < highs.length; i++) {
    let hi = -Infinity;
    let lo = Infinity;
    for (let j = i - window + 1; j <= i; j++) {
      if (highs[j] > hi) hi = highs[j];
      if (lows[j] < lo) lo = lows[j];
    }
    upper[i] = hi;
    lower[i] = lo;
  }
  return { upper, lower };
}

function makeRun() {
  const startSec = 1_700_000_000;
  const step = 3600;
  const count = 240;
  const candles = makeCandles(startSec, step, count, 200, 5, 11);
  const close = candles.close;
  const high = candles.high;
  const low = candles.low;

  const boll = bollinger(close, 20, 2);
  const don = donchian(high, low, 20);

  const equity: { time: number; value: number }[] = [];
  const drawdown: { time: number; value: number }[] = [];
  let eq = 10000;
  let peak = eq;
  for (let i = 0; i < count; i++) {
    eq += (close[i] - (i ? close[i - 1] : close[0])) * 0.7;
    peak = Math.max(peak, eq);
    equity.push({ time: candles.time[i], value: round2(eq - 10000) });
    drawdown.push({
      time: candles.time[i],
      value: round2(((eq - peak) / peak) * 100),
    });
  }

  const markers = [
    { kind: "buy" as const, time: candles.time[20], price: close[20], text: "Buy 1.0", decision_index: 0 },
    { kind: "sell" as const, time: candles.time[55], price: close[55], text: "Sell 1.0", decision_index: 1 },
    { kind: "buy" as const, time: candles.time[80], price: close[80], text: "Buy 0.6", decision_index: 2 },
    { kind: "veto" as const, time: candles.time[110], price: close[110], text: "Risk veto: ATR breach", decision_index: 3 },
    { kind: "hold" as const, time: candles.time[140], price: close[140], text: "Hold", decision_index: 4 },
    { kind: "sell" as const, time: candles.time[170], price: close[170], text: "Sell 0.6", decision_index: 5 },
    { kind: "buy" as const, time: candles.time[200], price: close[200], text: "Buy 0.8", decision_index: 6 },
  ];

  const positions = [
    { side: "long" as const, start: candles.time[20], end: candles.time[55] },
    { side: "long" as const, start: candles.time[80], end: candles.time[170] },
    { side: "long" as const, start: candles.time[200], end: candles.time[count - 1] },
  ];

  return {
    kind: "run" as const,
    asset: "ETH",
    granularity: "1h",
    candles,
    indicators: {
      sma20: asLineSeries(candles.time, sma(close, 20)),
      sma50: asLineSeries(candles.time, sma(close, 50)),
      sma200: asLineSeries(candles.time, sma(close, 200)),
      ema20: asLineSeries(candles.time, ema(close, 20)),
      ema50: asLineSeries(candles.time, ema(close, 50)),
      bollUpper: asLineSeries(candles.time, boll.upper),
      bollMiddle: asLineSeries(candles.time, boll.middle),
      bollLower: asLineSeries(candles.time, boll.lower),
      donchianUpper: asLineSeries(candles.time, don.upper),
      donchianLower: asLineSeries(candles.time, don.lower),
      rsi: asLineSeries(candles.time, rsi(close, 14)),
    },
    equity,
    drawdown,
    markers,
    positions,
  };
}

function makeCompare() {
  const startSec = 1_700_000_000;
  const step = 3600;
  const count = 240;
  const arms = ["baseline", "rsi-tight", "macd-cross", "donchian-breakout"].map(
    (id, idx) => {
      const rand = rng(31 + idx * 17);
      const equity: { time: number; value: number }[] = [];
      const drawdown: { time: number; value: number }[] = [];
      let eq = 0;
      let peak = 0;
      for (let i = 0; i < count; i++) {
        eq += (rand() - 0.48) * (1 + idx * 0.3);
        peak = Math.max(peak, eq);
        equity.push({ time: startSec + i * step, value: round2(eq) });
        drawdown.push({
          time: startSec + i * step,
          value: round2(eq - peak),
        });
      }
      return {
        id,
        label: id,
        equity,
        drawdown,
      };
    },
  );
  return {
    kind: "compare" as const,
    granularity: "1h",
    arms,
  };
}

function makeScenario() {
  const startSec = 1_700_000_000;
  const step = 3600;
  const count = 96;
  const candles = makeCandles(startSec, step, count, 1850, 12, 23);
  const equity: { time: number; value: number }[] = [];
  let eq = 0;
  for (let i = 0; i < count; i++) {
    eq += (candles.close[i] - (i ? candles.close[i - 1] : candles.close[0])) * 0.4;
    equity.push({ time: candles.time[i], value: round2(eq) });
  }
  const markers = [
    { kind: "buy" as const, time: candles.time[12], price: candles.close[12], text: "Buy", decision_index: 0 },
    { kind: "sell" as const, time: candles.time[40], price: candles.close[40], text: "Sell", decision_index: 1 },
    { kind: "buy" as const, time: candles.time[60], price: candles.close[60], text: "Buy", decision_index: 2 },
    { kind: "sell" as const, time: candles.time[85], price: candles.close[85], text: "Sell", decision_index: 3 },
  ];
  const positions = [
    { side: "long" as const, start: candles.time[12], end: candles.time[40] },
    { side: "long" as const, start: candles.time[60], end: candles.time[85] },
  ];
  return {
    kind: "scenario" as const,
    asset: "BTC",
    granularity: "1h",
    candles,
    markers,
    positions,
    equity,
  };
}

function makeStrategy() {
  const startSec = 1_700_000_000;
  const step = 3600;
  const count = 480;
  const candles = makeCandles(startSec, step, count, 60000, 200, 37);
  const liveEquity: { time: number; value: number }[] = [];
  const paperEquity: { time: number; value: number }[] = [];
  const drawdown: { time: number; value: number }[] = [];
  let live = 0;
  let paper = 0;
  let peak = 0;
  const r = rng(101);
  for (let i = 0; i < count; i++) {
    live += (r() - 0.49) * 4;
    paper += (r() - 0.5) * 4;
    peak = Math.max(peak, live);
    liveEquity.push({ time: candles.time[i], value: round2(live) });
    paperEquity.push({ time: candles.time[i], value: round2(paper) });
    drawdown.push({ time: candles.time[i], value: round2(live - peak) });
  }
  return {
    kind: "strategy" as const,
    asset: "BTC",
    granularity: "1h",
    candles,
    liveEquity,
    paperEquity,
    drawdown,
  };
}

function makeLive() {
  const startSec = 1_700_000_000;
  const step = 60;
  const count = 60;
  const candles = makeCandles(startSec, step, count, 200, 1.5, 7);
  const equity: { time: number; value: number }[] = [];
  let eq = 0;
  for (let i = 0; i < count; i++) {
    eq += (candles.close[i] - (i ? candles.close[i - 1] : candles.close[0])) * 0.8;
    equity.push({ time: candles.time[i], value: round2(eq) });
  }
  return {
    kind: "live" as const,
    asset: "ETH",
    granularity: "1m",
    candles,
    equity,
    markers: [
      { kind: "buy" as const, time: candles.time[10], price: candles.close[10], text: "Buy", decision_index: 0 },
      { kind: "sell" as const, time: candles.time[35], price: candles.close[35], text: "Sell", decision_index: 1 },
    ],
    live_index: count - 1,
    connection: "connected" as const,
    cache: "fresh" as const,
  };
}

function makeWizard() {
  const startSec = 1_700_000_000;
  const step = 3600;
  const count = 30;
  const candles = makeCandles(startSec, step, count, 100, 1.2, 3);
  const equity: { time: number; value: number }[] = [];
  let eq = 0;
  for (let i = 0; i < count; i++) {
    eq += (candles.close[i] - (i ? candles.close[i - 1] : candles.close[0])) * 1.2;
    equity.push({ time: candles.time[i], value: round2(eq) });
  }
  return {
    kind: "wizard" as const,
    asset: "DEMO",
    granularity: "1h",
    candles,
    equity,
  };
}

// ── Track B (Charts dashboard section, 2026-05-23) ───────────────────────
// Three new fixtures that back the B1–B4 dashboard canvases. Each uses
// the same mulberry32 PRNG with a fixed seed so re-runs are byte-equal.
// Port of the handoff's generators in
// docs/design/trading-charts/XVN.zip → design_handoff_charts/source/charts/chart-data.js
// (XVN_STRATEGIES, makeEquity, makeDrawdownSeries, makeMonthlyMatrix).

const STRATEGY_ROTATION = [
  { id: "fib", name: "Fibonacci Golden Cross", short: "Fib · GC", color: "#D4A547", kind: "Trend", return: 82.41, sharpe: 1.92, mdd: -18.72, win: 58.6, pf: 1.81 },
  { id: "ema", name: "EMA Pullback", short: "EMA · 50/200", color: "#E8DCB0", kind: "Trend", return: 46.27, sharpe: 1.41, mdd: -14.38, win: 54.1, pf: 1.46 },
  { id: "brk", name: "Breakout Retest", short: "BRK · 4h", color: "#E07A3A", kind: "Momentum", return: 28.14, sharpe: 1.07, mdd: -12.93, win: 51.2, pf: 1.32 },
  { id: "msw", name: "Momentum Swing", short: "MSW · 1d", color: "#B98AB4", kind: "Momentum", return: 12.68, sharpe: 0.74, mdd: -15.91, win: 47.8, pf: 1.18 },
  { id: "mvr", name: "Mean Reversion AI", short: "MVR · 15m", color: "#6BAFA8", kind: "Reversion", return: 34.12, sharpe: 1.18, mdd: -16.04, win: 53.0, pf: 1.39 },
  { id: "vsc", name: "Volatility Scalper", short: "VSC · 5m", color: "#D67B5C", kind: "Vol", return: 21.85, sharpe: 0.96, mdd: -11.42, win: 50.7, pf: 1.24 },
  { id: "lqh", name: "Liquidation Hunter", short: "LQH · 1h", color: "#8C6024", kind: "Vol", return: 18.04, sharpe: 0.81, mdd: -19.20, win: 46.2, pf: 1.16 },
  { id: "btc", name: "BTC Buy & Hold", short: "BTC · HOLD", color: "#6B6553", kind: "Bench", return: -3.21, sharpe: 0.22, mdd: -26.84, win: 43.1, pf: 0.89, dashed: true as const },
];

function hashSeed(s: string): number {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h >>> 0;
}

function makeEquityCurve(target: number, points: number, seed: number): number[] {
  const rand = rng(seed);
  const out: number[] = [];
  let v = 0; // accumulated return, baselined at 0
  for (let i = 0; i < points; i++) {
    const shock = (rand() - 0.5) * 2 * 0.012;
    v += 0.0006 + shock;
    v += ((target / 100) * (i / points) - v) * 0.012;
    out.push(round2(v * 100));
  }
  if (out.length > 0) {
    out[0] = 0;
    out[out.length - 1] = round2(target);
  }
  return out;
}

function makeDrawdownFromEquity(equity: number[]): number[] {
  const out: number[] = [];
  let peak = equity[0] ?? 0;
  for (const v of equity) {
    if (v > peak) peak = v;
    out.push(round2(v - peak)); // ≤ 0
  }
  return out;
}

function makeMonthlyMatrix(
  strategy: (typeof STRATEGY_ROTATION)[number],
  months: number,
  seed: number,
): Array<{ year: number; month: number; value: number }> {
  const rand = rng(seed);
  const base = strategy.return / 100 / 12;
  const out: Array<{ year: number; month: number; value: number }> = [];
  // Anchor to 2024-01 so the matrix is recognisable.
  let year = 2024;
  let month = 1;
  for (let i = 0; i < months; i++) {
    const v = base + (rand() - 0.5) * 0.10;
    out.push({ year, month, value: round2(v) });
    month += 1;
    if (month > 12) {
      month = 1;
      year += 1;
    }
  }
  return out;
}

function makeMultiStrategyEquity() {
  const points = 240;
  const startSec = Date.UTC(2024, 0, 2) / 1000;
  const stepSec = 86400; // daily
  const time: number[] = [];
  for (let i = 0; i < points; i++) time.push(startSec + i * stepSec);

  const strategies = STRATEGY_ROTATION.slice(0, 5).map((s) => {
    const equity = makeEquityCurve(s.return, points, hashSeed(s.id));
    const drawdown = makeDrawdownFromEquity(equity);
    const monthly = makeMonthlyMatrix(s, 12, hashSeed(s.id) ^ 0xDEADBEEF);
    return {
      id: s.id,
      name: s.name,
      short: s.short,
      color: s.color,
      kind: s.kind,
      ...(s.dashed ? { dashed: true as const } : {}),
      equity,
      drawdown,
      monthly,
      metrics: {
        return: s.return,
        sharpe: s.sharpe,
        mdd: s.mdd,
        win: s.win,
        pf: s.pf,
      },
    };
  });

  return {
    kind: "multi_strategy_equity" as const,
    generatedAt: startSec,
    granularity: "1d",
    time,
    strategies,
    lead: strategies[0].id,
  };
}

function makeAnnotationsFixture() {
  // Five annotations matching the handoff's `chart-ai-annotation.jsx`
  // sample. `idx` references the bar in the live-fixture candle array.
  return [
    {
      idx: 22,
      side: "top" as const,
      type: "PATTERN" as const,
      title: "Bull Flag",
      body: "Flag consolidation after impulse. Breakout > 64,920 likely retests 63,100 wick.",
      conf: 0.74,
      action: "WATCH" as const,
    },
    {
      idx: 52,
      side: "bottom" as const,
      type: "FLOW" as const,
      title: "Volume Divergence",
      body: "LL price with HH buy volume — accumulation footprint, 3-bar window.",
      conf: 0.68,
      action: "LONG" as const,
    },
    {
      idx: 80,
      side: "top" as const,
      type: "RISK" as const,
      title: "Liquidation Wall",
      body: "$48M long liq cluster at 65,800. Likely magnet on next vol expansion.",
      conf: 0.82,
      action: "CAUTION" as const,
      danger: true,
    },
    {
      idx: 110,
      side: "bottom" as const,
      type: "REVERSION" as const,
      title: "RSI Reset",
      body: "RSI cooled 71 → 47 without breaking trend. Mean-reversion re-entry zone.",
      conf: 0.61,
      action: "LONG" as const,
    },
    {
      idx: 144,
      side: "top" as const,
      type: "STRUCTURE" as const,
      title: "Break of Structure",
      body: "HL → HH → BoS sequence confirmed. Bias flips bullish on close > 65,200.",
      conf: 0.79,
      action: "LONG" as const,
    },
  ];
}

function makeMonthlyReturnsFixture() {
  // 5 strategies × 17 months. Independent fixture from
  // multi-strategy-equity (which carries 12 months per strategy inline)
  // so the heatmap can be evaluated standalone. Same seed (99) the
  // handoff uses.
  const months = 17;
  const rand = rng(99);
  return STRATEGY_ROTATION.slice(0, 5).map((s) => {
    const base = s.return / 100 / 12;
    const cells: Array<{ year: number; month: number; value: number }> = [];
    let year = 2024;
    let month = 1;
    for (let i = 0; i < months; i++) {
      cells.push({ year, month, value: round2(base + (rand() - 0.5) * 0.10) });
      month += 1;
      if (month > 12) {
        month = 1;
        year += 1;
      }
    }
    return {
      id: s.id,
      name: s.name,
      color: s.color,
      cells,
    };
  });
}

export const fixtures = {
  run: makeRun,
  compare: makeCompare,
  scenario: makeScenario,
  strategy: makeStrategy,
  live: makeLive,
  wizard: makeWizard,
  // Track B dashboard fixtures
  "multi-strategy-equity": makeMultiStrategyEquity,
  annotations: makeAnnotationsFixture,
  "monthly-returns": makeMonthlyReturnsFixture,
};

function main() {
  mkdirSync(OUT_DIR, { recursive: true });
  for (const [key, gen] of Object.entries(fixtures)) {
    const payload = gen();
    const file = join(OUT_DIR, `${key}.json`);
    writeFileSync(file, JSON.stringify(payload, null, 2) + "\n");
    console.log(`wrote ${file}`);
  }
}

const isMain =
  typeof process !== "undefined" &&
  process.argv[1] &&
  import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  main();
}
