// frontend/web/src/features/live/deployment-risk.test.ts
//
// TDD spec for the pure deployment-risk selector (CT5 S0).
// Covers drawdown tone, running P&L, daily-loss buffer tone, formatters.
//
// HONESTY MANDATE: all rendered values come from the wire contract exactly as
// received. paper/testnet = simulated. No fabricated or inferred money figures.

import { describe, expect, it } from "vitest";
import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";
import {
  dailyLossBufferTone,
  drawdownTone,
  formatEta,
  formatPct,
  formatUsd,
  runningPnl,
  toneGlyph,
  type RiskTone,
} from "./deployment-risk";

// ─── fixture helper ─────────────────────────────────────────────────────────

function dep(over: Partial<LiveDeploymentSummary> = {}): LiveDeploymentSummary {
  return {
    deployment_id: "dep_01",
    strategy_id: "strat-1",
    strategy_name: null,
    mode: "paper",
    status: "running",
    started_at: "2026-06-13T10:00:00Z",
    last_decision_at: null,
    venue: "alpaca-paper",
    venue_connected: true,
    deployed_capital_usd: null,
    realized_pnl_usd: null,
    unrealized_pnl_usd: null,
    drawdown_pct: null,
    daily_loss_limit_remaining_usd: null,
    daily_loss_budget_usd: null,
    stop_at: null,
    risk_veto_count_since_last_visit: null,
    paused: false,
    flatten_requested: false,
    global_safety_paused: false,
    source: "human",
    unavailable_reason: null,
    ...over,
  };
}

// ─── drawdownTone ────────────────────────────────────────────────────────────

describe("drawdownTone", () => {
  it("null → neutral", () => {
    expect(drawdownTone(null)).toBe<RiskTone>("neutral");
  });

  it("4.9 → gold (below warn threshold)", () => {
    expect(drawdownTone(4.9)).toBe<RiskTone>("gold");
  });

  it("0 → gold (healthy floor)", () => {
    expect(drawdownTone(0)).toBe<RiskTone>("gold");
  });

  it("5.0 → warn (at warn threshold)", () => {
    expect(drawdownTone(5.0)).toBe<RiskTone>("warn");
  });

  it("14.9 → warn (just below danger threshold)", () => {
    expect(drawdownTone(14.9)).toBe<RiskTone>("warn");
  });

  it("15.0 → danger (at danger threshold)", () => {
    expect(drawdownTone(15.0)).toBe<RiskTone>("danger");
  });

  it("50.0 → danger (well above threshold)", () => {
    expect(drawdownTone(50.0)).toBe<RiskTone>("danger");
  });
});

// ─── runningPnl ──────────────────────────────────────────────────────────────

describe("runningPnl", () => {
  it("positive combined P&L → gold + ▲", () => {
    const result = runningPnl(dep({ unrealized_pnl_usd: 500, realized_pnl_usd: 200 }));
    expect(result.value).toBe(700);
    expect(result.tone).toBe<RiskTone>("gold");
    expect(result.glyph).toBe("▲");
  });

  it("negative combined P&L → danger + ▼", () => {
    const result = runningPnl(dep({ unrealized_pnl_usd: -300, realized_pnl_usd: -100 }));
    expect(result.value).toBe(-400);
    expect(result.tone).toBe<RiskTone>("danger");
    expect(result.glyph).toBe("▼");
  });

  it("zero combined P&L → gold + ▲ (non-negative = gold)", () => {
    const result = runningPnl(dep({ unrealized_pnl_usd: 0, realized_pnl_usd: 0 }));
    expect(result.value).toBe(0);
    expect(result.tone).toBe<RiskTone>("gold");
    expect(result.glyph).toBe("▲");
  });

  it("both null → neutral + —", () => {
    const result = runningPnl(dep({ unrealized_pnl_usd: null, realized_pnl_usd: null }));
    expect(result.value).toBeNull();
    expect(result.tone).toBe<RiskTone>("neutral");
    expect(result.glyph).toBe("—");
  });

  it("only unrealized present, realized null → uses unrealized", () => {
    const result = runningPnl(dep({ unrealized_pnl_usd: 800, realized_pnl_usd: null }));
    expect(result.value).toBe(800);
    expect(result.tone).toBe<RiskTone>("gold");
    expect(result.glyph).toBe("▲");
  });

  it("only realized present, unrealized null → uses realized", () => {
    const result = runningPnl(dep({ unrealized_pnl_usd: null, realized_pnl_usd: -50 }));
    expect(result.value).toBe(-50);
    expect(result.tone).toBe<RiskTone>("danger");
    expect(result.glyph).toBe("▼");
  });
});

