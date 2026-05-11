// Hand-written TS mirror of
// `xvision_engine::api::settings::providers::{ProvidersReport, ProviderRow,
// AddProviderRequest}`.
//
// We don't auto-generate via `cargo xtask gen-types` because the engine's
// `eval::compare` types still reference `Finding` / `MetricsSummary` /
// `RunMode` / `RunStatus` without `derive(TS)` — fixing those derives is
// its own cleanup. Same pattern PR #52 used for `types.compare.ts`.

export type ProviderKindStr = "anthropic" | "openai-compat" | "local-candle";

export type ProviderRow = {
  name: string;
  kind: ProviderKindStr;
  base_url: string;
  api_key_env: string;
  api_key_set: boolean;
  synthetic: boolean;
  referenced_by_intern: boolean;
};

export type ProvidersReport = {
  providers: ProviderRow[];
};

export type AddProviderRequest = {
  name: string;
  kind: ProviderKindStr;
  base_url: string;
  api_key_env: string;
};
