import { describe, expect, test } from "vitest";

import type {
  ChartBar,
  ChartEquityPoint,
  DecisionRowDto,
  DrawdownPoint,
} from "@/api/types.gen";

import {
  buildPositionRows,
  currentEquity,
  dailyPnl,
  drawdownFromPeak,
  latestCloseByAsset,
  unrealizedPnl,
} from "./live-account";

function eq(time: number, equity_usd: number): ChartEquityPoint {
  return { time, equity_usd };
}

// Unix-seconds helper for a given UTC ISO timestamp.
const sec = (iso: string) => Math.floor(new Date(iso).getTime() / 1000);

function bar(time: number, close: number, asset = "BTC"): ChartBar & { _asset?: string } {
  // `asset` is not on ChartBar; tests that need per-asset bars build the
  // map directly. This helper is for single-asset close lookups.
  void asset;
  return { time, open: close, high: close, low: close, close, volume: 0 };
}

function dec(over: Partial<DecisionRowDto> = {}): DecisionRowDto {
  return {
    decision_index: 0,
    timestamp: "2026-06-09T10:00:00Z",
    asset: "BTC",
    action: "hold",
    conviction: null,
    justification: null,
    reasoning: null,
    order_size: null,
    fill_price: null,
    fill_size: null,
    fee: null,
    pnl_realized: null,
    ...over,
  };
}

describe("currentEquity", () => {
  test("returns the last equity point's value", () => {
    expect(currentEquity([eq(1, 100), eq(2, 110), eq(3, 105)])).toBe(105);
  });

  test("returns null for an empty series", () => {
    expect(currentEquity([])).toBeNull();
  });
});

describe("dailyPnl", () => {
  // Anchor "now" at 2026-06-09T15:00:00Z; midnight-UTC boundary is
  // 2026-06-09T00:00:00Z.
  const now = sec("2026-06-09T15:00:00Z");

  test("uses the equity point at/just before midnight UTC as the baseline", () => {
    const series = [
      eq(sec("2026-06-08T22:00:00Z"), 1000), // pre-midnight (baseline)
      eq(sec("2026-06-09T01:00:00Z"), 1100),
      eq(sec("2026-06-09T14:00:00Z"), 1200), // current
    ];
    const out = dailyPnl(series, now);
    expect(out.basis).toBe("midnight");
    expect(out.usd).toBe(200); // 1200 - 1000
    expect(out.pct).toBeCloseTo(20, 6); // 200 / 1000 * 100
  });

  test("prefers the latest point at/before midnight when several exist pre-midnight", () => {
    const series = [
      eq(sec("2026-06-08T20:00:00Z"), 900),
      eq(sec("2026-06-09T00:00:00Z"), 1000), // exactly midnight → baseline
      eq(sec("2026-06-09T12:00:00Z"), 1050),
    ];
    const out = dailyPnl(series, now);
    expect(out.basis).toBe("midnight");
    expect(out.usd).toBe(50);
  });

  test("falls back to first point and flags basis when no pre-midnight point exists", () => {
    const series = [
      eq(sec("2026-06-09T03:00:00Z"), 1000), // first point is after midnight
      eq(sec("2026-06-09T14:00:00Z"), 1080),
    ];
    const out = dailyPnl(series, now);
    expect(out.basis).toBe("series-start");
    expect(out.usd).toBe(80);
    expect(out.pct).toBeCloseTo(8, 6);
  });

  test("returns null usd/pct (basis 'none') for an empty series", () => {
    const out = dailyPnl([], now);
    expect(out.basis).toBe("none");
    expect(out.usd).toBeNull();
    expect(out.pct).toBeNull();
  });

  test("pct is null when the baseline equity is zero (no division)", () => {
    const series = [
      eq(sec("2026-06-08T22:00:00Z"), 0),
      eq(sec("2026-06-09T12:00:00Z"), 100),
    ];
    const out = dailyPnl(series, now);
    expect(out.usd).toBe(100);
    expect(out.pct).toBeNull();
  });
});

describe("drawdownFromPeak", () => {
  test("uses the last drawdown point's pct when the stream provides it", () => {
    const dd: DrawdownPoint[] = [
      { time: 1, drawdown_pct: 0 },
      { time: 2, drawdown_pct: 5.5 },
    ];
    expect(drawdownFromPeak(dd, [])).toBeCloseTo(5.5, 6);
  });

  test("falls back to equity-derived drawdown when no drawdown series", () => {
    // peak 120, current 90 → (120-90)/120*100 = 25
    const series = [eq(1, 100), eq(2, 120), eq(3, 90)];
    expect(drawdownFromPeak([], series)).toBeCloseTo(25, 6);
  });

  test("returns null when neither series has data", () => {
    expect(drawdownFromPeak([], [])).toBeNull();
  });
});

