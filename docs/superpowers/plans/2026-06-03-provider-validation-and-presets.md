# Provider validation + presets + prod config repair

Date: 2026-06-03
Branch: `fix/provider-validation-presets` (worktree `.worktrees/provider-fixes`, off `origin/main`)

## Problem

Operator reported three provider bugs; investigation collapsed them into one
root cause plus two feature gaps.

1. **Custom-provider create with an invalid name corrupts the config.**
   Entering `Gemini` (capital G) produced:
   `validation failed for /data/config/default.toml: providers[2].name: provider
   name must match [a-z0-9-]+ providers[3].name: ...`
   Root cause: `add_inner`
   (`crates/xvision-engine/src/api/settings/providers.rs`) validates *empty* and
   *leading-underscore* names but **not** the `[a-z0-9-]+` charset that the
   config loader's garde validator (`xvision-core/src/config.rs:98
   validate_provider_name`) enforces. So the bad row is **written to
   `default.toml` first**, then the post-write `load_runtime` re-validation
   fails — leaving the file permanently invalid. Repeating the attempt persisted
   two bad rows (`providers[2]`, `providers[3]`).

2. **"Blank providers list" + "openrouter already exists" in prod = same cause.**
   Once `default.toml` holds invalid rows, `load_runtime` rejects the **whole
   file**, so `list` (via `load_cfg`) returns empty/error → the page looks blank.
   But `add` checks name collisions against the **raw TOML** (toml_edit,
   bypassing validation), so it still sees `openrouter` and returns `409`. No DB
   corruption — providers live in `config/default.toml`.

3. **Missing presets.** Nous Research and Gemini have no preset; Gemini's
   OpenAI-compatible endpoint also needs a models-URL fix.

## Goals / non-goals

In scope: prevent invalid-name writes (root cause), add Gemini + Nous presets,
make Gemini work end-to-end (OpenAI-compat), seed Gemini in the default config
for new installs, repair the live prod config.

Out of scope: a native (non-OpenAI) Gemini provider kind; reworking the
config-loader to skip individual bad rows (we fix the writer, not the loader —
"fix root cause, don't suppress").

## Work units

### WU1 — Reject invalid provider names before write (root cause)
Files: `crates/xvision-core/src/config.rs`, `crates/xvision-engine/src/api/settings/providers.rs`
- Expose the existing name rule from core as a reusable pub fn (e.g.
  `pub fn validate_provider_name_str(name: &str) -> Result<(), String>`) and have
  the garde validator (`config.rs:98`) delegate to it (single source of truth) —
  the garde adapter just maps `Err(String)` → `garde::Error::new`.
- In `add_inner`, call it on the **trimmed** name **before** the toml_edit write
  (alongside the existing empty / `_`-prefix checks at providers.rs:896-903),
  returning `ApiError::Validation`. This maps to HTTP 400 (confirmed:
  `dashboard/src/error.rs:59` → `StatusCode::BAD_REQUEST`).
DoD:
- `add` with name `"Gemini"` returns `Err(ApiError::Validation(_))`.
- After that rejected call, `default.toml` is byte-unchanged and `load_cfg`
  still succeeds (no corruption).
- Edge-case tests (per completeness review G5): name with leading/trailing
  whitespace that trims to valid is accepted; name `length > 32` is rejected by
  `add` (today only the garde loader enforces 32 — the shared fn must too); a
  name with an embedded space/uppercase is rejected.
- A core unit test asserts the garde validator and the new `pub fn` agree
  (delegation didn't change behavior).
- Existing provider tests (providers.rs:1719-1846) still pass.

### WU2 — Gemini OpenAI-compat models URL
File: `crates/xvision-engine/src/providers/fetcher.rs` (`openai_compat_models_url`)
- A base ending in `/openai` (Gemini: `.../v1beta/openai`) must yield
  `{base}/models`, not `{base}/v1/models`. **Trim trailing slashes first, then
  test the suffix** so both `.../v1beta/openai` and `.../v1beta/openai/`
  normalize identically (canonical stored form is the no-slash variant).
DoD:
- `openai_compat_models_url("https://generativelanguage.googleapis.com/v1beta/openai")`
  == `https://generativelanguage.googleapis.com/v1beta/openai/models`.
