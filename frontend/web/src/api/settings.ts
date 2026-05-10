// Settings API — read-only fetchers for the v1 Settings tabs (brokers,
// daemon, identity). Providers and danger-zone CRUD live elsewhere.

import { apiFetch } from "./client";
import type {
  BrokersReport,
  DaemonReport,
  IdentityReport,
} from "./types.gen";

export const settingsKeys = {
  all: ["settings"] as const,
  brokers: () => [...settingsKeys.all, "brokers"] as const,
  daemon: () => [...settingsKeys.all, "daemon"] as const,
  identity: () => [...settingsKeys.all, "identity"] as const,
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