// ─── dailyLossBufferTone ─────────────────────────────────────────────────────

describe("dailyLossBufferTone", () => {
  it("budget null → neutral (no daily-loss limit configured)", () => {
    expect(dailyLossBufferTone(500, null)).toBe<RiskTone>("neutral");
  });

  it("budget 0 → neutral (degenerate zero budget, no gradient)", () => {
    expect(dailyLossBufferTone(500, 0)).toBe<RiskTone>("neutral");
  });

  it("r=0.6 (remaining=300, budget=500) → gold (>50%)", () => {
    expect(dailyLossBufferTone(300, 500)).toBe<RiskTone>("gold");
  });

  it("r=0.4 (remaining=200, budget=500) → neutral (healthy decay, 25–50%)", () => {
    expect(dailyLossBufferTone(200, 500)).toBe<RiskTone>("neutral");
  });

  it("r=0.2 (remaining=100, budget=500) → warn (≤25%)", () => {
    expect(dailyLossBufferTone(100, 500)).toBe<RiskTone>("warn");
  });

  it("r=0 (remaining=0, budget=500) → danger (breached)", () => {
    expect(dailyLossBufferTone(0, 500)).toBe<RiskTone>("danger");
  });

  it("negative remaining (breach past zero) → danger", () => {
    expect(dailyLossBufferTone(-100, 500)).toBe<RiskTone>("danger");
  });

  it("remaining null → neutral (data absent)", () => {
    expect(dailyLossBufferTone(null, 500)).toBe<RiskTone>("neutral");
  });
});

// ─── formatEta ───────────────────────────────────────────────────────────────

describe("formatEta", () => {
  const NOW = 1_000_000_000_000; // fixed epoch ms for determinism

  it("null stopAt → null (no real limit)", () => {
    expect(formatEta(null, NOW)).toBeNull();
  });

  it("past stopAt → 'overdue'", () => {
    const past = new Date(NOW - 60_000).toISOString();
    expect(formatEta(past, NOW)).toBe("overdue");
  });

  it("exactly at now → 'overdue' (ms <= 0)", () => {
    const at = new Date(NOW).toISOString();
    expect(formatEta(at, NOW)).toBe("overdue");
  });

  it("~2h 15m in future → '~2h 15m left'", () => {
    const ms = (2 * 60 + 15) * 60 * 1000;
    const future = new Date(NOW + ms).toISOString();
    expect(formatEta(future, NOW)).toBe("~2h 15m left");
  });

  it("exactly 1h in future → '~1h 0m left'", () => {
    const future = new Date(NOW + 60 * 60 * 1000).toISOString();
    expect(formatEta(future, NOW)).toBe("~1h 0m left");
  });

  it("~45m in future → '~45m left'", () => {
    const future = new Date(NOW + 45 * 60 * 1000).toISOString();
    expect(formatEta(future, NOW)).toBe("~45m left");
  });

  it("~30s in future → '~30s left'", () => {
    const future = new Date(NOW + 30_000).toISOString();
    expect(formatEta(future, NOW)).toBe("~30s left");
  });
});

// ─── toneGlyph ───────────────────────────────────────────────────────────────

describe("toneGlyph", () => {
  it("gold → ✓", () => expect(toneGlyph("gold")).toBe("✓"));
  it("warn → ⚠", () => expect(toneGlyph("warn")).toBe("⚠"));
  it("danger → ✗", () => expect(toneGlyph("danger")).toBe("✗"));
  it("neutral → —", () => expect(toneGlyph("neutral")).toBe("—"));
});

// ─── formatUsd ───────────────────────────────────────────────────────────────

describe("formatUsd", () => {
  it("null → —", () => expect(formatUsd(null)).toBe("—"));
  it("0 → $0", () => expect(formatUsd(0)).toBe("$0"));
  it("10000 → $10,000", () => expect(formatUsd(10000)).toBe("$10,000"));
  it("1234567 → $1,234,567", () => expect(formatUsd(1234567)).toBe("$1,234,567"));
  it("negative values → negative $", () => expect(formatUsd(-500)).toBe("-$500"));
});

// ─── formatPct ───────────────────────────────────────────────────────────────

describe("formatPct", () => {
  it("null → —", () => expect(formatPct(null)).toBe("—"));
  it("4.2 → 4.2%", () => expect(formatPct(4.2)).toBe("4.2%"));
  it("0 → 0%", () => expect(formatPct(0)).toBe("0%"));
  it("15 → 15%", () => expect(formatPct(15)).toBe("15%"));
  it("3.14159 → 3.1% (1 decimal)", () => expect(formatPct(3.14159)).toBe("3.1%"));
});