- Trailing-slash input normalizes to the same result.
- Existing cases unchanged: `.../v1` → `{base}/models`; bare host → `{base}/v1/models`.
- Unit test (lives in `xvision-engine`; fn is `pub(crate)`) covers all cases.
Note: chat path already uses `{base}/chat/completions` (`agent/llm.rs:1298`,
trims trailing slash), correct for the Gemini base — no change needed there.

### WU3 — Frontend presets + client-side name validation
File: `frontend/web/src/routes/settings/providers.tsx`
- Add `KIND_OPTIONS` entries (canonical no-trailing-slash base URLs):
  - Gemini → wireKind `openai-compat`, name `gemini`, base
    `https://generativelanguage.googleapis.com/v1beta/openai`, keyHelp "Google AI
    Studio key (AIza…)".
  - Nous Research → wireKind `openai-compat`, name `nous-research`, base
    `https://inference-api.nousresearch.com/v1`, keyHelp "Nous Portal API key".
- For the custom path's Name field, validate `^[a-z0-9-]+$` (1..=32, no leading
  `_`) inline and block submit with a clear message, mirroring the server rule.
DoD:
- Both presets render and prefill name+base.
- A custom name with uppercase/spaces shows an inline error and disables submit.
- `providers.test.tsx` updated/green.
- No wire-type change (presets are FE-only `KIND_OPTIONS` constants;
  `AddProviderRequest` unchanged) → no ts-rs regeneration (confirmed by review).

### WU4 — Seed Gemini AND Nous as default providers (operator request)
Files: `config/default.toml`, `crates/xvision-engine/src/api/settings/providers.rs`,
`crates/xvision-core/src/config.rs` (test)
- Add two `[[providers]]` blocks (openai-compat, canonical no-slash bases from WU3):
  - `gemini`, `api_key_env = "GEMINI_API_KEY"`
  - `nous-research`, `api_key_env = "NOUS_API_KEY"`
  Rewrite the "None are seeded by default" comment (lines 12-15) to describe the
  two seeded, key-less starter providers and that users paste keys to enable them.
- Teach `default_api_key_env_for` (providers.rs:1475) the conventions so the
  add-flow env matches the seed: `gemini → GEMINI_API_KEY`,
  `nous-research → NOUS_API_KEY`. Mirror these env names in the WU3 preset keyHelp.
