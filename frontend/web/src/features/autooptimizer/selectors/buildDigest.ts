import type { CycleRunDetail, CycleRunSummary, StatsRow } from "../api";
import type { Digest } from "../ui/EditorialHeadline";

const WEEK_MS = 7 * 86_400_000;

/** Compact token count: 820 / 12.4k / 31.8M / 2.1B */
export function formatTokensCompact(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${n}`;
}

/**
 * Trailing-7-day digest for the editorial headline.
 *
 * Experiments / kept / spend come from `StatsRow`s with `ts` in the window;
 * tokens come from `CycleRunSummary.input_tokens + output_tokens` (F23) over
 * cycles whose `last_created_at` is in the window. When every recent cycle's
 * token fields are null the tokens stat is omitted; when there are no recent
 * stats rows at all the digest is null.
 */
export function buildDigest(
  stats: StatsRow[],
  cycles: CycleRunSummary[],
  now: number = Date.now(),
): Digest | null {
  const cutoff = now - WEEK_MS;
  const recent = stats.filter((r) => {
    const t = new Date(r.ts).getTime();
    return Number.isFinite(t) && t >= cutoff;
  });
  if (recent.length === 0) return null;

  const experiments = recent.reduce(
    (n, r) => n + r.kept + r.suspect + r.dropped,
    0,
  );
  const kept = recent.reduce((n, r) => n + r.kept, 0);
  const spendUsd = recent.reduce((n, r) => n + (r.cost_usd ?? 0), 0);

  let tokenSum = 0;
  let anyTokens = false;
  for (const c of cycles) {
    const t = new Date(c.last_created_at).getTime();
    if (!Number.isFinite(t) || t < cutoff) continue;
    if (c.input_tokens == null && c.output_tokens == null) continue;
    anyTokens = true;
    tokenSum += (c.input_tokens ?? 0) + (c.output_tokens ?? 0);
  }

  return {
    experiments,
    kept,
    spend: `$${spendUsd.toFixed(2)}`,
    ...(anyTokens ? { tokens: formatTokensCompact(tokenSum) } : {}),
  };
}

/**
 * Best find of the last cycle: ΔSharpe from the cycle's `StatsRow.
 * best_delta_holdout` plus the hash of a kept (active) node from the cycle's
 * detail. Null whenever the data is thin — no stats row, no delta, no kept
 * node, or no detail loaded yet.
 */
export function deriveBestFind(
  stats: StatsRow[] | undefined,
  lastCycle: CycleRunSummary | null | undefined,
  detail: CycleRunDetail | undefined,
): { hash: string; delta: number } | null {
  if (!lastCycle || !detail || !stats) return null;
  const row = stats.find((r) => r.cycle_id === lastCycle.cycle_id);
  if (!row || row.best_delta_holdout == null) return null;
  const kept = detail.nodes?.find((n) => n.status === "active");
  if (!kept) return null;
  return { hash: kept.bundle_hash, delta: row.best_delta_holdout };
}
