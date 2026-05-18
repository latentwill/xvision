---
track: harness-typed-mechanical-params
worktree: .worktrees/harness-typed-mechanical-params
branch: task/harness-typed-mechanical-params
phase: in-progress
last_updated: 2026-05-18T00:00:00Z
owner: claude-f6
---

# What I'm doing right now

F-6 (harness-typed-mechanical-params) code is landed on
`task/harness-typed-mechanical-params`. Three commits on top of
the conductor's claim commit:

1. `feat(strategies): typed MechanicalParams enum at the deserialize
   boundary` — new `mechanical.rs` module with per-template typed
   structs + `Custom(serde_json::Value)` fallback, custom Deserialize
   on `Strategy` that validates `mechanical_params` against the
   typed variant for `manifest.template`, `typed_params()` accessor,
   `validate_typed()` strict variant, `min_warmup_bars()` delegates
   to typed dispatch. Drive-by: drop 3 stray `prompt_version`
   inits from `ResolvedAgentSlot` test fixtures so the engine lib
   tests can build (F-3 fallout).
2. `feat(core): deny_unknown_fields + cross-field validators on
   trading types` — `deny_unknown_fields` on `InternBriefing`,
   `TraderDecision`, `RiskDecision`, `RiskConfig`/`Limits`/`Stops`,
   `RiskCaps`, `Capital`. Cross-field invariants (companion methods,
   garde 0.22 has no struct-level custom validator): TP > SL on
   directional `TraderDecision`s; `RiskStops` min ≤ max.
3. `feat(engine): pre-persist validate seam + tighten
   set_mechanical_param` — single `validate_strategy_for_persist`
   seam in `FilesystemStore::save`, `set_mechanical_param`
   validates the patched JSON against the typed variant, 11 new
   integration tests in `tests/mechanical_params.rs`, 2 more unit
   tests in `strategies::store::tests`.

Ready to flip contract status to `pr-open` once the PR is created.

# Blocked on

Nothing. F-6 is fully parallel-safe with F-2/F-3/F-4/F-5/F-7 —
disjoint files. The F-5 contract explicitly carves out
`crates/xvision-engine/src/strategies/**` as F-6's territory.

Pre-existing F-3 fallout (test SQLite schema lacks `prompt_version`
column → 26 test failures across `agents::store`, `authoring`,
`eval::postprocess`, attestation, eval-run, set_pipeline) is
**unchanged from baseline `origin/main` + the build-fix patch in
PR #299 / this branch**. Verified via a throwaway worktree at
`/tmp/xvision-pre-f6` running the same test suite — same exact 26
failures. **F-6 introduces zero regressions.**

# Verification

```
cargo build --workspace
→ clean (only pre-existing dead_code warnings in
  xvision-engine/src/api/eval.rs test fixtures).

cargo test -p xvision-core
→ 92 passed; 0 failed (88 lib + 4 integration).
  14 new tests added by F-6 (deny_unknown_fields + cross-field
  invariants on InternBriefing/TraderDecision/RiskDecision/
  RiskConfig/RiskStops).

cargo test -p xvision-engine --lib strategies
→ 59 passed; 0 failed (46 baseline + 11 mechanical unit + 2 store).

cargo test -p xvision-engine --test mechanical_params
→ 11 passed; 0 failed.

cargo test -p xvision-engine --tests --no-fail-fast
→ 624 passed; 26 failed. The 26 are pre-existing F-3 fallout
  (test SQLite schema missing prompt_version column). Baseline
  verified — F-6 contributes 25 new green tests and zero new
  failures.

cargo clippy -p xvision-core --no-deps
→ 1 pre-existing warning (validate_provider_name &String → &str)
  unrelated to F-6. xvision-observability and xvision-engine
  have pre-existing clippy errors on `origin/main` too (verified
  via /tmp/xvision-pre-f6 worktree); not introduced by F-6.
```

# Next up

1. Open PR for `task/harness-typed-mechanical-params`.
2. Flip contract `status:` from `claimed` to `pr-open` and add the
   PR number to the board entry. Push the conductor sweep
   reflecting the new state.
3. After merge, file a separate F-3-fallout track to repair the
   test SQLite migration (`prompt_version` column on `agent_slots`)
   so the 26 baseline-broken tests come back green.