describe("latestCloseByAsset", () => {
  test("maps each asset to its latest bar close", () => {
    const bars = new Map<string, ChartBar[]>([
      ["BTC", [bar(1, 100), bar(2, 110)]],
    ]);
    const m = latestCloseByAsset(bars);
    expect(m.get("BTC")).toBe(110);
  });

  test("ignores assets with no bars", () => {
    const bars = new Map<string, ChartBar[]>([["BTC", []]]);
    expect(latestCloseByAsset(bars).has("BTC")).toBe(false);
  });
});

describe("unrealizedPnl", () => {
  test("sums (latest price - entry) * signed qty over open positions", () => {
    const positions = [
      { asset: "BTC", side: "long" as const, qty: 2, entry_price: 100 },
      { asset: "ETH", side: "short" as const, qty: 3, entry_price: 50 },
    ];
    const prices = new Map([
      ["BTC", 110], // long: (110-100)*2 = +20
      ["ETH", 40], // short: (40-50)*-3 = +30
    ]);
    expect(unrealizedPnl(positions, prices)).toBeCloseTo(50, 6);
  });

  test("returns null when any open position has no latest price (not derivable)", () => {
    const positions = [
      { asset: "BTC", side: "long" as const, qty: 1, entry_price: 100 },
    ];
    expect(unrealizedPnl(positions, new Map())).toBeNull();
  });

  test("returns 0 when there are no open positions", () => {
    expect(unrealizedPnl([], new Map())).toBe(0);
  });
});

describe("buildPositionRows", () => {
  test("returns current open positions from the max decision_index, with entry time", () => {
    const rows = [
      dec({
        decision_index: 0,
        asset: "BTC",
        action: "long_open",
        fill_price: 100,
        fill_size: 2,
        timestamp: "2026-06-09T08:00:00Z",
      }),
      dec({
        decision_index: 1,
        asset: "ETH",
        action: "short_open",
        fill_price: 50,
        fill_size: 4,
        timestamp: "2026-06-09T09:00:00Z",
      }),
    ];
    const prices = new Map([
      ["BTC", 110],
      ["ETH", 45],
    ]);
    const out = buildPositionRows(rows, prices);
    expect(out.map((r) => r.asset).sort()).toEqual(["BTC", "ETH"]);

    const btc = out.find((r) => r.asset === "BTC")!;
    expect(btc.side).toBe("long");
    expect(btc.qty).toBe(2);
    expect(btc.entry_price).toBe(100);
    expect(btc.entry_time).toBe("2026-06-09T08:00:00Z");
    expect(btc.current_value).toBeCloseTo(220, 6); // 2 * 110
    expect(btc.unrealized_pnl).toBeCloseTo(20, 6); // (110-100)*2
    expect(btc.pct_change).toBeCloseTo(10, 6); // 20 / (2*100) * 100

    const eth = out.find((r) => r.asset === "ETH")!;
    expect(eth.side).toBe("short");
    expect(eth.unrealized_pnl).toBeCloseTo(20, 6); // (45-50)*-4 = +20
  });

  test("uses the position set after the highest decision_index (closes drop out)", () => {
    const rows = [
      dec({
        decision_index: 0,
        asset: "BTC",
        action: "long_open",
        fill_price: 100,
        fill_size: 1,
      }),
      dec({ decision_index: 1, asset: "BTC", action: "flat", fill_size: 1 }),
    ];
    expect(buildPositionRows(rows, new Map([["BTC", 110]]))).toEqual([]);
  });

  test("leaves value/pnl/pct null when latest price is missing", () => {
    const rows = [
      dec({
        decision_index: 0,
        asset: "BTC",
        action: "long_open",
        fill_price: 100,
        fill_size: 1,
        timestamp: "2026-06-09T08:00:00Z",
      }),
    ];
    const [row] = buildPositionRows(rows, new Map());
    expect(row.current_value).toBeNull();
    expect(row.unrealized_pnl).toBeNull();
    expect(row.pct_change).toBeNull();
    expect(row.entry_time).toBe("2026-06-09T08:00:00Z");
  });

  test("returns [] for no decisions", () => {
    expect(buildPositionRows([], new Map())).toEqual([]);
  });
});
