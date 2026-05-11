// Settings API — fetchers for the v1 Settings tabs. Brokers / daemon /
// identity are read-only snapshots; providers is the only CRUD surface
// in this module.

import { apiFetch } from "./client";
import type {
  AddProviderRequest,
  BrokersReport,
  DaemonReport,
  FactoryResetReport,
  IdentityReport,
  ProviderRow,
  ProvidersReport,
  RegenIdentityReport,
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

// ─── Danger ops ────────────────────────────────────────────────────────────

export function dangerWipeDb(): Promise<WipeDbReport> {
  return apiFetch<WipeDbReport>("/api/settings/danger/wipe-db", {
    method: "POST",
    body: JSON.stringify({ confirm: DANGER_CONFIRM_TOKEN }),
  });
}

export function dangerRegenIdentity(): Promise<RegenIdentityReport> {
  return apiFetch<RegenIdentityReport>(
    "/api/settings/danger/regen-identity",
    {
      method: "POST",
      body: JSON.stringify({ confirm: DANGER_CONFIRM_TOKEN }),
    },
  );
}

export function dangerFactoryReset(): Promise<FactoryResetReport> {
  return apiFetch<FactoryResetReport>("/api/settings/danger/factory-reset", {
    method: "POST",
    body: JSON.stringify({ confirm: DANGER_CONFIRM_TOKEN }),
  });
}
