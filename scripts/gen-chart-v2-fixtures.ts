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

export const fixtures = {
  run: makeRun,
  compare: makeCompare,
  scenario: makeScenario,
  strategy: makeStrategy,
  live: makeLive,
  wizard: makeWizard,
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
