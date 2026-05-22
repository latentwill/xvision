---
track: cli-model-bakeoff
lane: integration
wave: cli-operator-safety-wave-b-2026-05-22
worktree: .worktrees/cli-model-bakeoff
branch: task/cli-model-bakeoff
base: origin/main
status: ready
depends_on:
  - cli-eval-model-override                                        # per-arm override delegates to this verb's mechanism
  - cli-strategy-clone-model-override                              # alt-mode bakeoff materializes cloned strategies; needs the clone primitive
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/model.rs                       # NEW — `xvn model bakeoff` subcommand
  - crates/xvision-cli/src/commands/mod.rs                         # register the new `model` subcommand
  - crates/xvision-cli/src/main.rs                                 # dispatch wiring (minimal — one match arm)
  - crates/xvision-engine/src/api/eval/bakeoff.rs                  # NEW — engine-side orchestrator that drives the strategy×model matrix
  - crates/xvision-engine/migrations/035_eval_bakeoffs.sql         # NEW — persisted bakeoff record (run_ids[], status, params, summary)
  - crates/xvision-engine/migrations/035_eval_bakeoffs.down.sql    # NEW (paired)
  - crates/xvision-dashboard/src/cli_jobs/allowlist.rs             # extend the existing `["model", "bakeoff"]` STRICT_TEMPLATES entry with any new flags
  - crates/xvision-cli/tests/model_bakeoff_cli.rs                  # NEW
  - crates/xvision-engine/tests/eval_bakeoff_orchestrator.rs       # NEW
