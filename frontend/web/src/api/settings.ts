// Settings API — fetchers for the v1 Settings tabs. Brokers / daemon /
// identity are read-only snapshots; providers is the only CRUD surface
// in this module.

import { apiFetch } from "./client";
import type {
  AddProviderRequest,
  BrokersReport,
  DaemonReport,
  IdentityReport,
  ProviderRow,
  ProvidersReport,
} from "./types.gen";

export const settingsKeys = {
  all: ["settings"] as const,
  brokers: () => [...settingsKeys.all, "brokers"] as const,
  daemon: () => [...settingsKeys.all, "daemon"] as const,
  identity: () => [...settingsKeys.all, "identity"] as const,
  providers: () => [...settingsKeys.all, "providers"] as const,
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

export function getDaemon(): Promise<DaemonReport> {
  return apiFetch<DaemonReport>("/api/settings/daemon");
}

export function getIdentity(): Promise<IdentityReport> {
  return apiFetch<IdentityReport>("/api/settings/identity");
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

export function removeProvider(name: string): Promise<void> {
  return apiFetch<void>(
    `/api/settings/providers/${encodeURIComponent(name)}`,
    { method: "DELETE" },
  );
}
