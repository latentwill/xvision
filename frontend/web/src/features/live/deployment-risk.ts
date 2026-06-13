// frontend/web/src/features/live/deployment-risk.ts
//
// Pure capital-risk selectors for the CT5 live-deployments strips (S0 foundation).
// No JSX, no fetch, no side effects. Injectable nowMs for deterministic tests.
// Consumed by the 8s4/n0k/awm strips built in CT5 S1+.
//
// HONESTY MANDATE: all values come from the wire contract as-is.
// paper/testnet = simulated — never present as "real" money.
//
// Tone tokens map to the wired Tailwind tokens:
//   gold    → text-gold   (healthy)
//   warn    → text-warn   (caution)
//   danger  → text-danger (breach / loss)
//   neutral → no tone class (data absent)

import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";

export type RiskTone = "gold" | "warn" | "danger" | "neutral";

// ─── drawdownTone ─────────────────────────────────────────────────────────────

/**
 * Drawdown % tone per spec §5.2:
 *   <5   → gold (healthy)
 *   5–15 → warn (caution)
 *   ≥15  → danger (breach)
 *   null → neutral (data absent)
 *
 * `drawdown_pct` is a percentage in the range 0–100 on the wire contract,
 * where 0 = no drawdown and 100 = full capital lost.
 */
export function drawdownTone(drawdownPct: number | null): RiskTone {
  if (drawdownPct === null) return "neutral";
  if (drawdownPct < 5) return "gold";
  if (drawdownPct < 15) return "warn";
  return "danger";
}

// ─── runningPnl ──────────────────────────────────────────────────────────────

/**
 * Running P&L = unrealized_pnl_usd + realized_today_usd.
 *   ≥0   → gold (▲) — includes zero (non-loss)
 *   <0   → danger (▼)
 *   both null → neutral (—)
 *
 * If only one component is null it is treated as 0 for the sum, so a single
 * non-null leg still produces a meaningful signal.
 */
export function runningPnl(d: LiveDeploymentSummary): {
  value: number | null;
  tone: RiskTone;
  glyph: "▲" | "▼" | "—";
} {
  const unrealized = d.unrealized_pnl_usd;
  const realizedToday = d.realized_today_usd;

  if (unrealized === null && realizedToday === null) {
    return { value: null, tone: "neutral", glyph: "—" };
  }

  const value = (unrealized ?? 0) + (realizedToday ?? 0);
  if (value >= 0) {
    return { value, tone: "gold", glyph: "▲" };
  }
  return { value, tone: "danger", glyph: "▼" };
}

// ─── dailyLossBufferTone ─────────────────────────────────────────────────────

/**
 * Daily-loss buffer tone. The contract exposes remaining $ but NOT the budget
 * denominator, so the >50%/≤25% gradient is not computable yet.
 *
 * Breach case only (spec §5.2 minimum):
 *   remaining > 0  → gold (healthy)
 *   remaining ≤ 0  → danger (breach)
 *   null           → neutral (no budget configured)
 *
 * TODO: full %-gradient needs a daily_loss_budget_usd field on the contract (follow-up).
 */
export function dailyLossBufferTone(remainingUsd: number | null): RiskTone {
  if (remainingUsd === null) return "neutral";
  if (remainingUsd > 0) return "gold";
  return "danger";
}

// ─── toneGlyph ───────────────────────────────────────────────────────────────

/** Status glyph for a tone (✓ gold, ⚠ warn, ✗ danger, — neutral). */
export function toneGlyph(tone: RiskTone): "✓" | "⚠" | "✗" | "—" {
  switch (tone) {
    case "gold":    return "✓";
    case "warn":    return "⚠";
    case "danger":  return "✗";
    case "neutral": return "—";
  }
}

// ─── formatters ──────────────────────────────────────────────────────────────

/**
 * Plain unsigned USD with thousands separators, no cents (e.g. `"$10,000"`).
 * Returns `"—"` for null. Negative values render as `"-$500"`.
 */
export function formatUsd(n: number | null): string {
  if (n === null) return "—";
  const abs = Math.abs(n);
  const formatted = `$${abs.toLocaleString("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 0,
  })}`;
  return n < 0 ? `-${formatted}` : formatted;
}

/**
 * Percentage with one decimal place (e.g. `"4.2%"`). Returns `"—"` for null.
 * Input is a percentage value (0–100), not a fraction.
 */
export function formatPct(n: number | null): string {
  if (n === null) return "—";
  // Use toPrecision-style via toLocaleString to avoid floating-point drift;
  // one decimal place, strip trailing zeros for whole numbers.
  const formatted = n.toLocaleString("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 1,
  });
  return `${formatted}%`;
}