- Teach `sensible_default_model` (providers.rs:1489) arms so "Set as default"
  yields a working model out of the box (review G3):
  `gemini → "gemini-2.5-flash"`, `nous-research → "Hermes-4-405B"` (or the current
  Nous default id; pick a real catalog id and note it's overridable).
- **Update the test that asserts an empty seed** (review G1):
  `repo_default_toml_ships_with_no_user_providers` (config.rs:1153) currently
  asserts zero non-`_` providers — it must be rewritten to assert exactly
  `["gemini", "nous-research"]` (order-insensitive) and its doc-comment updated.
  Re-verify the sibling repo-file readers still pass: `loads_repo_default_toml`
  (config.rs:781, only checks temperature/step/rate-limit — survives) and
  `does_not_auto_derive_default_llm_provider` (config.rs:1176 — survives).
DoD:
- Fresh `default.toml` loads via `load_runtime` with both rows present and valid.
- `repo_default_toml_ships_with_no_user_providers` updated and green.
- Fresh install with no `GEMINI_API_KEY` / `NOUS_API_KEY` shows the two
  providers as **configurable-but-keyless** (the `○ missing` pill, providers.tsx:363),
  NOT an error state — keyless seeded rows render gracefully in the list (the
  add-flow `needs_api_key` guard does not apply to pre-existing rows).
- The named *fixture-based* tests (`tests/api_provider_parity.rs:56`,
  `cli/tests/doctor_providers.rs:50`) write their own `MIN_CONFIG` to a TempDir
  and do NOT read the repo file — they are unaffected (review G2 correction; the
  earlier "adjust counts" note was wrong).

### WU5 — Repair live prod config (ops; gated on SSH authorization)
- Read live `default.toml` on xvn dev host (`root@100.120.48.1`), remove the
  invalid `name = "Gemini"` rows, recreate the xvn container, verify the
  providers list loads.
- BLOCKED: SSH to the prod host needs explicit operator authorization (the
  classifier denied the read). Alternative: operator runs the documented
  commands via `!`.
DoD: providers list renders again; `openrouter` visible; a clean re-add of a
valid provider succeeds.

### WU6 — One bad provider row can't take down the whole list (operator request)
Files: `crates/xvision-core/src/config.rs`,
`crates/xvision-engine/src/api/settings/providers.rs`,
`frontend/web/src/routes/settings/providers.tsx` (+ regenerated ts types)
- Root cause of the "blank list": `load_runtime`'s `#[garde(dive)]` on
  `providers` fails the WHOLE config load if any single provider row is invalid,
  and every provider API path calls it.
- Add `xvision_core::config::load_runtime_lenient` → `(RuntimeConfig,
  Vec<InvalidProvider>)`: parses, drops individually-invalid/duplicate provider
  rows (reporting them), then validates the cleaned config strictly. Non-provider
  errors still fail loudly (leniency scoped to `[[providers]]`).
- Engine: the providers module's `load_cfg` now delegates to the lenient loader,
  so list/show/add/update/remove/resolve all stop hard-failing on one bad row.
  `list` surfaces dropped rows via a new `ProvidersReport.invalid:
  Vec<InvalidProviderRow>` (ts `invalid?`, back-compat). `remove` accepts a name
  present only in the invalid set, so a corrupted row is removable via the API
  (self-heal without SSH).
- Frontend: an inline warn-toned strip lists invalid rows (name + reason) with a
  per-row Remove (no popups; dark-mode-safe `warn` tokens).
DoD:
- Core: strict load rejects a file with an invalid row; lenient load keeps the
  valid rows + reports the bad one; non-provider errors still fail. (tested)
- Engine: `list` returns valid rows + `invalid` instead of erroring; `remove`
  deletes an invalid row. (tested)
- Frontend: strip renders + Remove calls `removeProvider(name)`. (tested)
- ts-rs types regenerated (`InvalidProviderRow.ts`, `ProvidersReport.invalid?`).

## Note: pre-existing `origin/main` breakage (inherited, mostly out of scope)
The worktree is off `origin/main`, which currently does NOT cleanly build/test
("main is not build-gated"):
- `crates/xvision-core/src/store.rs:275` — a `TraderDecision` test fixture
  missing 12 SL/TP fields → core lib-test didn't compile. **Fixed here**
  (test-only, needed to verify the core change). Does not affect the deploy build.
- 9 `xvision-engine` lib tests fail independently of this change
  (`_sqlx_migrations.version` UNIQUE in `eval::review::engine` + `api::strategy`;
  `activation_mode is filter_gated but filter is None` in `strategies::store`).
  In files this PR does not touch; reproduce single-threaded → not a flake, but
  pre-existing. Left for a separate fix; flagged so they aren't mistaken for
  regressions from this PR.

## Verification
- `scripts/cargo test -p xvision-engine -p xvision-core` (disk-guarded wrapper,
  in the worktree with a per-track `CARGO_TARGET_DIR`).
- `pnpm --dir frontend/web test` for providers.test.tsx.
- Manual: add a custom provider named `Gemini` → expect inline block (FE) and
  400 with no file change (BE); select the Gemini preset + paste a key → Test
  connection succeeds.

## Revisions after plan-review gate (round 1)
- Feasibility: PASS. Scope: PASS (2 corrections folded in: canonical no-slash
  base URLs + trim-then-test in WU2; comment rewrite + keyless-UX DoD in WU4).
- Completeness: FAIL → addressed. G1 (the `repo_default_toml_ships_with_no_user_providers`
  test breaks) now an explicit WU4 step; G2 (wrong tests named) corrected; G3
  (`sensible_default_model` arms) added; G4 (`nous-research` env convention)
  added as `NOUS_API_KEY`; G5 (thin WU1 edge-case tests) expanded.
- Operator follow-up: **Nous is also seeded** as a default provider (not just
  Gemini) — WU4 seeds both `gemini` and `nous-research`.

## Risk / rollback
- Seeding a provider changes the long-standing "no seeded providers" default —
  low risk, but watch integration tests asserting provider counts.
- All code work is isolated in the worktree; prod repair is reversible (we keep a
  copy of the pre-edit `default.toml`).
