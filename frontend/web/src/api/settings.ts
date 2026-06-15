// Settings API — fetchers for the v1 Settings tabs. Brokers / daemon /
// identity are read-only snapshots; providers is the only CRUD surface
// in this module.

import { ApiError, apiFetch } from "./client";
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
  Catalog,
  FactoryResetReport,
  IdentityReport,
  MemoryReport,
  MemoryStatus,
  ObservabilityReport,
  ProfileReport,
  ProviderModelsReport,
  ProviderRow,
  ProvidersReport,
  RefreshOutcome,
  ResetWorkspaceReport,
  RetentionModeDto,
  TestConnectionReport,
  UpdateMemoryRequest,
  UpdateProfileRequest,
  UpdateProviderRequest,
} from "./types.gen";

// Per-route confirm phrases. Mirrored from
// `xvision_engine::api::settings::danger::{RESET_WORKSPACE_CONFIRM,
// FACTORY_RESET_CONFIRM, REGEN_IDENTITY_CONFIRM}`.
//
// qa-dashboard-auth-hardening (2026-05-17): these constants are
// re-exported so the UI can display the phrase for discoverability,
// but the operator must still type it themselves — the typed text is
// what travels on the wire to the backend, which validates against
// the per-route constant. We deliberately do NOT auto-fill the
// payload with these strings.
//
// F-4 (2026-05-18): the legacy `DANGER_WIPE_DB_PHRASE` ("WIPE
// DATABASE") is gone — the route was replaced by the selective
// `reset_workspace` op. `RESET WORKSPACE` is the new phrase.
export const DANGER_RESET_WORKSPACE_PHRASE = "RESET WORKSPACE";
export const DANGER_FACTORY_RESET_PHRASE = "FACTORY RESET";
export const DANGER_REGEN_IDENTITY_PHRASE = "REGEN IDENTITY";

export const settingsKeys = {
  all: ["settings"] as const,
  brokers: () => [...settingsKeys.all, "brokers"] as const,
  daemon: () => [...settingsKeys.all, "daemon"] as const,
  identity: () => [...settingsKeys.all, "identity"] as const,
  observability: () => [...settingsKeys.all, "observability"] as const,
  memory: () => [...settingsKeys.all, "memory"] as const,
  memoryStatus: () => [...settingsKeys.all, "memory", "status"] as const,
  profile: () => [...settingsKeys.all, "profile"] as const,
  providers: () => [...settingsKeys.all, "providers"] as const,
  providerModels: (name: string) =>
    [...settingsKeys.all, "providers", name, "models"] as const,
  providerCatalog: (name: string) =>
    [...settingsKeys.all, "providers", name, "catalog"] as const,
};

// ─── Identity (on-chain NFT snapshot) ─────────────────────────────────────

export function getIdentity(): Promise<IdentityReport> {
  return apiFetch<IdentityReport>("/api/settings/identity");
}

// ─── Observability (trace retention) ──────────────────────────────────────

export function getObservability(): Promise<ObservabilityReport> {
  return apiFetch<ObservabilityReport>("/api/settings/observability");
}

