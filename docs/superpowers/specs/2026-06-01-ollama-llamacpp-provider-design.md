# Ollama and llama.cpp First-Class Provider Support

**Date:** 2026-06-01  
**Status:** Implemented  
**Branch:** feature/no-filter-creation-warning (will be extracted to dedicated branch)

## Problem

Ollama and llama.cpp servers already worked via `ProviderKind::OpenaiCompat` with a custom base URL, but had no dedicated treatment: no named presets in the wizard, no typed dispatch path, no native catalog integration, and URL-pattern sniffing was the only way the system recognized them as local providers.

## Design Decisions

### 1. ProviderKind enum (`crates/xvision-core/src/config.rs`)

Two new variants added:

```rust
pub enum ProviderKind {
    Anthropic,
    OpenaiCompat,
    Ollama,    // serde: "ollama"
    LlamaCpp,  // serde: "llama-cpp"
    LocalCandle,
}
```

**Why not URL-pattern routing:** Dedicated enum variants give typed dispatch at every match site (compiler enforces exhaustiveness), enable kind-specific `has_key` resolution, and let the CLI/UI surface them as named choices rather than inferred categories.

**Backward compat:** Existing `openai-compat` rows with `localhost:11434` or `localhost:8080` continue to work — the URL-pattern branch in `fetcher_for()` and the Cline map are untouched.

### 2. Authentication (`crates/xvision-engine/src/api/settings/providers.rs`)

`api_key_env` is present in `ProviderEntry` for all kinds but treated as **optional** for `Ollama` and `LlamaCpp`. `entry_has_key()` returns `true` when `api_key_env` is empty for these kinds, so no `KeyMissing` error fires for a no-auth local deployment. Remote deployments behind a bearer-token proxy set `api_key_env` normally.

### 3. Catalog fetching (`crates/xvision-engine/src/providers/fetcher.rs`)

**`OllamaFetcher`** — hits `{base_url}/api/tags`:
- Maps `model.name` → `ModelEntry.id`
- Constructs `display_name` from `details.family` + `details.parameter_size` when both present
- Stores the full Ollama model object in `ModelEntry.raw` (family, parameter_size, quantization_level, size preserved for UI)
- Auth: `bearer_auth` header only when `api_key` non-empty

**`LlamaCppFetcher`** — hits `{base_url}/v1/models`:
- Same OpenAI-compat shape as `OpenAiCompatFetcher` but typed separately so `fetcher_for()` routes on kind
- Typically returns one model; id verbatim, full object in `raw`
- Auth: same optional-bearer pattern

### 4. LLM wire protocol

No new backend structs. Both kinds route to `OpenAiCompatIntern` / `OpenAiCompatBackend` / `OpenaiCompatDispatch` — Ollama and llama.cpp both speak the OpenAI chat-completions API.

### 5. Cline provider map (`crates/xvision-agent-client/src/provider_map.rs`)

- `ProviderKind::Ollama` → `provider_id = "ollama"`, `base_url = Some(base_url)`
- `ProviderKind::LlamaCpp` → `provider_id = "litellm"`, `base_url = Some(base_url)` (no dedicated Cline gateway for llama-server)

### 6. CLI (`crates/xvision-cli/src/commands/provider.rs`)

`--kind` accepts `ollama` and `llama-cpp`. Connection probe (`xvn provider check --probe`):
- Ollama: `GET {base_url}/api/tags`
- All others (including llama-cpp): `GET {base_url}/v1/models`

### 7. Frontend (`frontend/web/src/routes/settings/providers.tsx`)

Two new entries in `KIND_OPTIONS`:
- **Ollama (local)** — `wireKind: "ollama"`, default URL `http://localhost:11434`, API key optional
- **llama.cpp server** — `wireKind: "llama-cpp"`, default URL `http://localhost:8080`, API key optional

`keyRequired()` returns `false` for both kinds regardless of URL (no localhost check needed — they're always optional-auth by design). Filter panel gains `ollama` and `llama-cpp` options.

## Files Changed

| File | Change |
|---|---|
| `crates/xvision-core/src/config.rs` | `ProviderKind` + round-trip tests |
| `crates/xvision-engine/src/providers/fetcher.rs` | `OllamaFetcher`, `LlamaCppFetcher`, routing |
| `crates/xvision-engine/src/api/settings/providers.rs` | `has_key`, `add`, `update`, `fetch_models`, `resolve_provider` |
| `crates/xvision-engine/src/api/eval.rs` | `dispatch_from_provider` match arm |
| `crates/xvision-engine/src/eval/postprocess.rs` | `findings_model_for_provider` |
| `crates/xvision-engine/src/eval/preflight.rs` | `kind_to_str` |
| `crates/xvision-eval/src/provider_registry.rs` | `intern_backend`, `trader_backend` |
| `crates/xvision-agent-client/src/provider_map.rs` | `map_provider` |
| `crates/xvision-dashboard/src/llm_dispatch.rs` | dispatch match arm |
| `crates/xvision-dashboard/src/routes/eval/review.rs` | dispatch + no-auth guard |
| `crates/xvision-cli/src/commands/model.rs` | dispatch match arm |
| `crates/xvision-cli/src/commands/eval/review.rs` | dispatch + no-auth guard |
| `crates/xvision-cli/src/commands/provider.rs` | CLI flags + probe URL |
| `frontend/web/src/routes/settings/providers.tsx` | KIND_OPTIONS presets, keyRequired, filter |

## Deferred Items

- **Lowercase `authorization` header** in `providers.rs:668` — `req.header("authorization", …)` vs idiomatic `req.bearer_auth(…)`. Functionally equivalent over HTTP/1.1; consistency fix deferred.
- **Ollama fetch duplication** — `fetch_ollama_provider_models` in `providers.rs` and `OllamaFetcher` in `fetcher.rs` both parse `/api/tags`. Shared helper deferred to avoid over-engineering in initial landing.
- **Model picker Ollama metadata** — `ModelEntry.raw` contains family/parameter_size but the model picker UI does not yet surface it as a secondary label. Frontend follow-up.
