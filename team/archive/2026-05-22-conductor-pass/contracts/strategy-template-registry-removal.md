---
track: strategy-template-registry-removal
lane: foundation
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/strategy-template-registry-removal
branch: task/strategy-template-registry-removal
base: origin/task/templates-elimination
status: merged
depends_on:
  - templates-elimination
blocks: []
stacking: declared:templates-elimination
allowed_paths:
  - crates/xvision-engine/src/templates/**
  - crates/xvision-engine/src/strategies/manifest.rs
  - crates/xvision-engine/src/strategies/mechanical.rs
  - crates/xvision-engine/src/strategies/mod.rs
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/strategies/templates.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/src/strategies_folder/prepop.rs
  - crates/xvision-engine/src/strategies_folder/prepop/**
  - crates/xvision-engine/src/strategies_folder/mod.rs
  - crates/xvision-engine/tests/**
  - crates/xvision-dashboard/tests/**
  - crates/xvision-mcp/src/tools.rs
  - crates/xvision-cli/src/commands/strategy.rs
  - crates/xvision-cli/tests/strategy_cli.rs
  - docs/strategies/templates/**
  - frontend/web/src/api/types.gen/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/templates.rs
  - crates/xvision-engine/src/agents/validate.rs
  - crates/xvision-engine/src/eval/**
  - crates/xvision-observability/**
  - frontend/web/src/routes/**
  - frontend/web/src/features/**
  - frontend/web/src/components/**
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/prompts/wizard.md
interfaces_used:
  - authoring::CreateStrategyReq / CreateStrategyOut
  - api_strategy::create_strategy
  - strategies::manifest::StrategyManifest
  - strategies::mechanical::MechanicalParams
  - strategies_folder::prepop seed surface
parallel_safe: false
parallel_conflicts:
  - "Heavy engine refactor. Cannot run in parallel with any other engine track."
verification:
  - cargo test --workspace
  - cargo clippy --workspace -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **`template_registry` removed.** `crates/xvision-engine/src/templates/` directory deleted (8 strategy starter files + `mod.rs` + `registry.rs`). The marketplace baseline (`baselines::ma_crossover::ma_crossover_template()`) is decoupled from the registry — relocate it to a baseline-specific module under `crates/xvision-engine/src/baselines/` or fold into prepop seed content; worker's call.
  - **`authoring::create_strategy` no longer consults `template_registry`.** Either:
    - (a) Drop the `template` field from `CreateStrategyReq` entirely; rewrite `create_strategy` to build a blank `Strategy` directly.
    - (b) Keep `CreateStrategyReq.template` as a free-text label for the strategy (operator-chosen tag, not a registry key); `create_strategy` ignores it for scaffolding.
    - Worker decides; document the choice in the PR description.
  - **`manifest.template` field removal + `MechanicalParams` refactor.** The `template: String` discriminator on `StrategyManifest` is removed. `MechanicalParams::from_value(template, value)` is refactored to dispatch on something other than the manifest template string. Reasonable options:
    - Collapse `MechanicalParams` to a single `Custom`-style variant that holds raw JSON; the typed per-template variants are gone.
    - Move the discriminator onto the `MechanicalParams` variant itself (self-describing JSON with a `kind` field), letting serde dispatch.
    - Drop the `MechanicalParams` enum entirely and replace with `serde_json::Value` on the manifest, with validation moving into a domain-specific module.
    - Worker decides; document the choice.
  - **Strategy template content migrates to prepop seeds.** The 8 strategy starters' content (display_name, plain_summary, agent prompts, mechanical_params shapes) becomes markdown seed entries under `docs/strategies/templates/` (the existing prepop pipeline reads from there via `include_dir!`). Seeds are operator-readable markdown with front-matter, not raw JSON. The marketplace baseline migrates similarly.
  - **CLI surface updated.** `xvn strategy create --template <name>` either:
    - Removes the `--template` flag (operator scaffolds via the folder + wizard or via `xvn strategies init` for prepop), OR
    - Keeps the flag but reads the named template from the strategies folder rather than from a deleted `template_registry`.
    - Worker decides; document the choice.
  - **MCP `create_strategy` tool surface updated.** Schema reflects the new `CreateStrategyReq` shape. If the `template` field is dropped, the tool's input schema sheds it.
  - **`/api/agents/templates` (AgentTemplate picker) is unaffected.** That surface is the agent-picker, not strategy templates. Forbidden_paths includes `crates/xvision-engine/src/agents/templates.rs` to prevent accidental scope creep.
  - **Backward-compat for on-disk strategy manifests.** Existing manifests carrying `template: "trend_follower"` (or any other registry name) must continue to load. Use `#[serde(default, skip_serializing)]` on a private read-shim if needed. Add a regression test.
  - **Frontend agents-new page** consumes `/api/agents/templates` (AgentTemplate) — unaffected by this contract.
  - **Hard save-gate untouched.** `crates/xvision-engine/src/agents/validate.rs` not edited.
  - **Wizard files untouched.** This contract is engine-side only. The wizard's templates-elimination work already merged via the parent contract; this follow-up does not edit `wizard_loop.rs` or `prompts/wizard.md`.
  - **Tests:**
    - Existing tests that reference `template_registry`, `template: "trend_follower"`, etc. (~11 files across `tests/seven_templates.rs`, `tests/template_validation.rs`, `tests/tokens.rs`, `tests/strategy_update_metadata.rs`, `tests/llm_dispatch.rs`, `tests/mechanical_params.rs`, `tests/strategy_roundtrip.rs`, `tests/seeded_artifacts.rs`, dashboard `tests/http.rs`, `tests/inspector_routes.rs`, `tests/strategy_patch_route.rs`) are migrated to the new shape. Either update the assertions to the new behavior or delete tests that are no longer meaningful.
    - New: backward-compat manifest read test.
    - New: end-to-end strategy creation via the folder (no template).
  - **Grep clean:** `rg --hidden -n 'template_registry|manifest\.template' crates/ frontend/` returns only the optional `#[serde(default, skip_serializing)]` shim and the deletion-adjacent occurrences.
---

# Scope

Follow-up to `templates-elimination`. Removes the strategy
`template_registry` and `manifest.template` discriminator from the
engine; migrates the 8 strategy starter shapes to operator-readable
prepop seeds under `docs/strategies/templates/`; updates the CLI
and MCP surfaces that consumed the registry.

This is the heavy engine-side refactor the parent contract
deferred. It crosses 5 crates and ~23 source files plus ~11 test
files. The wizard work landed first (parent contract) so the
operator-visible deadlock is unblocked; this follow-up takes its
time on the engine refactor without urgency.

# Out of scope

- `crates/xvision-engine/src/agents/templates.rs` — `AgentTemplate`
  for the `/agents/new` agent-picker. Distinct concept from
  strategy templates. **Stays.**
- `crates/xvision-engine/src/agents/validate.rs` — the hard
  save-gate. Untouched.
- The wizard. The parent contract already handled it.
- Frontend UI changes (route table, components, features).
- Eval engine, observability, broker, wallet plan.
- DB migrations.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategy-template-registry-removal status
git -C .worktrees/strategy-template-registry-removal log --oneline -3 origin/main..HEAD
# Confirm templates-elimination has merged before claiming this track:
git -C .worktrees/strategy-template-registry-removal log origin/main --oneline | grep -i templates-elimination
```

If the worktree does not exist (only create after `templates-elimination` lands on origin/main):

```bash
git fetch --prune origin
git worktree add .worktrees/strategy-template-registry-removal \
  -b task/strategy-template-registry-removal origin/main
```

# Notes

Status starts as `deferred`. Becomes `ready` when
`templates-elimination` merges. Conductor reassigns status as part
of the merge sweep.

This contract is the result of the 2026-05-21 conductor descope —
see the "Conductor descope decision" section of
`team/contracts/templates-elimination.md` for the rationale and
the worker checkpoint that surfaced the scope mismatch.

Several judgment calls the worker will need to make:

1. How to replace the `MechanicalParams::from_value(template, value)`
   typed dispatch (collapse to one variant, self-describing JSON,
   or drop the enum).
2. Whether `CreateStrategyReq.template` survives as a free-text
   label or disappears entirely.
3. Whether `xvn strategy create --template <name>` keeps the flag
   (reading from the folder) or sheds it.
4. Where to relocate the marketplace baseline (`baselines::ma_crossover`)
   now that `template_registry` is gone.

Each decision is documented in the PR description.

Append checkpoints / PR links below.
