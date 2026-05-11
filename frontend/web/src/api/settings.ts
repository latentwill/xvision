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
