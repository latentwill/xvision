---
track: wizard-strategy-template-optional
lane: integration
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/wizard-strategy-template-optional
branch: task/wizard-strategy-template-optional
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/src/routes/strategies.rs
  - crates/xvision-dashboard/tests/wizard_loop.rs
  - crates/xvision-dashboard/tests/http.rs
  - crates/xvision-dashboard/prompts/wizard.md
  - crates/xvision-engine/src/authoring/**
  - crates/xvision-engine/src/strategies/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - authoring::list_templates / create_strategy_from_template
  - StrategyDraft (xvision-engine::strategies)
  - wizard_loop::create_strategy_draft tool schema
parallel_safe: false
parallel_conflicts:
  - "wizard-scenario-create-tool-repair: also edits wizard_loop.rs. Coordinate disjoint regions; later claimant rebases."
verification:
  - cargo test -p xvision-dashboard
  - cargo test -p xvision-engine -- authoring strategies
  - cargo clippy -p xvision-dashboard -- -D warnings
acceptance:
  - The wizard `create_strategy_draft` tool no longer lists
    `template` in `required`. Passing `template: null` or omitting
    the field produces a blank/minimal strategy draft with the same
    shape a no-op template would produce.
  - Underlying `authoring::create_strategy_from_template(None)` (or
    a new `create_blank_strategy()` helper, worker's call) returns
    a valid `StrategyDraft` with empty agents / no mechanical_params
    / sensible defaults. The downstream `set_*` tools (already
    existing) can fill it in.
  - The existing template-named path (`create_strategy_draft({
    template: "trend_follower", name: "X" })`) continues to work
    unchanged — this is purely a requirement relaxation.
  - The wizard system prompt (`prompts/wizard.md`) is updated to
    describe templates as **reference examples** the agent can read
    via `list_templates`, NOT as a prerequisite to create. Reword
    the line at `wizard_loop.rs:76` (`"... ensure it has a [template]"`)
    accordingly.
  - Backend unit test: `create_blank_strategy` (or
    `create_strategy_from_template(None)`) produces a draft whose
    serde-round-trip equals the same shape produced by an empty
    template stub.
  - Wizard integration test: feeding `create_strategy_draft({
    name: "Blank Run" })` (no template) returns a valid `{ id }`
    and downstream `set_agent` / `set_mechanical_param` calls work
    on it.
  - The operator-visible wizard error message
    ("the API does require a template to create a strategy") no
    longer appears in any code path. Grep confirms.
---

# Scope

Operator (2026-05-18): the wizard told them "the API does require a
template to create a strategy" — confirming the schema gate at
`wizard_loop.rs:1440`. Templates should be **reference examples**
the agent can browse with `list_templates` when it wants a starting
shape, NOT a hard requirement to create.

Two halves:

1. **Tool schema relaxation** — drop `template` from `required`,
   accept null / omitted, default to blank.
2. **Authoring path** — provide a blank-draft entry point so the
   wizard's no-template call doesn't fall into an "unknown template"
   error inside `authoring::`.

System prompt copy update to match.

# Out of scope

- Removing or renaming any existing template. Templates stay where
  they are; the wizard simply stops *requiring* one.
- Restructuring the StrategyDraft shape.
- Frontend chat rail copy / surface changes (the rail just relays
  the wizard's tool errors, which will stop firing once the
  schema is relaxed).
- Multi-template composition / partial inheritance. V2.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/wizard-strategy-template-optional status
git -C .worktrees/wizard-strategy-template-optional log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/wizard-strategy-template-optional \
  -b task/wizard-strategy-template-optional origin/main
```

# Notes

Coordinate with `wizard-scenario-create-tool-repair` (also edits
`wizard_loop.rs`) via team/queue/. Disjoint regions of the file —
this contract owns `create_strategy_draft` tool schema (~line 1432-1442)
and the system prompt block (~line 76). Scenario repair owns
`normalize_create_scenario_input` (~line 1112) and the `create_scenario`
tool schema (~line 1453-1480). Single-writer overlap on the file
itself but no overlap on the regions.

Append checkpoints / PR links below.
