// Settings API — fetchers for the v1 Settings tabs. Brokers / daemon /
// identity are read-only snapshots; providers is the only CRUD surface
// in this module.

import { apiFetch } from "./client";
import {
  createTrace,
  durationSince,
  errorSummary,
  safeUrlHost,
} from "@/lib/logger";
import type {
  AddProviderRequest,
  AlpacaTestReport,
  BrokersReport,
  FactoryResetReport,
  ProviderModelsReport,
  ProviderRow,
  ProvidersReport,
  TestConnectionReport,
  UpdateProviderRequest,
  WipeDbReport,
} from "./types.gen";

// Confirm string the engine expects. Mirrored from
// `xvision_engine::api::settings::danger::CONFIRM_TOKEN`.
const DANGER_CONFIRM_TOKEN = "yes-i-am-sure";

export const settingsKeys = {
  all: ["settings"] as const,
  brokers: () => [...settingsKeys.all, "brokers"] as const,
  daemon: () => [...settingsKeys.all, "daemon"] as const,
  identity: () => [...settingsKeys.all, "identity"] as const,
  providers: () => [...settingsKeys.all, "providers"] as const,
  providerModels: (name: string) =>
    [...settingsKeys.all, "providers", name, "models"] as const,
};

export function getBrokers(): Promise<BrokersReport> {
  const trace = createTrace("settings");
  const started = performance.now();
  trace.debug("settings.broker.load");
  return apiFetch<BrokersReport>("/api/settings/brokers")
    .then((report) => {
      trace.debug("settings.broker.load.ok", {
        duration_ms: durationSince(started),
        broker_count: 2,
      });
      return report;
    })
    .catch((err) => {
      trace.error("settings.broker.load.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

// ─── Brokers (Alpaca) CRUD ─────────────────────────────────────────────────

// Hand-written wire shapes for the Alpaca-credentials surface. The
// engine-side `AlpacaStored` and `SetAlpacaReq` types don't carry
// `derive(TS)` (secrets shouldn't accidentally leak into the generated
// surface); mirror them here.
export type SetAlpacaRequest = {
  api_key_id: string;
  api_secret_key: string;
  base_url: string | null;
};

export type AlpacaStored = {
  stored: boolean;
  stored_key_id_suffix: string | null;
  base_url: string | null;
};

export function setAlpacaCredentials(
  body: SetAlpacaRequest,
): Promise<AlpacaStored> {
  const trace = createTrace("settings", {
    broker: "alpaca",
    base_url_host: safeUrlHost(body.base_url),
  });
  const started = performance.now();
  trace.info("settings.broker.save");
  return apiFetch<AlpacaStored>("/api/settings/brokers/alpaca", {
    method: "POST",
    body: JSON.stringify(body),
  })
    .then((stored) => {
      trace.info("settings.broker.save.ok", {
        stored: stored.stored,
        duration_ms: durationSince(started),
      });
      return stored;
    })
    .catch((err) => {
      trace.error("settings.broker.save.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function clearAlpacaCredentials(): Promise<void> {
  return apiFetch<void>("/api/settings/brokers/alpaca", {
    method: "DELETE",
  });
}

/// Connectivity probe for Alpaca — calls `/v2/account` with the stored
/// (or env-var fallback) credentials. Network/auth failures surface in
/// `error` rather than as HTTP errors so the UI renders an inline pill.
export function testAlpacaConnection(): Promise<AlpacaTestReport> {
  const trace = createTrace("settings", { broker: "alpaca" });
  const started = performance.now();
  trace.info("settings.broker.test");
  return apiFetch<AlpacaTestReport>(
    "/api/settings/brokers/alpaca/test-connection",
    { method: "POST" },
  ).then((report) => {
    trace.info("settings.broker.test.ok", {
      ok: report.ok,
      duration_ms: durationSince(started),
    });
    return report;
  });
}

// ─── Providers CRUD ────────────────────────────────────────────────────────

export function listProviders(): Promise<ProvidersReport> {
  const trace = createTrace("settings");
  const started = performance.now();
  trace.debug("settings.providers.load");
  return apiFetch<ProvidersReport>("/api/settings/providers").then((report) => {
    trace.debug("settings.providers.load.ok", {
      duration_ms: durationSince(started),
      provider_count: report.providers.length,
    });
    return report;
  });
}

export function addProvider(
  body: AddProviderRequest,
): Promise<ProviderRow> {
  const trace = createTrace("settings", {
    provider: body.name,
    kind: body.kind,
    base_url_host: safeUrlHost(body.base_url),
  });
  const started = performance.now();
  trace.info("settings.provider.create");
  return apiFetch<ProviderRow>("/api/settings/providers", {
    method: "POST",
    body: JSON.stringify(body),
  })
    .then((row) => {
      trace.info("settings.provider.create.ok", {
        enabled_model_count: row.enabled_models.length,
        duration_ms: durationSince(started),
      });
      return row;
    })
    .catch((err) => {
      trace.error("settings.provider.create.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function updateProvider(
  name: string,
  body: UpdateProviderRequest,
): Promise<ProviderRow> {
  const trace = createTrace("settings", {
    provider: name,
    kind: body.kind,
    base_url_host: safeUrlHost(body.base_url),
  });
  const started = performance.now();
  trace.info("settings.provider.update");
  return apiFetch<ProviderRow>(
    `/api/settings/providers/${encodeURIComponent(name)}`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  )
    .then((row) => {
      trace.info("settings.provider.update.ok", {
        enabled_model_count: row.enabled_models.length,
        duration_ms: durationSince(started),
      });
      return row;
    })
    .catch((err) => {
      trace.error("settings.provider.update.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

export function removeProvider(name: string): Promise<void> {
  return apiFetch<void>(
    `/api/settings/providers/${encodeURIComponent(name)}`,
    { method: "DELETE" },
  );
}

/// Fetch the provider's upstream model catalog. Backend caches for ~5
/// minutes; large lists (OpenRouter) still return quickly on a cache
/// hit. 400/404 errors bubble back via `ApiError`.
export function listProviderModels(
  name: string,
): Promise<ProviderModelsReport> {
  const trace = createTrace("settings", { provider: name });
  const started = performance.now();
  trace.debug("settings.provider.models.load");
  return apiFetch<ProviderModelsReport>(
    `/api/settings/providers/${encodeURIComponent(name)}/models`,
  )
    .then((report) => {
      trace.debug("settings.provider.models.load.ok", {
        model_count: report.models.length,
        duration_ms: durationSince(started),
      });
      return report;
    })
    .catch((err) => {
      trace.error("settings.provider.models.load.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

/// Persist the operator's curated subset of models for a provider.
/// Returns the refreshed `ProviderRow` so the caller can swap the cached
/// row without an extra GET.
export function setEnabledModels(
  name: string,
  models: string[],
): Promise<ProviderRow> {
  return apiFetch<ProviderRow>(
    `/api/settings/providers/${encodeURIComponent(name)}/enabled-models`,
    {
      method: "PUT",
      body: JSON.stringify({ models }),
    },
  );
}

/// Connectivity probe — POST that calls the provider's catalog endpoint
/// and returns `{ ok, latency_ms, model_count, error? }`. Network/auth
/// failures land in `error` (with `ok = false`) rather than as HTTP
/// errors, so the UI renders an inline pill either way.
export function testProviderConnection(
  name: string,
): Promise<TestConnectionReport> {
  const trace = createTrace("settings", { provider: name });
  const started = performance.now();
  trace.info("settings.provider.test.start");
  return apiFetch<TestConnectionReport>(
    `/api/settings/providers/${encodeURIComponent(name)}/test-connection`,
    { method: "POST" },
  ).then((report) => {
    trace.info(report.ok ? "settings.provider.test.ok" : "settings.provider.test.error", {
      ok: report.ok,
      model_count: report.model_count,
      duration_ms: durationSince(started),
    });
    return report;
  });
}

// ─── Danger ops ────────────────────────────────────────────────────────────

export function dangerWipeDb(): Promise<WipeDbReport> {
  return apiFetch<WipeDbReport>("/api/settings/danger/wipe-db", {
    method: "POST",
    body: JSON.stringify({ confirm: DANGER_CONFIRM_TOKEN }),
  });
}

export function dangerFactoryReset(): Promise<FactoryResetReport> {
  return apiFetch<FactoryResetReport>("/api/settings/danger/factory-reset", {
    method: "POST",
    body: JSON.stringify({ confirm: DANGER_CONFIRM_TOKEN }),
  });
}
