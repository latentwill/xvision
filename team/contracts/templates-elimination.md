---
track: templates-elimination
lane: foundation
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/templates-elimination
branch: task/templates-elimination
base: origin/main
status: ready
depends_on: []
blocks:
  - wizard-folder-recall-honesty
stacking: none
allowed_paths:
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/src/strategies/manifest.rs
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/agents/templates.rs
  - crates/xvision-engine/src/strategies_folder/prepop.rs
  - crates/xvision-engine/src/strategies_folder/prepop/**
  - crates/xvision-engine/src/strategies_folder/mod.rs
  - crates/xvision-engine/tests/strategies_folder.rs
  - crates/xvision-engine/tests/authoring*.rs
  - crates/xvision-engine/tests/strategy_api*.rs
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/tests/wizard_loop.rs
  - crates/xvision-dashboard/prompts/wizard.md
  - frontend/web/src/api/types.gen/**           # regenerated when ts-rs Rust types change
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/validate.rs
  - crates/xvision-engine/src/eval/**
  - crates/xvision-observability/**
  - frontend/web/src/routes/**
  - frontend/web/src/features/**
  - frontend/web/src/components/**
interfaces_used:
  - authoring::CreateStrategyReq / CreateStrategyOut
  - api_strategy::create_strategy / create_strategy_agent
  - strategies::manifest::StrategyManifest
  - strategies_folder::prepop (existing seed surface)
  - wizard_loop::run_tool dispatch (create_strategy, list_templates, list_strategies_folder, list_strategy_ideas)
  - wizard_loop::agent_tool_defs (tool registration)
parallel_safe: false
parallel_conflicts:
  - "Holds wizard_loop.rs and authoring.rs single-writer for the wave. wizard-folder-recall-honesty waits on this contract to merge."
verification:
  - cargo test -p xvision-engine
  - cargo test -p xvision-dashboard
  - cargo clippy --workspace -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **Engine surface:** `CreateStrategyReq` (`crates/xvision-engine/src/authoring.rs:43`) no longer has a `template` field. `authoring::list_templates` is removed. `authoring::create_strategy` builds a blank single-agent draft directly, without consulting any template registry; the `agents` slot starts empty (no placeholder prompt). The draft is valid for `update_slot` / `update_strategy` to fill in before save.
  - **Manifest:** `crates/xvision-engine/src/strategies/manifest.rs:10` (`pub template: String`) is removed from `StrategyManifest`. For backward compat with existing on-disk manifests that carry the field, leave one release of `#[serde(default, skip_serializing)]` on a private read-shim if the worker decides that's worth it; either way the field is not part of the public Rust struct shape and is not surfaced on the API. A follow-up release deletes the shim.
  - **API surface:** `crates/xvision-engine/src/api/strategy.rs:40` `template: String` field on the API request shape is removed. The downstream tag derivation at `:428` (`push_unique_tag(... manifest.template ...)`) is removed. The `strategy.manifest.template` read at `:270` is removed. ts-rs regenerates the corresponding `frontend/web/src/api/types.gen/**` files automatically.
  - **Pipeline-template content migration:** `crates/xvision-engine/src/agents/templates.rs` (615 lines) is deleted. Its content (per-template `display_name`, `plain_summary`, agent prompts, mechanical_params shapes) is migrated to seed entries under `crates/xvision-engine/src/strategies_folder/prepop.rs` (or sibling files under a new `prepop/seeds/` directory — worker picks the layout consistent with the merged `strategies-folder-prepopulation` contract, #419). Seeds are markdown with front-matter, operator-readable, not raw JSON.
  - **Wizard scaffolding:** `WIZARD_BLANK_TEMPLATE` (`crates/xvision-dashboard/src/wizard_loop.rs:47-100`) is removed. The `create_strategy` handler at `:779-800` no longer maps `template: None → "custom"`. `WizardCreateStrategyInput::template` is removed. The wizard's `create_strategy` tool schema (`:2077` area) no longer declares a `template` field.
  - **Wizard tool surface:** the `list_templates` tool dispatch branch and its `agent_tool_defs` entry are removed from `wizard_loop.rs`. `list_strategies_folder` and `list_strategy_ideas` are unchanged.
  - **Defensive fix for chained-write:** the wizard's `create_strategy` handler does not cache `self.last_draft_id` from a failed response. On `create_strategy` failure, the wizard surfaces the engine error verbatim and does not chain `create_strategy_agent` against a phantom id. New unit/integration test asserts: simulated `create_strategy` failure → no follow-on `create_strategy_agent` call observed.
  - **Wizard prompt:** `crates/xvision-dashboard/prompts/wizard.md` is rewritten so the library surface narrative points at the strategies folder only — no mention of templates as a separate concept. Include an instruction: "if the folder is empty, offer to seed it with `strategies init` (prepop)." This folds finding #1's wizard prompt half into this track (the wizard-prompt copy change is small and naturally lives alongside the create-strategy change).
  - **Hard save-gate untouched:** `crates/xvision-engine/src/agents/validate.rs` is not edited. The 200-char + placeholder-SHA-256 rule remains load-bearing for direct API / MCP / wallet-plan callers that submit a placeholder prompt.
  - **End-to-end replay test:** a new wizard test replays the operator's 2026-05-21 transcript (request a fibonacci+RSI strategy with Gemini Flash Lite 3.1 as the agent's model) against a freshly seeded folder; the wizard references at least one seeded fibonacci entry rather than narrating "your folder is empty," and the strategy creation succeeds end-to-end without tripping the save-gate.
  - **Grep clean:** zero references to `list_templates`, `template_registry`, `WIZARD_BLANK_TEMPLATE`, or `manifest.template` (`rg --hidden -n 'list_templates|template_registry|WIZARD_BLANK_TEMPLATE|manifest\.template' crates/ frontend/`) outside of (a) the deletion commits, (b) seed content under `strategies_folder/prepop/`, (c) the optional one-release `#[serde(default, skip_serializing)]` shim.
  - **Backward-compat read:** an existing strategy manifest on disk that still carries a `template: "trend_follower"` field loads without error. Add one regression test.
  - **No changes outside listed allowed paths.**
---

# Scope

Eliminate the `templates` concept from xvision. The strategies folder
becomes the only library surface; pipeline-template starter content
that used to live in `crates/xvision-engine/src/agents/templates.rs`
migrates into folder seed entries (via the existing
`strategies_folder::prepop` surface from V2F). Operator stance,
recorded 2026-05-21: "whatever the user has in its strategy folder
will be the context for the user. We can include example templates
in there like we do today."

This is also the resolution to the P0 placeholder deadlock in
`team/intake/2026-05-21-qa-chat-rail-strategy-create-broken.md`
finding #2: removing the placeholder-seeding template path means
the wizard no longer feeds the save-gate a forbidden prompt. The
save-gate itself is untouched; the contradiction is resolved by
removing the bad seed, not by softening the validator.

The defensive fix for finding #3 (don't chain `create_strategy_agent`
against a phantom draft id on `create_strategy` failure) lands in
this same contract because it touches the same `wizard_loop.rs`
create handler.

The wizard-prompt half of finding #1 (don't narrate "empty folder"
when the folder is non-empty; offer prepop when it really is empty)
lands here too because the wizard prompt is being rewritten anyway
to drop template references. The remaining behavioral half of
finding #1 lives in the dependent `wizard-folder-recall-honesty`
track.

# Out of scope

- The hard save-gate at `crates/xvision-engine/src/agents/validate.rs:157-172,324`.
  Not edited. The 200-char + placeholder rule stays load-bearing.
- The eval engine, observability, broker, wallet plan. Not touched.
- Frontend UI changes (route table, components, features). The two
  IA tracks (`strategies-folder-into-view-toggle`,
  `memory-into-agents-section`) own those.
- `chat_messages` insert failures (`chat-messages-insert-failing`).
  Separate parallel track.
- DB migrations. The `manifest.template` field lives in the
  per-strategy on-disk file, not in SQLite — no migration needed.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/templates-elimination status
git -C .worktrees/templates-elimination log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/templates-elimination
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/templates-elimination \
  -b task/templates-elimination origin/main
```

# Notes

This contract reverses two prior wave decisions; record both in the
PR description:

- `wizard-strategy-template-optional` (archived 2026-05-18) said
  "Templates stay where they are; the wizard simply stops *requiring*
  one." Now retired — templates leave the engine entirely.
- `agent-pipeline-template-library-expansion` (#409, archived
  2026-05-21) expanded the in-engine template library. Its content
  is the raw material for this track's prepop seed migration, not
  a competing source of truth.

Append checkpoints / PR links below.