forbidden_paths:
  - frontend/web/**                                                # SPA-side bakeoff UI is a separate wave
  - crates/xvision-mcp/**                                          # MCP parity is a follow-on
  - crates/xvision-cli/src/commands/strategy.rs                    # cloning is the sibling cli-strategy-clone-model-override track
  - crates/xvision-cli/src/commands/eval/**                        # eval-launch override is cli-eval-model-override
interfaces_used:
  - xvision_engine::api::eval::EvalRunRequest                      # bakeoff issues N of these (one per arm)
  - xvision_engine::api::eval::cancel                              # used to honor `--max-runs` cap and limit-breach cancellation
  - xvision_engine::eval::limits::EvalLimits                       # bakeoff passes operator-supplied caps through
  - xvision_engine::eval::compare::ComparisonReport                # bakeoff `--compare --markdown` reuses the existing report rendering (PR #532)
  - The override receipt from `cli-eval-model-override`            # each bakeoff run carries its `(provider, model)` in `provider_diagnostics`
parallel_safe: false                                               # touches CLI dispatch wiring (mod.rs + main.rs); coordinate with any other Wave B track that touches them
parallel_conflicts:
  - cli-eval-model-override                                        # blocking dependency
  - cli-strategy-clone-model-override                              # blocking dependency
verification:
  - cargo test -p xvision-cli --test model_bakeoff_cli
  - cargo test -p xvision-engine --test eval_bakeoff_orchestrator
  - cargo test -p xvision-cli
  - cargo test --workspace
acceptance:
  - **`xvn model bakeoff` is the new verb.** Subcommand at `crates/xvision-cli/src/commands/model.rs`. Required flags: `--strategies <comma-ids>` (1..N strategy ids), `--scenario <id>` (a single scenario; multi-scenario is a v2). Required model selector: `--provider <name>` + `--models <comma-ids>` (1..N models). Either supplied together — supplying neither and relying on each strategy's bound model returns a usage error ("either supply --provider/--models or use --use-strategy-models to opt into per-strategy defaults"). Optional: `--name <bakeoff-name>`, `--max-runs <n>` (default = strategies × models; explicit cap is operator-friendly), `--sequential` (default for LLM-backed strategies, parallel allowed only via `--parallel`), `--wait`, `--compare`, `--markdown`, `--yes` (skip dry-run gate).
  - **Two modes for materialization.** Default `--mode override`: each arm uses `cli-eval-model-override`'s per-run override receipt (no new strategy records). `--mode clone` materializes one cloned strategy per `(strategy, model)` arm via `cli-strategy-clone-model-override`, producing a durable bakeoff library. `--mode clone` requires a `--clone-name-template` (e.g. `"{strategy}-{model}-bakeoff"`).
  - **Dry-run plan + `--yes` gate.** Same shape as `experiment-run-scope-guardrails` (PR #429): print the full plan to stderr before launching unless `--yes` is supplied. Plan lists: total arms (strategies × models), max-runs cap, per-arm `(strategy, provider, model)`, decision/token/wall-clock caps, expected total cost upper bound. With `--yes`, plan still prints; launch proceeds.
  - **Hard limit propagation.** `--max-decisions`, `--max-input-tokens`, `--max-output-tokens`, `--max-wall-clock` apply per-arm and route through `EvalLimits` (PR #428). Bakeoff-level total token ceiling enforced via summing per-arm caps in the dry-run plan.
  - **Persisted bakeoff record.** Migration 035 creates `eval_bakeoffs(id, name, status, params_json, summary_json, started_at, completed_at)` and `eval_bakeoff_runs(bakeoff_id, run_id, arm_strategy_id, arm_provider, arm_model, status)`. `xvn model bakeoff status <bakeoff_id>` reads the record + joined runs (similar shape to `xvn eval batch status`).
  - **Compare integration.** With `--compare --markdown`, after all arms reach terminal state the verb emits the `ComparisonReport` (PR #532) over the bakeoff's run ids and prints the markdown table to stdout. With `--compare` only, the report is emitted as JSON. The 10-run cap from `compare_runs` applies — if the bakeoff has more than 10 arms, the verb chunks the compare into multiple tables (one per 10-arm slice) and notes the chunking in the output.
  - **Cancel honors per-arm limits.** When an arm breaches a hard limit, it transitions to `cancelled_limit` (PR #428's terminal status); the bakeoff record marks it accordingly; the remaining queued arms continue per `--sequential` ordering.
  - **JSON discipline.** `--json` returns one object: `{ bakeoff_id, name, status, arms: [...], summary: { ... }, run_reports: [...] }`. Stdout-only per PR #531.
  - **Allowlist.** The dashboard remote CLI allowlist already has a `["model", "bakeoff"]` STRICT_TEMPLATES entry. This contract verifies/extends the `permitted_flags` list to cover every flag above. Bounded-only — the dashboard cannot pass arbitrary flags.
  - **Tests.**
    - `model_bakeoff_cli.rs`: launches a 2-strategy × 2-model bakeoff against a tiny seeded scenario in `--mode override --sequential --wait --max-decisions 10`; asserts all 4 arms reach terminal, each run carries its override receipt, the comparison report (with `--compare --markdown`) lists all 4 arms.
    - `eval_bakeoff_orchestrator.rs`: engine-side test that the orchestrator respects `--max-runs` (e.g. matrix of 4 arms with `--max-runs 2` launches exactly 2), and that per-arm limit breach cancels just that arm.

---

# Scope

Track #6 of `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`, absorbing #7 (`cli-two-run-rerun-workflow` is the common-case shape of this verb, not a separate surface).

This is the headline operator verb the Hermes session in the intake exists to enable. The acceptance sketch in the intake (the example bakeoff invocation against two BTC strategies × Gemini 3.5 Flash) is the canonical use case. After this lands, the Python orchestration the operator was forced to write is replaced by one CLI invocation.

The verb sits on top of the Wave A + earlier-merged primitives:
- Eval cancel (#425), hard limits (#428), scope guardrails (#429) — the safety floor.
- Provider parity (#530) — bakeoff trusts that "provider X model Y" is launchable iff `effective_providers::resolve_provider` says so.
- JSON stdout discipline (#531) — the structured bakeoff output is parseable.
- Results enrichment (#532) — the comparison table at the end of `--compare` carries tokens, wall clock, and action distribution.
- Override (Wave B sibling) + clone (Wave B sibling) — the two materialization modes.

# Out of scope

- Multi-scenario bakeoffs. v2 if needed (sweep across (strategy × model × scenario) cube). Today: one scenario per bakeoff invocation.
- Cross-bakeoff aggregation. Each bakeoff is its own record. "Compare these 3 bakeoffs" is a v2 ask.
- Per-slot model overrides in multi-agent strategies. Single `--provider/--model` applies to every LLM-backed slot in each arm.
- Dashboard SPA UI. The engine + CLI ship here; SPA adopts in a follow-on UI track.
- MCP parity. Follow-on.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/cli-model-bakeoff -b task/cli-model-bakeoff origin/main
cd .worktrees/cli-model-bakeoff
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-model-bakeoff"

# This contract depends on cli-eval-model-override and cli-strategy-clone-model-override.
# If either is not yet merged, rebase on its branch — or proceed from origin/main
# and rebase mechanically when each merges. The verb's CLI flag surface compiles
# without the dependencies; the engine plumbing for `--mode override` and
# `--mode clone` reads from the sibling contracts' exposed APIs.
```

# Notes

**Decomposition rationale.** Intake #6 is the headline; #7 is the common-case wrapper. They share the same surface: a verb that launches N runs against a (strategy × model) matrix, bounded, sequential, optionally compared. Shipping them as one contract avoids gratuitous indirection.

**Migration 035.** Confirm the migration number is free at sync-before-work. Per `team/CONFLICT_ZONES.md`, migration numbers are claimed via the contract. If 034 (claimed by `cli-eval-model-override` if it ships a migration) and 035 land in the same wave, the bakeoff contract gets 035.

**Compare chunking note.** `compare_runs` caps at 10. A 5-strategy × 4-model bakeoff has 20 arms; the markdown output prints two tables. The chunking is mechanical (split by index); the v2 path is to extend `compare_runs` itself, which is out of scope here.

**`--use-strategy-models` flag.** An escape hatch for the operator who wants to bakeoff with each strategy's natively-bound model (i.e. "compare these 3 strategies as authored"). Mutually exclusive with `--models`.

**Allowlist already pre-allows this.** Per `crates/xvision-dashboard/src/cli_jobs/allowlist.rs`, the `["model", "bakeoff"]` STRICT_TEMPLATES entry was added by V2B remote-cli-job-safety in anticipation of this verb. Extend its `permitted_flags` list as you add flags here — do NOT change the head or the safe-to-surface principle.
