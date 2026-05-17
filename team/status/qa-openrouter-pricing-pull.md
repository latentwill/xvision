---
track: qa-openrouter-pricing-pull
owner: claude (qa-operator-2026-05-17)
status: ready-for-review
last_update: 2026-05-17
---

## Current

Claimed 2026-05-17. Working in `.worktrees/qa-openrouter-pricing-pull` against
`task/qa-openrouter-pricing-pull`. Cargo target dir pinned to
`$HOME/.cargo-target/xvision` to avoid duplicating `target/` in the worktree.

## Findings (investigation phase)

Where the model-library cache lives:

- Catalog data shape: `crates/xvision-core/src/providers/catalog.rs`
  (`ModelEntry`, `Catalog`). `ModelEntry` already carries
  `pricing_per_million_input_usd: Option<f64>` and
  `pricing_per_million_output_usd: Option<f64>` (PR #185 surface).
- Cache + fetch: `crates/xvision-engine/src/providers/{cache,fetcher,service}.rs`.
  `CatalogService` is the process-wide owner; reads `$XVN_HOME/cache/models/<provider>.json`
  on first access and refreshes via the matching fetcher.

Where the OpenRouter `/models` parser already lives:

- `crates/xvision-engine/src/providers/fetcher.rs::parse_openrouter_models`
  (extended by #185) **already parses pricing**. It reads
  `pricing.prompt` / `pricing.completion` via `parse_per_token_usd` (which
  rejects `<= 0.0`, including the `"0"` strings free routes return), then
  multiplies by 1e6 to store as `$/Mtok`. Tests
  `parse_openrouter_models_extracts_full_metadata` and
  `openrouter_max_completion_falls_back_to_context_length` already assert
  the parsed values are `Some(15.0)` / `Some(75.0)` and `None` for free
  routes. So contract acceptance #1 ("pricing persisted alongside
  max-tokens") is already satisfied by the merged #185.

Where per-run token cost is calculated:

- Nowhere in the engine, today. The Run summary surfaces
  `actual_input_tokens` / `actual_output_tokens`
  (`crates/xvision-engine/src/api/eval.rs::RunSummary`) and the export
  surface produces `provider_diagnostics.tokens_used`
  (`crates/xvision-engine/src/eval/export.rs::build_provider_diagnostics`),
  but no module multiplies those by per-token pricing.
- Frontend agent-runs Trace dock and RunStatusStrip do reference
  `total_cost_usd` and `cost_usd`
  (`frontend/web/src/features/agent-runs/{TraceDock,RunStatusStrip}.tsx`,
  `frontend/web/src/api/types-agent-runs.ts`), but every numeric value
  there comes from mock fixtures (`mock-fixtures.ts`); no Rust call site
  populates them. The `ModelCallFinishedEvent.cost_usd` field in
  `xvision-observability::events` is defined but no emitter computes it.
- The "wrong number" the operator observed therefore can't be a hardcoded
  fallback — it's most likely the mock-fixture path or a downstream
  display path computing cost from a stale per-Mtok constant. The right
  fix per this contract's scope is to provide the canonical cost function
  the eval/observability surfaces will call, keyed on the pulled
  `ModelEntry` pricing, so when the call sites are wired up next they
  pull the truth instead of a constant.

Conclusion: the deliverable is an `eval::cost` module that takes a
`Catalog` (or per-model pricing) plus `input_tokens` / `output_tokens` and
returns `Option<f64>` USD, treating absent / zero pricing as `None`
(unknown) rather than `Some(0.0)`. Anthropic / OpenAI catalogs that don't
carry pricing return `None`, so existing operator-trusted Anthropic /
OpenAI cost paths (which compute cost out-of-band) are untouched.

## Plan

1. Add `crates/xvision-engine/src/eval/cost.rs` with
   `compute_token_cost_usd(input_tokens, output_tokens, &ModelEntry) -> Option<f64>`
   and `compute_token_cost_usd_from_catalog(input_tokens, output_tokens, model_id, &Catalog) -> Option<f64>`.
   Pricing semantics:
   - Returns `None` if either price is `None` or `<= 0`.
   - Computes `(in_tok * in_per_mtok + out_tok * out_per_mtok) / 1_000_000`.
   - Anthropic / OpenAI catalogs without pricing → `None`, leaving the
     existing out-of-band path in place.
2. Expose the module from `crates/xvision-engine/src/eval/mod.rs`.
3. Unit tests in the same file:
   - Fixture OpenRouter `ModelEntry` (Claude Opus 4.7 @ $15/$75 per Mtok)
     with known token counts → assert exact USD result.
   - Pricing absent → `None`.
   - Pricing `Some(0.0)` → `None` (the puller already filters these to
     `None`, but the cost fn defends against future shape drift).
   - Anthropic-style entry with no pricing → `None`.
4. Wider verification:
   - `cargo test -p xvision-engine`
   - `cargo clippy -p xvision-engine -- -D warnings`
5. Commit on `task/qa-openrouter-pricing-pull` with the standard
   Co-Authored-By trailer. No push, no PR.

## Path reconciliation note

The contract's `allowed_paths` references paths that don't exist on
`origin/main`: `crates/xvision-engine/src/llm/**`,
`crates/xvision-engine/src/eval/dispatcher.rs`, and
`crates/xvision-engine/src/eval/trader_output.rs` (the actual file lives
at `eval/executor/trader_output.rs` and is pure error classification, not
cost). Same pattern as PR #185 reconciled: I'm taking the allowed scope
as "the new `eval/cost.rs` plus narrow edits to whatever in `eval/` or
`providers/fetcher.rs` is load-bearing for OpenRouter pricing pull and
cost calc". The conflict-zone caveat for `eval/executor/trader_output.rs`
doesn't bite — this track doesn't need to edit that file.

## Verification

Cargo PATH on this workstation is `~/.cargo/bin/cargo`; `CARGO_TARGET_DIR`
pinned to `$HOME/.cargo-target/xvision` to avoid duplicating `target/` in
the worktree.

- `cargo test -p xvision-engine --lib eval::cost` → **8 / 8 pass**:
  - `cost_for_openrouter_claude_opus_matches_openrouter_pricing_page`
  - `cost_returns_none_when_pricing_absent`
  - `cost_returns_none_when_either_side_missing`
  - `cost_treats_zero_pricing_as_unknown`
  - `cost_treats_negative_or_nonfinite_pricing_as_unknown`
  - `cost_from_catalog_looks_up_by_exact_id`
  - `cost_scales_linearly_with_tokens`
  - `cost_handles_zero_token_counts`

- `cargo test -p xvision-engine` → 264 pass, 4 fail. **All 4 failures
  pre-exist on `origin/main` (5b40959)** — confirmed by running the same
  tests in a baseline worktree. Failures:
  - `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
  - `eval::postprocess::tests::extract_and_record_persists_findings_and_indexes_them`
  - `eval::postprocess::tests::extract_and_record_returns_zero_on_extractor_error`
  - `eval::postprocess::tests::extract_and_record_returns_zero_when_extractor_returns_empty_array`

  These are baseline drift, unrelated to OpenRouter pricing. None of them
  reference `cost`, `pricing`, or any file this contract touches.

- `cargo clippy -p xvision-engine --no-deps -- -D warnings` → 31 errors,
  **31 errors on origin/main baseline** (same count). `cost.rs` itself
  produces zero clippy diagnostics; the 31 errors come from
  `strategies/agent_ref.rs`, `strategies/validate.rs`, and similar files
  outside this contract's scope.

- `cargo clippy -p xvision-engine -- -D warnings` (the verbatim contract
  command, which pulls in deps) hits the pre-existing
  `xvision-core::config::validate_provider_name` ptr_arg lint — also
  present on `origin/main` baseline.

## Conflicts

- `qa-remove-agent-max-tokens` (multi-owner on `eval/executor/trader_output.rs`
  and any future `eval/dispatcher.rs`): no collision expected from this
  track; cost lives in its own new module.
