import type { RunSummary } from "@/api/types.gen";

export type NamedStrategy = {
  agent_id: string;
  display_name?: string | null;
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
  shortRunId: string;
  strategyId: string;
  shortStrategyId: string;
  scenarioId: string;
  shortScenarioId: string;
};

export function evalRunLabels(
  summary: RunSummary,
  strategies: NamedStrategy[] = [],
  scenarios: NamedScenario[] = [],
): EvalRunLabels {
  const strategyName = displayStrategyName(summary.agent_id, strategies);
  const scenarioName = displayScenarioName(summary.scenario_id, scenarios);
  return {
    strategyName,
    scenarioName,
    title: `${strategyName} on ${scenarioName}`,
    subtitle: `${summary.mode} · ${summary.status}`,
    runId: summary.id,
    shortRunId: shortId(summary.id),
    strategyId: summary.agent_id,
    shortStrategyId: shortId(summary.agent_id),
    scenarioId: summary.scenario_id,
    shortScenarioId: shortId(summary.scenario_id),
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
): string {
  return (
    scenarios.find((s) => s.id === id)?.display_name?.trim() ||
    fallbackName("Scenario", id)
  );
}

export function shortId(id: string, len = 10): string {
  return id.length > len ? `${id.slice(0, len)}...` : id;
}

function fallbackName(kind: string, id: string): string {
  const normalized = id
    .replace(/^sc[_-]/i, "")
    .replace(/[_-]+/g, " ")
    .trim();
  if (normalized && !/^[0-9A-Z]{10,}$/i.test(normalized)) {
    return normalized.replace(/\b\w/g, (ch) => ch.toUpperCase());
  }
  return `${kind} ${shortId(id, 8)}`;
}
