---
track: cli-eval-model-override
lane: leaf
wave: cli-operator-safety-wave-b-2026-05-22
worktree: .worktrees/cli-eval-model-override
branch: task/cli-eval-model-override
base: origin/main
status: ready
depends_on: []
blocks:
  - cli-model-bakeoff                                              # bakeoff per-arm model override delegates to this verb's mechanism
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/eval/mod.rs                    # extend RunArgs with --provider / --model
  - crates/xvision-engine/src/api/eval.rs                          # EvalRunRequest carries an optional override; resolve_provider honors it
  - crates/xvision-engine/src/eval/run.rs                          # propagate override into the agent dispatch
  - crates/xvision-engine/migrations/034_eval_model_override.sql   # NEW (optional — only if override receipt needs persistence; see Notes)
  - crates/xvision-engine/migrations/034_eval_model_override.down.sql # NEW (paired)
  - crates/xvision-cli/tests/eval_model_override_cli.rs            # NEW
  - crates/xvision-engine/tests/eval_model_override.rs             # NEW
forbidden_paths:
  - crates/xvision-cli/src/commands/strategy.rs                    # cloning is the sibling track cli-strategy-clone-model-override
  - frontend/web/**
  - crates/xvision-mcp/**
interfaces_used:
  - xvision_engine::api::eval::EvalRunRequest                      # carries optional override fields
  - xvision_engine::api::settings::providers::resolve_provider     # honors override before falling back to the strategy's bound provider
  - xvision_engine::eval::limits::EvalLimits                       # the override does not bypass hard limits (PR #428)
parallel_safe: true                                                # narrow surface; no overlap with Wave A's now-merged files except eval/mod.rs RunArgs (additive)
parallel_conflicts:
  - cli-strategy-clone-model-override                              # both in the same wave; coordinate via team/queue/ if both in-flight
  - cli-model-bakeoff                                              # depends on this; will rebase when this merges
verification:
  - cargo test -p xvision-cli --test eval_model_override_cli
  - cargo test -p xvision-engine --test eval_model_override
  - cargo test -p xvision-cli
acceptance:
  - **CLI flags.** `xvn eval run` gains `--provider <name>` and `--model <id>`. Both must be supplied together or both omitted; supplying one without the other returns an exit-2 usage error with a clear message ("--model requires --provider").
  - **Engine plumbing.** `EvalRunRequest` (in `crates/xvision-engine/src/api/eval.rs`) gains an optional `provider_override: Option<ProviderOverride>` field with shape `{ provider: String, model: String }`. The launch path resolves this via `effective_providers::resolve_provider` (the helper shipped in #530); if the override is unreachable (`key_missing`, `provider_disabled`, `model_disabled`, `provider_unknown`), the launch refuses with the same structured `reason` the strategy-bound-provider path uses.
  - **Override receipt.** Each run launched with an override stores the `(strategy_id, agent_id, provider, model)` it actually used in a stable, queryable form. Implementation choice: either (a) extend `eval_runs.provider_diagnostics` JSON to carry an `override: { provider, model }` block, or (b) add an `override_provider TEXT, override_model TEXT` pair on `eval_runs` via migration 034. Pick (a) if `provider_diagnostics` already round-trips through ts-rs without ceremony; pick (b) if the field is read in enough SQL joins to make a query-side column worth the migration cost. Document the decision in the PR description and in `Notes:`.
  - **No strategy mutation.** The override is per-run. The strategy's bound provider/model on disk is unchanged. The override does not create a derived strategy ID (that's `cli-strategy-clone-model-override`'s job).
  - **Hard limits still apply.** The override does not bypass `EvalLimits` (PR #428). A run with `--max-output-tokens 100` and an override to a chatty model still gets cancelled at 100.
  - **JSON contract honored.** `--json` stdout-only discipline from `cli-json-stdout-contract` (PR #531) is preserved. The new override field appears in the `eval results --json` output.
  - **Tests.** Two new integration tests:
    - `eval_model_override_cli.rs`: launches a backtest with `--provider <X> --model <Y>` where the strategy bound `(provider, model)` is different; asserts the resolved provider+model on the run match the override, not the strategy default.
    - `eval_model_override.rs`: engine-level test that `EvalRunRequest { provider_override: Some(...) }` routes through `resolve_provider` and refuses on a `key_missing` provider with the typed reason.

---

# Scope

Track #5 of `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`.

Today an operator who wants to retest a strategy under a new model must clone the strategy in the dashboard, rewire the agent, and launch a new eval — three steps to test one hypothesis. This contract adds an ephemeral per-launch override: keep the strategy as-is, supply `--provider X --model Y` on `xvn eval run`, and the engine resolves to the override for that run only.

The override produces a receipt (in `provider_diagnostics` or a new column) so the eval export, the dashboard run-detail view, and `xvn eval results` can show the actual `(provider, model)` used — operators reading a results table see at a glance which model produced which sharpe.

# Out of scope

- Permanent strategy edits — that's `cli-strategy-clone-model-override`.
- A "model" CLI subcommand surface — that's `cli-model-bakeoff`.
- Dashboard UI for the override. The engine accepts the field; SPA adoption is a follow-on.
- Multi-override per run (e.g. one model per agent slot). For now the override applies to every LLM-backed slot on the strategy; per-slot overrides are a v2 if operators ask.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/cli-eval-model-override -b task/cli-eval-model-override origin/main
cd .worktrees/cli-eval-model-override
# Per-worktree target dir — shared $HOME/.cargo-target/xvision collides under
# concurrent cargo. Use a per-track suffix.
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-eval-model-override"
```

# Notes

**Migration vs JSON.** The contract's acceptance bullet 3 lets the worker pick. Default lean: extend `provider_diagnostics` JSON (option a) — no migration risk and ts-rs already carries that field. Pick the column path only if a query-side filter ("show me every run that ran with override X") becomes urgent, which today is not on the roadmap.

**Per-slot future.** Today's intake says the override applies to the trader-style slot. With multi-agent strategies (memory `[[project_multi_agent_strategies]]`) an operator may want per-slot overrides. Punt: ship the single-override now; revisit when a real ask lands.

**Allowlist already pre-allows this.** The dashboard remote CLI allowlist already accepts `eval run` with bounded flags. Add `--provider` and `--model` to the permitted flag list in `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` (single-line additions, scoped to the existing `["eval", "run"]` template — or whichever STRICT_TEMPLATES entry covers `eval run` today).
