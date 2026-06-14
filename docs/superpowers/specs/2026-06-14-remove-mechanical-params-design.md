# Remove `mechanical_params` completely

**Date:** 2026-06-14
**Status:** Approved (operator), implementing
**Branch:** `refactor/remove-mechanical-params`

## Goal

Delete the `Strategy.mechanical_params` field and the `MechanicalParams` type
from the xvision workspace, fold `min_warmup_bars` down to a manifest-only
accessor, and retire every operator/agent write-surface that targets the blob
(MCP tool, chat rail, dashboard wizard, HTTP route). Do it safely: the tree
must `cargo build` + `cargo test` green at every crate boundary, and old
on-disk strategy JSON must still load.

## Why this is safe (verified)

An adversarial mapping pass (7 agents) confirmed the central premise:
**nothing reads `mechanical_params` at runtime to make a decision.**

- The only non-test reader is `Strategy::min_warmup_bars()` →
  `typed_params().min_warmup_bars()` (a JSON period-key walker), whose result
  reaches exactly one consumer: `warn_on_warmup_mismatch` in `api/eval.rs:3716`,
  which only emits `tracing::warn!`. It gates nothing; the run continues.
- The executor decision path reads `MechanisticConfig` (mechanistic) or runs
  the LLM pipeline from a `seed` that never includes `mechanical_params`.
  Indicator periods come from `MechanisticConfig`/`briefing_indicators`.
- The autooptimizer mutates `mechanical_params` keys, but candidates are scored
  by the same backtest executor that ignores the blob — a closed/dead loop.
- `EvalRunRequest.params_override` is documented as a per-run override of
  `mechanical_params` but is **never applied** — it is only persisted and used
  as a dedup fingerprint. It stays (the watchdog reads `max_run_duration_secs`
  from the same column); only its doc comment is corrected.

**Parse safety:** `StrategyRaw` (the `Strategy` deserialize mirror) is a plain
`#[derive(Deserialize)]` with **no `deny_unknown_fields`**, so old JSON carrying
`mechanical_params` deserializes cleanly (the key is ignored). No data
migration. No-users posture (`wipe-DB-instead-of-migrations`) means zero risk.

## Design decision: `min_warmup_bars`

`min_warmup_bars()` folds to manifest-only (Option A, operator-approved):

```rust
pub fn min_warmup_bars(&self) -> u32 {
    self.manifest.min_warmup_bars.unwrap_or(FALLBACK_MIN_WARMUP_BARS)
}
```

The call site `strategy.min_warmup_bars()` is unchanged. `MechanicalParams`,
`typed_params()`, and the period-walker (`custom_max_period`,
`is_period_like_key`) are deleted. **Behavioral delta:** a strategy with period
keys in `mechanical_params` but no explicit `manifest.min_warmup_bars` now
reports `0` instead of a derived value — meaning the soft warmup warning fires
less often. Acceptable: the value only ever fed a log line.

## Design decision: optimizer surface

`mechanical_params` is the autooptimizer's primary scalar param surface
(`tunable_param_keys()` seeds from it; `strategy_diff()` diffs it;
`apply_to()`/`tournament::apply_params()` write it; `program_view` renders +
parses it; `validator` resolves baselines from it). Since the blob is never
read at decision time, **tuning it was theater** — mutations scored against an
executor that ignored them. Removing it is a **correctness improvement**: the
optimizer keeps tuning the surfaces that *are* read — `risk.*`,
`mechanistic.*`, `filter`, `tunable_bounds`. Optimizer tests that proposed
diffs on `mechanical_params` keys (e.g. `ema_fast`) are **retargeted** to a
surviving surface, not deleted, so the propose→validate→apply loop stays
proven.

## Design decision: provenance

`cloned_from`/`imported_from` are stashed in `mechanical_params.metadata` by
clone/import. **Write-only dead code** — the `cloned_from()` accessor has zero
`src/` callers (only one test + a dated audit doc). Dropped, not rehomed. (The
unrelated scenario `parent_scenario_id` "cloned_from" feature is OUT OF SCOPE.)

## Work units (execution gated by crate compile)

