---
track: strategy-require-at-least-one-agent-fixture-migration
lane: leaf
wave: qa-2026-05-19
worktree: .worktrees/strategy-require-at-least-one-agent-fixture-migration
branch: task/strategy-require-at-least-one-agent-fixture-migration
base: origin/main
status: merged
depends_on: []                                                  # original track shipped via #341 (commit 3849680, partial)
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/eval.rs                       # drop the legacy trader_slot fallback branch in validate_eval_trader_source
  - crates/xvision-engine/tests/risk_min_notional.rs
  - crates/xvision-engine/tests/pipeline_inline.rs
  - crates/xvision-engine/tests/eval_progress.rs
  - crates/xvision-engine/tests/decisions_count.rs
  - crates/xvision-engine/tests/strategy_store.rs
  - crates/xvision-engine/tests/api_eval_min_notional.rs
  - crates/xvision-engine/tests/strategy_roundtrip.rs
  - crates/xvision-engine/tests/eval_executor_warmup.rs
  - crates/xvision-engine/tests/eval_executor_paper.rs
  - crates/xvision-engine/tests/eval_broker_circuit_breaker.rs
  - crates/xvision-engine/tests/eval_progress_backtest.rs
  - crates/xvision-engine/tests/api_eval_run.rs
  - crates/xvision-engine/tests/eval_run_scenario.rs
  - crates/xvision-engine/tests/eval_early_stop.rs              # also carries trader_slot per recon
  - crates/xvision-engine/tests/eval_guardrails.rs              # also carries trader_slot per recon
  - crates/xvision-engine/tests/strategy_id_path_safety.rs      # uses agents: vec![] with no trader_slot — re-verify after fallback removal
  - crates/xvision-engine/tests/mechanical_params.rs            # uses agents: vec![] — re-verify
forbidden_paths:
  - crates/xvision-engine/src/strategies/**                     # don't touch validation logic; the gate already exists
  - crates/xvision-engine/src/strategies/templates.rs           # CLI auto-migration path stays intact — intake explicitly carves templates out
  - crates/xvision-cli/**                                       # CLI surface already shipped via #341
  - frontend/web/**                                             # frontend gate already shipped
  - crates/xvision-engine/migrations/**                         # no schema changes
interfaces_used:
  - Strategy                                                    # crates/xvision-engine/src/strategies/types.rs
  - AgentRef                                                    # role: "trader" attaches the test trader agent
  - LLMSlot                                                     # legacy struct stays for serde compatibility; just not used in tests
  - validate_eval_trader_source                                 # crates/xvision-engine/src/api/eval.rs:952 — branch to delete
parallel_safe: false                                            # touches eval.rs which several active tracks read; coordinate via team/queue/
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine
  - cargo test --workspace
acceptance:
  - **All ~13 engine fixtures migrated.** Each test under `crates/xvision-engine/tests/` listed in `allowed_paths` that today builds a `Strategy` with `agents: vec![]` + `trader_slot: Some(LLMSlot { ... })` is rewritten to `agents: vec![AgentRef { agent_id, role: "trader".into() }]` with the legacy `trader_slot` field dropped (or set to `None` if the struct still requires it for serde). Where the test exercises the agent slot resolution, attach a real `Agent` record to the test's agent store with the matching `agent_id`.
  - **Legacy fallback branch deleted.** The "2. Legacy `trader_slot` on the strategy." branch in `crates/xvision-engine/src/api/eval.rs` (around `:1830-1855` per recon — verify on intake) is removed. `validate_eval_trader_source` rejects every empty-agents strategy with the no-agent-attached error, regardless of `trader_slot`.
  - **Matching legacy-acceptance test deleted.** `eval_trader_source_accepts_legacy_trader_slot_without_agents` (around `crates/xvision-engine/src/api/eval.rs:2573`) is removed. The "explicit missing-agent rejection" test at `:2598` stays.
  - **`cargo test --workspace` green.** No regressions in non-engine crates. Workspace-wide test pass.
  - **CLI auto-migration intact.** `xvn strategy create --template <name>` still produces a valid strategy because the CLI inserts an `Agent` row before persisting. The seven seed templates' `new_draft()` paths emitting legacy slots is explicitly out-of-scope (intake §"Out of scope"); the CLI's auto-migration layer covers it.
  - **No silent disabling of tests.** No `#[ignore]`, no commenting-out, no `#[cfg(not(test))]` to hide failures. If a fixture is irreducibly broken under the new validation, escalate via the conductor.

---

# Scope

Followup track carved from QA Round 4 (`team/intake/2026-05-19-qa-operator-round-4.md`,
§"Followups → `strategy-require-at-least-one-agent-fixture-migration`").
The original `strategy-require-at-least-one-agent` track landed
partially in PR #341 (commit `3849680`) — the validation message and the
CLI/UI gates ship today. What did not ship: dropping the legacy
`trader_slot` fallback branch inside `validate_eval_trader_source`,
because doing so requires migrating ~13 engine test fixtures that
construct `Strategy` structs with `agents: vec![]` + `trader_slot:
Some(LLMSlot { ... })`. This contract finishes that migration and
removes the fallback branch.

Per CLAUDE.md (terminology lock 2026-05-10) and the strategies refactor
(2026-05-12), legacy slot fields are free-text role labels — not the
source of truth. The `trader_slot` field on `Strategy` exists today
only so persisted JSON written before the refactor still deserializes.
At the eval boundary, the source of truth is `Strategy.agents`.

# Out of scope

- Rewriting the seven seed templates' `new_draft()` paths in
  `crates/xvision-engine/src/strategies/templates.rs` to emit `agents:
  Vec<AgentRef>` directly. Per the intake, the CLI auto-migrates these
  at save time, so changing the templates is a separate cleanup.
- Deleting the `trader_slot` field from `Strategy` itself. Removing the
  field is breaking for any persisted strategy JSON that still carries
  it; this contract only stops *reading* from it at the eval boundary.
  Schema-level removal is a future migration.
- Changing the frontend "at least one agent" gate (already shipped via
  #341, paths in `forbidden_paths`).
- Touching the CLI's auto-migration in `crates/xvision-cli/src/commands/strategy.rs`.
  Already wired.
- Adding *new* validation rules. This is a fallback removal, not an
  expansion.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategy-require-at-least-one-agent-fixture-migration status
git -C .worktrees/strategy-require-at-least-one-agent-fixture-migration log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategy-require-at-least-one-agent-fixture-migration -b task/strategy-require-at-least-one-agent-fixture-migration origin/main
```

# Notes

Recon (2026-05-21) surfaced two fixtures the intake's list did not
include but that also build `Strategy { agents: vec![], trader_slot:
Some(...) }`:

- `crates/xvision-engine/tests/eval_early_stop.rs:120`
- `crates/xvision-engine/tests/eval_guardrails.rs:109`

Plus two fixtures that already use `agents: vec![]` with no
`trader_slot` — re-verify these don't already fail under the current
validator before the fallback removal:

- `crates/xvision-engine/tests/strategy_id_path_safety.rs:33`
- `crates/xvision-engine/tests/mechanical_params.rs:38`

The legacy-fallback branch location to remove is around
`crates/xvision-engine/src/api/eval.rs:1830-1855` ("Legacy `trader_slot`
on the strategy."). Confirm with `grep -n "trader_slot" eval.rs` on
intake — recent merges may have shifted line numbers.
