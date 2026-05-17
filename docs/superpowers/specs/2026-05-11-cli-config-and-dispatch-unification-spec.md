# CLI Config + LLM Dispatch Unification — Spec

**Date:** 2026-05-11
**Surfaces:** `xvision-core` (config resolver), `xvision-cli` (every subcommand), `xvision-engine` (eval dispatch), `xvision-dashboard` (settings + wizard already-correct)
**Status:** Spec — surfaced from a live CLI-surface QA pass against `xvn-app` on extndly-dev. Companion to the in-flight `[intern]` → "default agent" rework. Ready to break into per-surface plans.
**Related:** Companion to `2026-05-11-qa-pass-2-spec.md` Batch 0. This spec is the *backend* of the same drift the dashboard QA pass surfaced in the UI.

---

## Goal

Collapse three diverging "where does the runtime config / LLM dispatch / provider secrets live" implementations into one shared resolver + bootstrap layer. The same drift that produced the strategies-vs-bundles bug (PR #82) is replicated across config-path lookup, LLM dispatch, and env-var preamble — every new CLI command currently picks one of several inconsistent answers, and they don't agree with what the dashboard does at boot.

This spec captures the drift sites, the contract we want, and the migration order. It is **not** a plan — file-level decomposition belongs in per-surface plans authored after review.

---

## What's broken today (observed live)

A 30-minute probe of the `xvn` CLI surface inside the running `xvn-app` container exposed five independent inconsistencies, all variants of the same root cause: nothing in the codebase is responsible for "where is the canonical runtime config" or "how do we reach an LLM," so each caller invents its own answer.

### 1. Four different config-path lookup conventions

| Caller | Lookup | Site |
|---|---|---|
| `xvn provider {list,show,check,add,remove}` | `cwd/config/default.toml` (hardcoded) | `crates/xvision-cli/src/commands/provider.rs:73` |
| `xvn ab-compare` | `<workspace_root>/config/default.toml` (hardcoded) | `crates/xvision-cli/src/commands/ab_compare.rs:65` |
| `xvn risk evaluate` | `cwd/config/risk.toml` + `cwd/config/whitelist.toml` (clap defaults) | `crates/xvision-cli/src/commands/risk.rs` (clap `[default: config/risk.toml]`) |
| Dashboard (`/api/settings/providers`, `llm_dispatch`) | `XVN_CONFIG_PATH` → `$HOME/config/default.toml` (correct) | `crates/xvision-dashboard/src/routes/settings/providers.rs:22`, `crates/xvision-dashboard/src/llm_dispatch.rs:123` |

Live symptom: `xvn provider list` from default cwd reads `/home/xvision/config/default.toml` which doesn't exist. From `/`, it reads `/config/default.toml` which is the read-only seed image (no `[[providers]]` rows) — so it returns zero providers while the dashboard's `/api/settings/providers` returns the two registered ones, because the dashboard reads via `XVN_CONFIG_PATH` which the operator set to `/config/default.toml` while the actual provider rows live in `/data/config/default.toml`.

### 2. Two config files in the running container

- `/config/default.toml` — read-only seed mounted from the image. No `[[providers]]` block.
- `/data/config/default.toml` — persistent volume copy. Has the deepseek + openrouter rows the operator added via the dashboard.

`XVN_CONFIG_PATH=/config/default.toml` is set in the container env but the dashboard's "Add provider" UI writes to `/data/config/default.toml`. The writer and reader paths disagree — confirmed via `xvn provider list` reporting empty.

### 3. `model_requirement` on strategy slots is unparsed by eval dispatch

`crates/xvision-engine/src/api/eval.rs` (pre-patch) hardcoded:

```rust
let api_key = std::env::var("ANTHROPIC_API_KEY")?;
let dispatch_arc: Arc<dyn LlmDispatch> = Arc::new(AnthropicDispatch::new(api_key));
```

The strategy bundle's `trader_slot.model_requirement` (e.g. `"openrouter.deepseek/deepseek-v4-flash"`) was *never* consulted. Every eval went to `api.anthropic.com` regardless. `AnthropicDispatch::new` (`crates/xvision-engine/src/agent/llm.rs:222`) hardcodes the URL with no override.

A draft patch on `explore/eval-dispatch-honors-model-requirement` (commit `0afb301`) resolves `<provider>.<model>` from the bundle, looks it up in the runtime config, and builds either `AnthropicDispatch` or `OpenaiCompatDispatch` accordingly. It's unbuilt and informational — feed it into the refactor, don't merge as-is.

### 4. Provider secrets only loaded for the daemon

`xvision_engine::api::settings::providers::load_providers_secrets_into_env` materializes `$XVN_HOME/secrets/providers.toml` into `std::env` so backend constructors that read `std::env::var(api_key_env)` find the keys. It's called **once** by `dashboard serve` at boot.

CLI single-shot verbs that hit LLMs (`xvn intern`, `xvn trader`, `xvn eval run`, `xvn provider check`) skip this preamble entirely. Result: every one of them 401s against a provider whose key is on disk but not exported to the shell.

### 5. `--prices` is a path, not JSON

Every `xvn indicator <name>` verb's `--help` documents `--prices` as a JSON array literal. Implementation in `crates/xvision-cli/src/commands/indicator.rs:131` is `serde_json::from_slice(&std::fs::read(path)?)` — it's a path. Inline JSON fails with the generic `No such file or directory (os error 2)` (no path in the message). Same misleading-help pattern in `risk evaluate`, `trader preview`, `ab-compare`.

### 6. QA bugs — independent of the refactor, tracked here for visibility

These three are real bugs surfaced during the same probe but are **not** fixed by Batches A–D. They live in different code paths and need their own attention. Logged here so they don't get lost; either roll into the refactor's PRs opportunistically or file as standalone follow-ups.

#### QA-1 — `xvn eval show --id <X>` rejects `--id`

- **Repro:** `xvn eval show --id 01ABC...` → `error: unexpected argument '--id' found`
- **Cause:** clap defines `<RUN_ID>` as positional; every other `eval` subcommand uses flags (`eval run --strategy --scenario`). Inconsistent.
- **Fix surface:** clap derive in `crates/xvision-cli/src/commands/eval.rs`. Pick one — flags everywhere or positional everywhere — and apply across the verb. Likely one-liner per arg.
- **Owner:** CLI ergonomics. Could be bundled into Batch D's plan.

#### QA-2 — `close-position` and `portfolio` disagree on open positions

- **Repro:** with `APCA_*` env set, `xvn fire-trade --side buy --size-bps 10` fills (ExecutionReceipt confirms). Immediately after, `xvn portfolio --venue alpaca` shows the BTC long; `xvn close-position --venue alpaca --asset BTC` returns `{"note": "no open position"}`.
- **Cause:** the two verbs read position state from different sources. One hits Alpaca's `/v2/positions`; the other reads from an internal cache / store that wasn't updated by the fire-trade write path. Needs tracing; this spec doesn't fix it.
- **Fix surface:** `crates/xvision-execution/src/alpaca.rs` + the venue-command wiring in `crates/xvision-cli/src/commands/venue.rs` and `commands/fire_trade.rs`. Either close-position should hit the live venue read (same as portfolio), or fire-trade should write through to whatever store close-position reads.
- **Severity:** medium — quietly lets a position pile up because the close operator thinks the position doesn't exist.

#### QA-3 — Cryptic file errors across the CLI

- **Repro:** `xvn risk show-config` (no args) → `No such file or directory (os error 2)` — no path.
- **Repro:** `xvn indicator macd --prices "[100,101,...]"` → same; the JSON-vs-path issue surfaces here too (Batch D), but the error message itself is the bug.
- **Repro:** `xvn trader preview --briefing /dev/null` → `EOF while parsing a value at line 1 column 0` — no file path, no hint that the file was empty.
- **Cause:** raw `?` propagation from `std::fs::read` / `serde_json::from_slice` with no `.with_context()` wrappers. Pattern repeats across most JSON-file-reading CLI verbs.
- **Fix surface:** drop in `anyhow::Context` / a small `read_json_file(path) -> Result<T>` helper across `commands/indicator.rs`, `commands/risk.rs`, `commands/trader.rs`, `commands/intern.rs`, `commands/ab_compare.rs`. Trivial per call site; high QoL win.
- **Severity:** low individually, but it adds up — every operator hits this within 5 minutes of using the CLI.

---

## Batch D — CLI hygiene

Three items below — small, mostly mechanical, but they're the difference between a CLI an operator can use and one they have to grep the source for. The refactor in Batches A–C touches enough of these files that landing D in the same wave is cheap.

### D.1 — Pick one I/O convention for JSON inputs

The current state: `--prices` / `--briefing` / `--snapshot` / `--portfolio` / `--cycles` / `--bars` / `--hlc` / `--hl` are all documented as "JSON" in `--help` and implemented as "path to JSON file." Pick one of:

- **(a) Path only** — rewrite all help text to say "path to JSON file." Cheapest; matches today's impl.
- **(b) Inline only** — change every reader to parse the literal. Verbose at the shell.
- **(c) Both, via `@` prefix** — `--prices [1,2,3]` parses as JSON; `--prices @file.json` reads from disk. The standard CLI convention (`curl`, `gh`).

Recommend **(c)** for `--prices` / single-value args and **(a)** for multi-megabyte args like `--cycles` / `--bars`. Plan-writer picks final, applies consistently across `indicator`, `risk evaluate`, `trader preview`, `intern preview`, `ab-compare`, `run-setup`.

### D.2 — `xvn fire-trade --dry-run`

`fire-trade` today submits a live paper order with no confirmation step. The operator pattern is: synthesize the `RiskDecision::Approved`, *print it*, and exit zero — without calling the venue executor. Mirror in `close-position` for symmetry. Default behavior unchanged.

### D.3 — Flag-vs-positional consistency

`eval show <RUN_ID>` vs `eval run --strategy --scenario` is the visible one (QA-1). Audit all `eval`, `strategy`, `skill`, `provider`, `store` subcommands and pick one convention per verb-family. Flag-style is more discoverable; positional is shorter for the common case. Either is fine — consistency is the point.

### Scope boundary

QA-2 (`close-position` vs `portfolio` position-source mismatch) is **explicitly out of Batch D** — it's a venue-layer bug, not CLI hygiene. File separately.

---

## Recommended sequencing

Four batches, each landable independently. A–C are the architectural drift fix; D is CLI hygiene riding the same wave.

### Batch A — Single config-path resolver

**New module:** `xvision_core::config::resolver`.

**Contract:**

```rust
/// Resolve the canonical runtime config path. Priority:
///   1. $XVN_CONFIG_PATH (explicit override)
///   2. $XVN_CONFIG_DIR/default.toml
///   3. $XVN_HOME/config/default.toml
///   4. <workspace_root>/config/default.toml (dev fallback only when
///      CARGO_MANIFEST_DIR is set — production binaries skip this)
pub fn resolve_config_path() -> Result<PathBuf, ConfigError>;

/// Load + validate. Single entry point used by every caller.
pub fn load_runtime_config() -> Result<RuntimeConfig, ConfigError>;
```

**Migration:**

- Delete the `cwd/config/default.toml` lookups in `xvision-cli/src/commands/provider.rs:73`, `risk.rs` (clap defaults), `ab_compare.rs:65`.
- Delete the duplicate `config_path()` helpers in `xvision-dashboard/src/routes/settings/providers.rs:22` and `xvision-dashboard/src/llm_dispatch.rs:123`. Route through the resolver.
- The `xvn provider --config <path>` flag (if it exists) collapses to the env-var override, which the resolver already honors.

**Container fix:** the two-file split is symptomatic — the persistent volume copy is what readers should see. Either:

- (a) make the writer always update both paths atomically, or
- (b) make `XVN_CONFIG_PATH` point at the persistent path in the compose file and stop mounting the seed image read-only.

(b) is simpler. Plan-writer should pick.

### Batch B — CLI bootstrap preamble

**New module:** `xvision_cli::bootstrap`.

**Contract:**

```rust
/// Single-shot CLI process bootstrap. Idempotent — safe to call from
/// every subcommand's `run` fn. Steps:
///   1. resolve xvn_home (env > $HOME/.xvn)
///   2. load_providers_secrets_into_env(&xvn_home) — same fn the
///      dashboard calls at boot
///   3. load risk.toml + whitelist.toml from $XVN_CONFIG_DIR (if present)
///      into a process-global lazy-static available to every subcommand
pub async fn prepare(xvn_home: &Path) -> Result<(), BootstrapError>;
```

**Migration:**

- Insert `bootstrap::prepare(&xvn_home).await?;` at the top of `xvision-cli/src/main.rs` `match`-on-subcommand block, before any subcommand dispatches.
- Remove ad-hoc `load_providers_secrets_into_env` calls (currently zero in CLI; will be needed by the resolved eval/intern/trader paths from Batch C).
- `xvn intern`, `xvn trader`, `xvn eval run`, `xvn provider check` stop requiring the operator to manually export `XVN_PROVIDER_*_KEY` / `ANTHROPIC_API_KEY` for keys already saved on disk.

### Batch C — Single LLM dispatch resolver

**New module:** `xvision_engine::agent::dispatch`.

**Contract:**

```rust
/// Resolve a `model_requirement` string to a concrete `LlmDispatch`,
/// honoring registered providers in the runtime config.
///
/// Format: `<provider_name>.<model_id>` (canonical) or `<provider>:<model>`
/// (planned migration — see "Open questions" below). Provider lookup
/// walks `RuntimeConfig.providers`. Dispatch is selected by `ProviderKind`:
///   - Anthropic → AnthropicDispatch with key from `api_key_env`
///   - OpenaiCompat → OpenaiCompatDispatch(base_url, key)
///   - LocalCandle → not supported in eval (yet); returns validation error
pub fn build_dispatch(
    model_requirement: &str,
    cfg: &RuntimeConfig,
) -> Result<Arc<dyn LlmDispatch>, DispatchError>;
```

**Migration:**

- Replace the hardcoded `AnthropicDispatch` block in `xvision-engine/src/api/eval.rs:428-431` with a `build_dispatch` call. (Draft on `explore/eval-dispatch-honors-model-requirement`, commit `0afb301`.)
- Replace the slot-prompt → backend call in `crates/xvision-engine/src/agent/execute.rs:54` so the slot's `model_requirement` actually controls which dispatch is used, not just what string gets put in the `model` field of an Anthropic request.
- Replace the wizard's intern dispatch construction in `xvision-dashboard/src/llm_dispatch.rs` if/when it diverges. Today it's already correct because it reads `XVN_CONFIG_PATH` — the wins are consistency and removing one more bespoke resolver.

**Legacy fallback:** when `model_requirement` starts with `anthropic.` but no `anthropic` provider is registered, fall back to `std::env::var("ANTHROPIC_API_KEY")`. Keeps existing strategy templates working through the migration.

---

## Companion change in the dashboard

The dashboard's "Add provider" UI writes to the same `RuntimeConfig` path the resolver picks. Once Batch A lands, the dashboard's two private `config_path()` helpers become one call into `xvision_core::config::resolver`. No UX change.

The `[intern]` → "default agent" rework (`2026-05-11-qa-pass-2-spec.md` Batch 0) is **orthogonal** to this spec: that rework renames the slot and exposes `provider + model` in Settings; this spec changes how every reader/writer of `RuntimeConfig` (including that new slot) reaches the file. The renames Batch 0 introduces don't conflict with the resolver work — they happen in `xvision-core::config::RuntimeConfig` struct fields, which the resolver returns opaquely.

---

## Open questions for plan-write

1. **Separator for `model_requirement`.** Today: `<provider>.<model>` via first-dot split. Works only because no provider name contains a dot. A `:` separator (`openrouter:deepseek/deepseek-v4-flash`) is more conventional and unambiguous. Plan-writer decides: one-time rename of every template string + a compat shim for in-flight bundles, or punt and document the brittle invariant.
2. **Test surface.** Each batch's plan should state which existing tests can stay (they cover the *behavior* the resolver/bootstrap/dispatch now provide) and which need new fixtures (config-path resolution under different env permutations).

Batch D's three items (I/O convention, dry-run flag, flag-vs-positional) have recommendations in place and are not "open" — they're decisions for the D plan-writer to confirm and apply consistently.

---

## Non-goals

- Renaming `xvn` subcommands. The verbs (`provider`, `strategy`, `skill`, `eval`, `fire-trade`, …) stay as-is.
- Touching the `StrategyBundle` schema, the eval pipeline shape, the broker surface, or the wizard tool loop.
- Migrating any on-disk format. The resolver returns `RuntimeConfig` as it exists today.
- Implementing local-candle dispatch. That's its own track; this spec only routes around it.

---

## Acceptance criteria

When all four batches land, the following hold:

**Batch A — Config resolver**
- `xvn provider list` and the dashboard's `/api/settings/providers` return the same rows from any cwd, given any combination of `XVN_CONFIG_PATH` / `XVN_CONFIG_DIR` / `XVN_HOME` env vars.
- The drift between `crates/xvision-dashboard/src/llm_dispatch.rs:123`, `crates/xvision-dashboard/src/routes/settings/providers.rs:22`, `crates/xvision-cli/src/commands/provider.rs:73`, `crates/xvision-cli/src/commands/ab_compare.rs:65`, and `crates/xvision-cli/src/commands/risk.rs` clap defaults is collapsed into one `xvision_core::config::resolver` call.
- The container's two-file split is resolved (compose-file fix or dual-write).

**Batch B — Bootstrap preamble**
- `xvn intern brief …` succeeds when `providers.toml` has a saved key for the configured provider, without the operator exporting `XVN_PROVIDER_*_KEY`.
- Same for `xvn trader run`, `xvn eval run`, `xvn provider check`.

**Batch C — Dispatch resolver**
- A strategy bundle declaring `trader_slot.model_requirement = "openrouter.deepseek/deepseek-v4-flash"` causes `xvn eval run` to dispatch against openrouter, not Anthropic.
- A bundle declaring `anthropic.claude-…` still works with `ANTHROPIC_API_KEY` even with no registered `anthropic` provider row (legacy fallback).

**Batch D — CLI hygiene**
- Every JSON-input flag across `indicator`, `risk evaluate`, `trader preview`, `intern preview`, `ab-compare`, `run-setup` uses the chosen convention consistently, and `--help` matches impl.
- `xvn fire-trade --dry-run` and `xvn close-position --dry-run` print the synthesized decision and exit zero without calling the venue.
- Flag-vs-positional convention is consistent within each verb family (audit complete; chosen convention applied).

**Out of scope (filed separately)**
- QA-1: `eval show --id <X>` rejected — single clap arg fix, bundle into D if convenient or file as own follow-up.
- QA-2: `close-position` vs `portfolio` position-source mismatch — venue-layer bug, separate ticket.
- QA-3: cryptic file-error messages across CLI verbs — file as own ticket; touches enough files that it can ride D's wave opportunistically but isn't gated on it.
