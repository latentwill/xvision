// Scenarios API — typed fetchers against `engine::api::scenarios::*`.

import { apiFetch } from "./client";
import {
  createTrace,
  durationSince,
  errorSummary,
} from "@/lib/logger";
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
  const trace = createTrace("scenario", {
    asset: req.asset.map((a) => a.symbol),
    granularity: req.granularity,
    from: req.time_window.start,
    to: req.time_window.end,
  });
  const started = performance.now();
  trace.info("scenario.create.start");
  return apiFetch<Scenario>("/api/scenarios", {
    method: "POST",
    body: JSON.stringify(req),
  })
    .then((scenario) => {
      trace.info("scenario.create.ok", {
        scenario_id: scenario.id,
        duration_ms: durationSince(started),
      });
      return scenario;
    })
    .catch((err) => {
      trace.error("scenario.create.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function cloneScenario(
  id: string,
  mutations: ScenarioMutations,
): Promise<Scenario> {
  const trace = createTrace("scenario", { scenario_id: id });
  const started = performance.now();
  trace.info("scenario.clone.start", {
    mutation_keys: Object.keys(mutations),
  });
  return apiFetch<Scenario>(`/api/scenarios/${encodeURIComponent(id)}/clone`, {
    method: "POST",
    body: JSON.stringify(mutations),
  })
    .then((scenario) => {
      trace.info("scenario.clone.ok", {
        scenario_id: scenario.id,
        duration_ms: durationSince(started),
      });
      return scenario;
    })
    .catch((err) => {
      trace.error("scenario.clone.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function archiveScenario(id: string): Promise<void> {
  const trace = createTrace("scenario", { scenario_id: id });
  const started = performance.now();
  trace.info("scenario.archive.start");
  return apiFetch<void>(`/api/scenarios/${encodeURIComponent(id)}/archive`, {
    method: "POST",
  })
    .then((result) => {
      trace.info("scenario.archive.ok", {
        duration_ms: durationSince(started),
      });
      return result;
    })
    .catch((err) => {
      trace.error("scenario.archive.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function deleteScenario(id: string): Promise<void> {
  const trace = createTrace("scenario", { scenario_id: id });
  const started = performance.now();
  trace.info("scenario.delete.start");
  return apiFetch<void>(`/api/scenarios/${encodeURIComponent(id)}`, {
    method: "DELETE",
  })
    .then((result) => {
      trace.info("scenario.delete.ok", {
        duration_ms: durationSince(started),
      });
      return result;
    })
    .catch((err) => {
      trace.error("scenario.delete.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}
