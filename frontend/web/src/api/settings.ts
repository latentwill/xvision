// Settings API — fetchers for the v1 Settings tabs. Brokers / daemon /
// identity are read-only snapshots; providers is the only CRUD surface
// in this module.

import { apiFetch } from "./client";
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
  return apiFetch<BrokersReport>("/api/settings/brokers");
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
  return apiFetch<AlpacaStored>("/api/settings/brokers/alpaca", {
    method: "POST",
    body: JSON.stringify(body),
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
  return apiFetch<AlpacaTestReport>(
    "/api/settings/brokers/alpaca/test-connection",
    { method: "POST" },
  );
}

// ─── Providers CRUD ────────────────────────────────────────────────────────

export function listProviders(): Promise<ProvidersReport> {
  return apiFetch<ProvidersReport>("/api/settings/providers");
}

export function addProvider(
  body: AddProviderRequest,
): Promise<ProviderRow> {
  return apiFetch<ProviderRow>("/api/settings/providers", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export function updateProvider(
  name: string,
  body: UpdateProviderRequest,
): Promise<ProviderRow> {
  return apiFetch<ProviderRow>(
    `/api/settings/providers/${encodeURIComponent(name)}`,
    {
      method: "PUT",
      body: JSON.stringify(body),
    },
  );
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
  return apiFetch<ProviderModelsReport>(
    `/api/settings/providers/${encodeURIComponent(name)}/models`,
  );
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
  return apiFetch<TestConnectionReport>(
    `/api/settings/providers/${encodeURIComponent(name)}/test-connection`,
    { method: "POST" },
  );
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
