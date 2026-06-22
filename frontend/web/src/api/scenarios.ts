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
  total: number;
};

/// Paged response envelope returned by `listScenariosPaged`.
export type ScenariosPage = {
  items: Scenario[];
  total: number;
};

export const scenarioKeys = {
  all: ["scenarios"] as const,
  /// Cache key includes the full filter (including `limit`/`offset`)
  /// so page changes refetch instead of slicing the same response.
  list: (filter?: ListScenariosFilter) =>
    [...scenarioKeys.all, "list", filter ?? {}] as const,
  detail: (id: string) => [...scenarioKeys.all, "detail", id] as const,
};

function buildScenariosListUrl(filter?: ListScenariosFilter): string {
  const params = new URLSearchParams();
  if (filter?.source) params.set("source", String(filter.source));
  if (filter?.include_archived) params.set("include_archived", "true");
  if (filter?.parent_scenario_id)
    params.set("parent_scenario_id", filter.parent_scenario_id);
  if (filter?.limit !== undefined && filter.limit !== null)
    params.set("limit", String(filter.limit));
  if (filter?.offset !== undefined && filter.offset !== null)
    params.set("offset", String(filter.offset));
  filter?.tags?.forEach((t) => params.append("tags", t));
  filter?.exclude_tags?.forEach((t) => params.append("exclude_tags", t));
  const qs = params.toString();
  return qs ? `/api/scenarios?${qs}` : "/api/scenarios";
}

export function listScenarios(filter?: ListScenariosFilter): Promise<Scenario[]> {
  return apiFetch<ScenariosListResponse>(buildScenariosListUrl(filter)).then(
    (r) => r.items,
  );
}

/// Paged variant — preserves the `total` field so the dashboard's
/// pager can render "page X of N" without a second round-trip.
export function listScenariosPaged(
  filter?: ListScenariosFilter,
): Promise<ScenariosPage> {
  return apiFetch<ScenariosListResponse>(buildScenariosListUrl(filter)).then(
    (r) => ({ items: r.items, total: r.total }),
  );
}

export function getScenario(id: string): Promise<Scenario> {
  return apiFetch<Scenario>(`/api/scenarios/${encodeURIComponent(id)}`);
}

export function createScenario(req: CreateScenarioRequest): Promise<Scenario> {
  const trace = createTrace("scenario", {
    asset_class: req.asset_class,
    quote_currency: req.quote_currency,
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
