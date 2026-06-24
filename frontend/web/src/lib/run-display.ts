import type { RunSummary } from "@/api/types.gen";

export type NamedStrategy = {
  agent_id: string;
  display_name?: string | null;
  color?: string | null;
};

export type NamedScenario = {
  id: string;
  display_name?: string | null;
};

export type EvalRunLabels = {
  strategyName: string;
  scenarioName: string;
  title: string;
  subtitle: string;
  runId: string;
  strategyId: string;
  scenarioId: string;
};

export function evalRunLabels(
  summary: RunSummary,
  strategies: NamedStrategy[] = [],
  scenarios: NamedScenario[] = [],
): EvalRunLabels {
  const strategyName =
    summary.strategy?.display_name?.trim() ||
    displayStrategyName(summary.agent_id, strategies);
  const scenarioName =
    summary.scenario?.display_name?.trim() ||
    displayScenarioName(summary.scenario_id, scenarios, summary.mode, summary.live_config?.stop_policy);
  return {
    strategyName,
    scenarioName,
    title: strategyName,
    subtitle: `${summary.mode} · ${summary.status}`,
    runId: summary.id,
    strategyId: summary.agent_id,
    scenarioId: summary.scenario_id,
  };
}

export function displayStrategyName(
  id: string,
  strategies: NamedStrategy[] = [],
): string {
  return (
    strategies.find((s) => s.agent_id === id)?.display_name?.trim() ||
    fallbackName("Strategy", id)
  );
}

export function displayScenarioName(
  id: string,
  scenarios: NamedScenario[] = [],
  mode?: string,
  stopPolicy?: { bar_limit?: number | null; decision_limit?: number | null; time_limit_secs?: bigint | null; trade_limit?: number | null },
): string {
  if (mode === 'live') {
    const parts: string[] = [];
    if (stopPolicy?.time_limit_secs) {
      const hours = Math.round(Number(stopPolicy.time_limit_secs) / 3600);
      parts.push(`${hours}h`);
    }
    if (stopPolicy?.bar_limit) parts.push(`${stopPolicy.bar_limit} bars`);
    if (stopPolicy?.decision_limit) parts.push(`${stopPolicy.decision_limit} decisions`);
    if (stopPolicy?.trade_limit) parts.push(`${stopPolicy.trade_limit} trades`);
    return parts.length > 0 ? `Forward Test · ${parts.join(' · ')}` : 'Forward Test';
  }
  const found = scenarios.find((s) => s.id === id);
  return found?.display_name?.trim() || id.slice(0, 8);
}

export function shortId(id: string, len = 10): string {
  void len;
  return id;
}

// Per-(strategy, scenario) sequence number, sorted by started_at ascending,
// with `id` as a stable tiebreaker. Falls back to {1,1} when siblings is
// empty so a brand-new run reads as Run #1 before the siblings list loads.
export function evalRunOrdinal(
  summary: RunSummary,
  siblings: RunSummary[],
): { index: number; total: number } {
  const samePair = siblings.filter(
    (r) =>
      r.agent_id === summary.agent_id &&
      r.scenario_id === summary.scenario_id,
  );
  if (samePair.length === 0) {
    return { index: 1, total: 1 };
  }
  const sorted = [...samePair].sort((a, b) => {
    const at = a.started_at ?? "";
    const bt = b.started_at ?? "";
    if (at !== bt) return at < bt ? -1 : 1;
    return a.id < b.id ? -1 : 1;
  });
  const idx = sorted.findIndex((r) => r.id === summary.id);
  return {
    index: (idx >= 0 ? idx : sorted.length - 1) + 1,
    total: sorted.length,
  };
}

// "Run #3 · May 18, 14:02" (or "Run #3/7 · …" when more than one run
// exists for the same strategy+scenario pair). Derived entirely from
// existing `RunSummary` fields — no backend contract change.
export function evalRunDisambiguator(
  summary: RunSummary,
  siblings: RunSummary[],
): string {
  const { index, total } = evalRunOrdinal(summary, siblings);
  const stamp = formatDisambiguatorTimestamp(summary.started_at);
  const ordinal = total > 1 ? `Run #${index}/${total}` : `Run #${index}`;
  return stamp ? `${ordinal} · ${stamp}` : ordinal;
}

function formatDisambiguatorTimestamp(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function fallbackName(kind: string, id: string): string {
  const normalized = id
    .replace(/^sc[_-]/i, "")
    .replace(/[_-]+/g, " ")
    .trim();
  if (normalized && !/^[0-9A-Z]{10,}$/i.test(normalized)) {
    return normalized.replace(/\b\w/g, (ch) => ch.toUpperCase());
  }
  return `${kind} ${id}`;
}
