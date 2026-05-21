---
track: wizard-folder-recall-honesty
lane: leaf
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/wizard-folder-recall-honesty
branch: task/wizard-folder-recall-honesty
base: origin/task/templates-elimination
status: claimed
depends_on:
  - templates-elimination
blocks: []
stacking: declared:templates-elimination
allowed_paths:
  - crates/xvision-dashboard/prompts/wizard.md
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/tests/wizard_loop.rs
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-observability/**
  - frontend/web/**
interfaces_used:
  - wizard_loop::run_tool (list_strategies_folder, list_strategy_ideas)
  - wizard_loop::WizardEvent (test harness)
parallel_safe: false
parallel_conflicts:
  - "Holds wizard_loop.rs + prompts/wizard.md after templates-elimination merges. Must rebase on origin/main once templates-elimination lands."
verification:
  - cargo test -p xvision-dashboard wizard_loop
  - cargo clippy -p xvision-dashboard -- -D warnings
acceptance:
  - **Empty-folder honesty.** When `list_strategies_folder` and `list_strategy_ideas` both return empty results, the wizard narrates exactly that ("your strategies folder is empty") and offers `xvn strategies init` / equivalent prepop. It does NOT collapse the empty state into "I didn't find anything specific about Fibonacci" — that wording suggests it searched the folder content rather than the operator-visible "folder is genuinely empty" condition.
  - **Non-empty honesty.** When either tool returns ≥1 entry, the wizard's narrative cites the returned entries by their `rel_path` (or `display_name` if the seed exposes one). It must never claim the folder is empty when it is not.
  - **Pattern-matching offer.** When the operator asks for a named pattern (fibonacci, RSI, mean-reversion, trend-follower, breakout, etc.) and the folder is empty, the wizard offers prepop before jumping to a blank draft. The prompt explicitly names this case.
  - **Regression test — non-empty folder.** New test under `crates/xvision-dashboard/tests/wizard_loop.rs`: seed the mock tool driver to return 3 folder entries for `list_strategies_folder`; run a wizard turn that asks "what do I have in my strategies folder"; assert the wizard's narrative event references at least one returned `rel_path`. Test fails on a "folder is empty" string.
  - **Regression test — empty folder, named pattern.** Seed the mock tool driver to return empty for both folder tools; ask "make me a fibonacci+RSI strategy"; assert the wizard's next tool call is the prepop init offer (or asks the operator before scaffolding a blank draft), not `create_strategy`.
  - **Wizard prompt copy is consistent with `templates-elimination`.** The folder-narrative rules added here do not contradict the templates-elimination rewrite of `prompts/wizard.md`.
  - **No changes outside listed allowed paths.**
---

# Scope

Wizard recall honesty about the strategies folder. Operator's
2026-05-21 session: the wizard said "your strategy folder is empty,
and I didn't find any specific Fibonacci-based ideas in the library"
in the same turn that `list_strategies_folder` and
`list_strategy_ideas` returned "completed." Either the folder
genuinely had no entries (in which case the right move is to offer
prepop) or the wizard's prompt collapsed non-empty results into a
"didn't find anything" narrative. Either way, the wizard prompt
needs a rule: narrate the folder honestly; offer prepop when it is
genuinely empty and the operator asked for a named pattern.

This depends on `templates-elimination` because that contract
already rewrites `prompts/wizard.md` to drop template references
and add the "if folder is empty, offer prepop" instruction. This
track layers behavioral tests on top, refines the prompt for the
pattern-matching case, and adds the non-empty regression test.

# Out of scope

- Engine changes. `strategies_folder` and `prepop` modules are not
  touched; this is a wizard-prompt + dashboard-test track.
- Frontend UI. The dashboard chat surface relays whatever the
  wizard narrates.
- New tools on the wizard dispatch. The existing
  `list_strategies_folder` and `list_strategy_ideas` are sufficient.
- The `chat_messages` insert path. Separate track
  (`chat-messages-insert-failing`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/wizard-folder-recall-honesty status
git -C .worktrees/wizard-folder-recall-honesty log --oneline -3 origin/main..HEAD
# Confirm templates-elimination has merged before claiming this track:
git -C .worktrees/wizard-folder-recall-honesty log origin/main --oneline | grep -i templates-elimination
```

If the worktree does not exist (only create after `templates-elimination` lands on origin/main):

```bash
git fetch --prune origin
git worktree add .worktrees/wizard-folder-recall-honesty \
  -b task/wizard-folder-recall-honesty origin/main
```

# Notes

Status starts as `deferred` (waiting on `templates-elimination`).
Move to `ready` when the foundation merges. Conductor reassigns
the status field as part of the merge sweep. `deferred` is the
lint-exempt waiting state; `blocked` would require an existing
worktree which doesn't make sense for a track that shouldn't be
claimed yet.

Append checkpoints / PR links below.