1. **Engine lib** (`cargo build -p xvision-engine` green): delete the field
   from `Strategy` + `StrategyRaw` + the `Deserialize` copy; delete
   `mechanical.rs`; rewrite `min_warmup_bars`; delete `typed_params()` and the
   `pub mod`/`pub use`; fix every non-test `src/` construction + field-access
   site; do the **autooptimizer** logic surgery (program_view render/parse pair,
   `strategy_diff` param loop, `risk_field_for_key` shadow lookup,
   `tunable_param_keys` seed, `apply_to` else-branch + dead `set_param_value`,
   `tournament::apply_params`, `validator::resolve_param_current_value`); remove
   the provenance stash helpers + call sites; retire `authoring::set_mechanical_param`
   + `SetMechanicalParamReq` + `api::strategy::set_mechanical_param`; correct
   `params_override` + manifest doc comments.
2. **Engine tests** (`cargo test -p xvision-engine`): delete `mechanical.rs`
   tests (with the file) and the `set_mechanical_param` test in
   `tests/mechanical_params.rs` (keep the data-field round-trip tests, strip
   their `mechanical_params`); retarget autooptimizer tests to risk/mechanistic
   keys; trim provenance assertions; strip `mechanical_params: json!({})` from
   every `#[cfg(test)]` + `tests/*.rs` construction site.
3. **xvision-mcp**: delete `xvn_set_mechanical_param` + its `SetMechanicalParamReq`
   + the `tool_names()` inventory entry + the create-draft help text; drop the
   `xvn_set_mechanical_param` entry from `tests/parity.rs` EXPECTED list (lockstep);
   delete the round-trip test; strip the create-atomic construction line + test fixtures.
4. **xvision-cli**: strip the `new_atomic`/`seed` construction lines; trim
   clone provenance doc comments; strip test fixtures + provenance assertions.
5. **xvision-dashboard**: delete the `PUT /api/strategy/:id/mechanical_params`
   route + handler + body struct + import token; delete the wizard tool def +
   dispatch arm + `expects_id` arm + help strings + test array entry; trim docs.
6. **Frontend** (`tsc` + vitest): delete `mechanical_params` from
   `api/strategies.ts`; retire the `set_mechanical_param` chat-rail narration
   (`tool-rows/registry.ts`, `ChatRail.tsx` ×2, `setup.tsx` ×2, `ChatRail.test.tsx`);
   strip the key from mock-strategy literals in 3 `*.test.tsx`; regenerate
   `types.gen` from the corrected Rust doc.
7. **On-disk JSON**: `jq 'del(.mechanical_params)'` over the 36
   `docs/strategies/templates/**/*.json` seeds (array-shaped descriptor key,
   copied as opaque prepop bytes — inert, stripped for zero references).

## Risks / watch-items

- **Round-trip coupling**: `program_view` render (L39-42) and `from_markdown`
  parse (L122) are a hard pair — remove together; `from_markdown` clones
  `base.*` for the dropped section, so drop the field from its `Strategy`
  literal.
- **Total `apply_to`**: removing the `mechanical_params` else-branch makes a
  non-risk/non-mechanistic `ParamChange` a silent no-op — the validator must
  reject such keys upstream (it will, since `tunable_param_keys` no longer
  advertises them).
- **Validator mechanistic resolution**: `resolve_param_current_value` only
  resolves `risk.*` + `mechanical_params` today; after dropping the blob
  fallback, confirm `mechanistic.*` keys still resolve a baseline (add a
  resolver if the kept surface needs it).
- **Two writers**: `MutationDiff::apply_to` and `tournament::apply_params` both
  write the blob — change both; check `apply_params` callers (delete or
  delegate to `apply_to`).
- **In-flight plan conflict**: `docs/superpowers/plans/2026-06-14-pine-input-bounds-optimizer.md`
  proposes wiring an edit endpoint to the route we delete. Flagged to operator;
  that plan tunes `tunable_bounds` (which stays) and can retarget its endpoint.

## Verification

- `cargo build` + `cargo test` per crate, then `--workspace`.
- Frontend `tsc` + vitest on changed code.
- Final: `grep -rn 'mechanical_params\|MechanicalParams\|set_mechanical_param'`
  over `crates/*/src`, `frontend/web/src` (non-test), and routes returns only
  intentional residue (scenario `parent_scenario_id`, historical evidence docs).
- Adversarial review workflow: completeness, behavior-drift, optimizer still
  tunes live surfaces, tests stayed meaningful.
