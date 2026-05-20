---
track: wizard-prompt-strategy-folder-and-templates
lane: leaf
wave: v2f
worktree: .worktrees/wizard-prompt-strategy-folder-and-templates
branch: task/wizard-prompt-strategy-folder-and-templates
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/prompts/wizard.md
  - crates/xvision-dashboard/tests/wizard_prompt_snapshot.rs   # if a prompt snapshot test exists
forbidden_paths:
  - crates/xvision-dashboard/src/wizard_loop.rs                # owned by strategies-folder-surface (tool registration)
  - crates/xvision-engine/src/strategies_folder/**             # owned by track 1
  - crates/xvision-engine/src/agents/templates.rs              # owned by track 3
  - frontend/web/**
  - crates/xvision-cli/**
interfaces_used:
  - none (markdown prompt edit)
parallel_safe: true
parallel_conflicts:
  - strategies-folder-surface
  - agent-pipeline-template-library-expansion
verification:
  - cargo test -p xvision-dashboard
  - bash scripts/board-lint.sh
acceptance:
  - **`crates/xvision-dashboard/prompts/wizard.md` updated** to (a) describe the strategies folder + when to consult it, (b) describe the expanded template library (refers to it as 7–9 templates, exact count matches the post-track-3 reality at merge time), (c) explicitly state that templates AND the strategies folder are *references for inspiration*, not prerequisites — closes the loop on #275.
  - **New tool surface called out by name** — the prompt names `list_strategies_folder`, `read_strategies_file`, and `list_strategy_ideas` (the latter even if track 4 hasn't merged yet — prompt-side discovery is allowed to reference a tool whose registration lands later, since the wizard runtime gracefully ignores unknown tool descriptions).
  - **No prerequisite language remains** — search for phrases like "you must pick a template", "the user must select a template", "required template". None should remain. The relaxation language from #275 stays.
  - **No popups language added** — the prompt should not instruct the agent to use modal confirmations or popovers (cross-check with the CLAUDE.md no-popups rule).
  - **Existing wizard test snapshots updated** if they assert on prompt content. The prompt is a build-time string in `wizard_loop.rs`; touching it changes the wizard turn snapshots. Rerun snapshot tests and commit deltas.
  - **`cargo test -p xvision-dashboard` clean**.
  - **No code changes** outside the listed allowed paths.

---

# Scope

Refresh the wizard system prompt at
`crates/xvision-dashboard/prompts/wizard.md` to describe the new
V2F surfaces (strategies folder + new tools + expanded template
library) and to explicitly reinforce — closing the loop on
`wizard-strategy-template-optional` (#275) — that templates and the
strategies folder are references, not prerequisites.

Spec: `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.

# Out of scope

- Registering tools on `wizard_loop.rs` (track 1).
- Adding agent-pipeline templates (track 3).
- Building the strategies folder (tracks 1, 2, 4, 6).
- Frontend tour or onboarding copy.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/wizard-prompt-strategy-folder-and-templates status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/wizard-prompt-strategy-folder-and-templates -b task/wizard-prompt-strategy-folder-and-templates origin/main
```

# Notes

Small track — a single markdown edit plus possibly a snapshot test
delta. Lands quickly. Coordinate with track 1 (which registers the
new tools) and track 3 (which adds the new templates) so the merge
ordering is sensible: ideally this track lands LAST in wave 1 so the
prompt reflects the actual tool list and template count, but it's
not strictly blocking.

If track 1 hasn't merged when this lands, the wizard runtime
gracefully handles tool names mentioned in the prompt but not yet
registered — the agent will simply not call them. Leave the
reference in; the registration catches up.
