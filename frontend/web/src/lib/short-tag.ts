// Short-tag helper used by the multi-eval capsule.
//
// The Capsule design (docs/design/Capsule · Multi-Eval.html) renders each
// concurrent eval as `strategy·scenario` — short, lowercase, recognisable at
// a glance. Real strategy/scenario names are not constrained to that form, so
// we derive a stable abbreviation from each name: the first alphanumeric word,
// lowercased, truncated to 4 characters. Falls back to a hex id-slice when no
// name is available so siblings stay visually distinct.

const MAX_LEN = 4;

/**
 * Derive a 1-4 character short tag from a human name. Returns `null` when
 * the input has no alphanumeric content (caller should fall back to an
 * id-slice).
 *
 * Examples:
 *   "mean-reversion-v3"   → "mean"
 *   "Momentum Breakout v2"→ "mome"
 *   "pairs-stat-arb-v7"   → "pair"
 *   "VIX Spike 2018-02"   → "vix"
 *   "  "                  → null
 */
export function shortenName(name: string | null | undefined): string | null {
  if (!name) return null;
  const match = name.match(/[A-Za-z0-9]+/);
  if (!match) return null;
  return match[0].toLowerCase().slice(0, MAX_LEN);
}

/**
 * Build a `strategy·scenario` short tag from optional names and a pair of ids
 * for fallback. Used by the capsule when one or both names haven't loaded
 * yet — keeps the row legible while the lookup queries are in flight.
 */
export function shortTag(
  agentName: string | null | undefined,
  scenarioName: string | null | undefined,
  agentIdFallback: string,
  scenarioIdFallback: string,
): string {
  const a = shortenName(agentName) ?? agentIdFallback.slice(-6) ?? "agent";
  const s = shortenName(scenarioName) ?? scenarioIdFallback.slice(-6) ?? "scen";
  return `${a}·${s}`;
}
