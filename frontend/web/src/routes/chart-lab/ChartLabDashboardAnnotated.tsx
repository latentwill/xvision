// /chart-lab/dashboards/annotated — fixture render of B3's
// AIAnnotationDashboard. Synthesises a payload from the
// `annotations.json` fixture + the same demo-candle generator the
// backend uses (kept in sync via shape; the frontend fixture's `idx`
// values index into the 170-bar candle array).

import fixtureAnnotations from "@/components/chart/v2/__fixtures__/annotations.json";
import { AIAnnotationDashboard } from "@/components/chart/v2/surfaces/AIAnnotationDashboard";
import type {
  Annotation,
  AnnotatedChartPayload,
  CandleColumns,
} from "@/components/chart/v2/types";

// Mulberry32 — same PRNG as scripts/gen-chart-v2-fixtures.ts.
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

/** Generate the same 170-bar candle column array the backend stub does
 *  so the fixture page and the production /charts/annotated route show
 *  the same anchor placement. */
function buildDemoCandles(): CandleColumns {
  const COUNT = 170;
  const STEP = 3600;
  const START = 1_738_368_000;
  const VOL = 280;
  const rand = rng(17);
  const out: CandleColumns = {
    time: [],
    open: [],
    high: [],
    low: [],
    close: [],
    volume: [],
  };
  let price = 63500;
  for (let i = 0; i < COUNT; i++) {
    const drift = Math.sin(i / 14) * 90 + Math.cos(i / 35) * 240;
    const prev = i > 0 ? Math.sin((i - 1) / 14) * 90 + Math.cos((i - 1) / 35) * 240 : 0;
    const open = price;
    const noise = (rand() - 0.5) * 2 * 210;
    const close = open + noise + (drift - prev);
    const high = Math.max(open, close) + rand() * VOL;
    const low = Math.min(open, close) - rand() * VOL;
    const vol = 800 + rand() * 1800;
    out.time.push(START + i * STEP);
    out.open.push(round2(open));
    out.high.push(round2(high));
    out.low.push(round2(low));
    out.close.push(round2(close));
    out.volume.push(round2(vol));
    price = close;
  }
  return out;
}

export function ChartLabDashboardAnnotated() {
  const payload: AnnotatedChartPayload = {
    kind: "annotated",
    source: "run",
    run_id: "fixture",
    asset: "BTC/USDT",
    granularity: "1h",
    candles: buildDemoCandles(),
    annotations: fixtureAnnotations as unknown as Annotation[],
  };
  return (
    <div className="space-y-3">
      <div className="text-[12px] text-text-3">
        Rendered against the synthesised demo candles + the{" "}
        <code className="text-text-2">annotations.json</code> fixture. Mirrors
        the backend stub at <code>/api/v2/charts/annotated/:run_id</code>.
      </div>
      <AIAnnotationDashboard payload={payload} />
    </div>
  );
}