export function setObservabilityMode(
  mode: RetentionModeDto,
): Promise<ObservabilityReport> {
  const trace = createTrace("settings", { retention_mode: mode });
  const started = performance.now();
  trace.info("settings.observability.set");
  return apiFetch<ObservabilityReport>("/api/settings/observability", {
    method: "PUT",
    body: JSON.stringify({ mode }),
  })
    .then((report) => {
      trace.info("settings.observability.set.ok", {
        mode: report.mode,
        duration_ms: durationSince(started),
      });
      return report;
    })
    .catch((err) => {
      trace.error("settings.observability.set.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

// ─── Memory (Cortex embedder + chat/optimizer toggles + status) ───────────

export function getMemorySettings(): Promise<MemoryReport> {
  return apiFetch<MemoryReport>("/api/settings/memory");
}

export function updateMemorySettings(
  req: UpdateMemoryRequest,
): Promise<MemoryReport> {
  const trace = createTrace("settings", {
    embedder: req.embedder,
    chat_enabled: req.chat_enabled,
    optimizer_enabled: req.optimizer_enabled,
  });
  const started = performance.now();
  trace.info("settings.memory.set");
  return apiFetch<MemoryReport>("/api/settings/memory", {
    method: "PUT",
    body: JSON.stringify(req),
  })
    .then((report) => {
      trace.info("settings.memory.set.ok", {
        embedder: report.embedder,
        chat_enabled: report.chat_enabled,
        optimizer_enabled: report.optimizer_enabled,
        duration_ms: durationSince(started),
      });
      return report;
    })
    .catch((err) => {
      trace.error("settings.memory.set.error", {
        duration_ms: durationSince(started),
        error: errorSummary(err),
      });
      throw err;
    });
}

// ─── Profile (operator display name / handle) ─────────────────────────────

export function getProfile(): Promise<ProfileReport> {
  return apiFetch<ProfileReport>("/api/settings/profile");
}

export function updateProfile(
  req: UpdateProfileRequest,
): Promise<ProfileReport> {
  return apiFetch<ProfileReport>("/api/settings/profile", {
    method: "PUT",
    body: JSON.stringify(req),
  });
}

export function getMemoryStatus(): Promise<MemoryStatus> {
  return apiFetch<MemoryStatus>("/api/settings/memory/status");
}

export function getBrokers(): Promise<BrokersReport> {
  const trace = createTrace("settings");
  const started = performance.now();
  trace.debug("settings.broker.load");
  return apiFetch<BrokersReport>("/api/settings/brokers")
    .then((report) => {
      trace.debug("settings.broker.load.ok", {
        duration_ms: durationSince(started),
        broker_count: 3,
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

// ─── Brokers (Alpaca, Byreal) CRUD ────────────────────────────────────────

// Byreal is a report-only broker surface — credentials are env-var-only
// (BYREAL_PRIVATE_KEY / BYREAL_NETWORK / BYREAL_ACCOUNT). The frontend
// surfaces a read-only BrokerCard for it (mirroring the Orderly treatment);
// there is no `setByrealCredentials` because the backend exposes no store
// endpoint for Byreal at this revision.

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

// Byreal stored creds. The `private_key` MUST be a Hyperliquid trading-only
// agent key (cannot withdraw) — see the non-custodial design. Mirrors the
// engine `SetByrealReq` / `ByrealStored` (no `derive(TS)` so the secret never
// leaks into the generated surface).
export type SetByrealRequest = {
  private_key: string;
  network: string | null;
  account: string | null;
};

export type ByrealStored = {
  stored: boolean;
  stored_key_id_suffix: string | null;
  network: string | null;
};

export function setByrealCredentials(
  body: SetByrealRequest,
): Promise<ByrealStored> {
  const trace = createTrace("settings", { broker: "byreal", network: body.network });
  const started = performance.now();
  trace.info("settings.broker.save");
  return apiFetch<ByrealStored>("/api/settings/brokers/byreal", {
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

export function clearByrealCredentials(): Promise<void> {
  return apiFetch<void>("/api/settings/brokers/byreal", {
    method: "DELETE",
  });
}

// Degen Arena (Virtuals) stored creds. The `apiKey` MUST be a Hyperliquid
// trade-only agent key (`0x` + 64 hex, cannot withdraw); `accountAddress` is
// the master account (`0x` + 40 hex). Mirrors the engine `SetDegenArenaReq` /
// `DegenArenaStored` (no `derive(TS)` so the secret never leaks into the
// generated surface). The backend ingest route is shared with the /live deploy
// strip: POST/DELETE /api/live/deploy/degen-arena.
export type SetDegenArenaRequest = {
  apiKey: string;
  accountAddress: string;
  network: string;
};

export type DegenArenaStored = {
  ok: boolean;
  stored_key_suffix?: string | null;
  network?: string | null;
};

// Deliberately uses a RAW `fetch` (not `apiFetch`, which logs body summaries)
// so the trade-only HL private key in the body is never passed through the
// shared logging helper — same cred-safety posture as `useDeployDegenArena`.
// Only the non-secret `network` and the redacted `ok` summary are traced.
export function setDegenArenaCredentials(
  body: SetDegenArenaRequest,
): Promise<DegenArenaStored> {
  const trace = createTrace("settings", { broker: "degen_arena", network: body.network });
  const started = performance.now();
  trace.info("settings.broker.save");
  return fetch("/api/live/deploy/degen-arena", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      apiKey: body.apiKey,
      accountAddress: body.accountAddress,
      network: body.network,
    }),
  })
    .then(async (res) => {
      if (!res.ok) {
        let message = `HTTP ${res.status}`;
        try {
          const errBody = (await res.json()) as { message?: string };
          if (typeof errBody.message === "string" && errBody.message.length > 0) {
            message = errBody.message;
          }
        } catch {
          // Non-JSON error body — keep the HTTP-status fallback message.
        }
        throw new ApiError(res.status, String(res.status), message);
      }
      const stored = (await res.json()) as DegenArenaStored;
      trace.info("settings.broker.save.ok", {
        stored: stored.ok,
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

export function clearDegenArenaCredentials(): Promise<void> {
  return fetch("/api/live/deploy/degen-arena", { method: "DELETE" }).then(
    (res) => {
      if (!res.ok) {
        throw new ApiError(res.status, String(res.status), `HTTP ${res.status}`);
      }
    },
  );
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

/// Read the cached provider catalog (the persisted result of the
/// provider's `/v1/models` endpoint). Returns 404 when the provider
/// either isn't configured or hasn't been refreshed yet — callers
/// distinguish via the error body; in practice the UI maps both to
/// "click Refresh to fetch".
///
/// Distinct from `listProviderModels` above, which goes through the
/// older `fetch_models` path and returns only `id`/`display_name`/
/// `context_length`. The catalog carries the full `ModelEntry` shape
/// (max_output_tokens, pricing, reasoning class, raw provider row).
export function getProviderCatalog(name: string): Promise<Catalog> {
  const trace = createTrace("settings", { provider: name });
  const started = performance.now();
  trace.debug("settings.provider.catalog.load");
  return apiFetch<Catalog>(
    `/api/settings/providers/${encodeURIComponent(name)}/catalog`,
  )
    .then((cat) => {
      trace.debug("settings.provider.catalog.load.ok", {
        model_count: cat.models.length,
        duration_ms: durationSince(started),
        fetched_at: cat.fetched_at,
      });
      return cat;
    })
    .catch((err) => {
      // 404 is the "not yet fetched" common case — log at debug, not
      // error, so the dashboard's normal cold-start doesn't fill the
      // log with red.
      const summary = errorSummary(err);
      const isMissing =
        typeof summary === "object" && summary !== null && "status" in summary
          ? (summary as { status?: number }).status === 404
          : false;
      const level = isMissing ? "debug" : "error";
      trace[level]("settings.provider.catalog.load.error", {
        duration_ms: durationSince(started),
        error: summary,
      });
      throw err;
    });
}

/// Force-refresh one provider's catalog (fetch + cache write + return
/// the fresh value). The UI calls this from the "Refresh models" button
/// in Settings → Providers and invalidates the relevant query keys.
export function refreshProviderCatalog(name: string): Promise<Catalog> {
  const trace = createTrace("settings", { provider: name });
  const started = performance.now();
  trace.info("settings.provider.catalog.refresh.start");
  return apiFetch<Catalog>(
    `/api/settings/providers/${encodeURIComponent(name)}/catalog/refresh`,
    { method: "POST" },
  ).then((cat) => {
    trace.info("settings.provider.catalog.refresh.ok", {
      model_count: cat.models.length,
      duration_ms: durationSince(started),
      source_url: safeUrlHost(cat.source_url) ?? cat.source_url,
    });
    return cat;
  });
}

/// Refresh every non-local-candle provider's catalog. Returns one row
/// per attempted provider so the UI can render a per-row indicator —
/// partial failures (one provider's auth misconfigured) don't fail the
/// whole batch.
export function refreshAllProviderCatalogs(): Promise<RefreshOutcome[]> {
  return apiFetch<RefreshOutcome[]>(
    "/api/settings/providers/catalog/refresh-all",
    { method: "POST" },
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
//
// Each danger function accepts the operator-typed phrase as a
// parameter rather than auto-filling a bundled constant. The backend
// rejects anything that doesn't match its per-route expectation;
// the constants above are exported for UI display purposes only.

export function dangerResetWorkspace(
  typedPhrase: string,
): Promise<ResetWorkspaceReport> {
  return apiFetch<ResetWorkspaceReport>(
    "/api/settings/danger/reset-workspace",
    {
      method: "POST",
      body: JSON.stringify({ confirm: typedPhrase }),
    },
  );
}

export function dangerFactoryReset(
  typedPhrase: string,
): Promise<FactoryResetReport> {
  return apiFetch<FactoryResetReport>("/api/settings/danger/factory-reset", {
    method: "POST",
    body: JSON.stringify({ confirm: typedPhrase }),
  });
}
