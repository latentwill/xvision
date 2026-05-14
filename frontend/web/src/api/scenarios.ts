// Scenarios API — typed fetchers against `engine::api::scenarios::*`.

import { apiFetch } from "./client";
import type {
  CreateScenarioRequest,
  ListScenariosFilter,
  Scenario,
  ScenarioMutations,
} from "./types.gen";

type ScenariosListResponse = {
  items: Scenario[];
};

export const scenarioKeys = {
  all: ["scenarios"] as const,
  list: (filter?: ListScenariosFilter) =>
    [...scenarioKeys.all, "list", filter ?? {}] as const,
  detail: (id: string) => [...scenarioKeys.all, "detail", id] as const,
};

export function listScenarios(filter?: ListScenariosFilter): Promise<Scenario[]> {
  const params = new URLSearchParams();
  if (filter?.source) params.set("source", String(filter.source));
  if (filter?.include_archived) params.set("include_archived", "true");
  if (filter?.parent_scenario_id)
    params.set("parent_scenario_id", filter.parent_scenario_id);
  filter?.tags?.forEach((t) => params.append("tags", t));
  const qs = params.toString();
  const path = qs ? `/api/scenarios?${qs}` : "/api/scenarios";
  return apiFetch<ScenariosListResponse>(path).then((r) => r.items);
}

export function getScenario(id: string): Promise<Scenario> {
  return apiFetch<Scenario>(`/api/scenarios/${encodeURIComponent(id)}`);
}

export function createScenario(req: CreateScenarioRequest): Promise<Scenario> {
  return apiFetch<Scenario>("/api/scenarios", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export function cloneScenario(
  id: string,
  mutations: ScenarioMutations,
): Promise<Scenario> {
  return apiFetch<Scenario>(`/api/scenarios/${encodeURIComponent(id)}/clone`, {
    method: "POST",
    body: JSON.stringify(mutations),
  });
}

export function archiveScenario(id: string): Promise<void> {
  return apiFetch<void>(`/api/scenarios/${encodeURIComponent(id)}/archive`, {
    method: "POST",
  });
}

export function deleteScenario(id: string): Promise<void> {
  return apiFetch<void>(`/api/scenarios/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}
