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
 * Running P&L = unrealized_pnl_usd + realized_pnl_usd.
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
  const realized = d.realized_pnl_usd;

  if (unrealized === null && realized === null) {
    return { value: null, tone: "neutral", glyph: "—" };
  }

  const value = (unrealized ?? 0) + (realized ?? 0);
  if (value >= 0) {
    return { value, tone: "gold", glyph: "▲" };
  }
  return { value, tone: "danger", glyph: "▼" };
}

// ─── dailyLossBufferTone ─────────────────────────────────────────────────────

/**
 * Daily-loss buffer tone. Full §5.2 %-gradient using the budget denominator
 * now available on the contract as `daily_loss_budget_usd`.
 *
 * Gradient (r = remainingUsd / budgetUsd):
 *   budgetUsd null or ≤ 0  → "neutral" (no daily-loss limit configured)
 *   remainingUsd null      → "neutral" (data absent)
 *   r > 0.5                → "gold"    (healthy, >50% remaining)
 *   r > 0.25 && r ≤ 0.5   → "neutral" (healthy but decaying)
 *   r > 0 && r ≤ 0.25     → "warn"    (≤25% remaining)
 *   r ≤ 0                  → "danger"  (breached)
 */
export function dailyLossBufferTone(remainingUsd: number | null, budgetUsd: number | null): RiskTone {
  if (budgetUsd === null || budgetUsd <= 0) return "neutral";
  if (remainingUsd === null) return "neutral";
  const r = remainingUsd / budgetUsd;
  if (r > 0.5) return "gold";
  if (r > 0.25) return "neutral";
  if (r > 0) return "warn";
  return "danger";
}

// ─── formatEta ───────────────────────────────────────────────────────────────

/**
 * Humanize a wall-clock deadline (RFC-3339 `stop_at`) into a coarse ETA string.
 *
 * Returns null when `stopAt` is null (no real time limit → caller renders nothing).
 * Returns `"overdue"` when the deadline has passed (ms ≤ 0).
 * Otherwise returns a coarse "~Xh Ym left" / "~Xm left" / "~Xs left" string:
 *   ≥ 1 hour → "~Xh Ym left"
 *   ≥ 1 min  → "~Xm left"
 *   else     → "~Xs left"
 *
 * `nowMs` is injectable for deterministic tests (defaults to `Date.now()`).
 */
export function formatEta(stopAt: string | null, nowMs?: number): string | null {
  if (stopAt === null) return null;
  const ms = Date.parse(stopAt) - (nowMs ?? Date.now());
  if (ms <= 0) return "overdue";
  const totalSecs = Math.floor(ms / 1000);
  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hours >= 1) return `~${hours}h ${mins}m left`;
  if (mins >= 1) return `~${mins}m left`;
  return `~${secs}s left`;
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
