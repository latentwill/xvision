---
track: wizard-prompt-strategy-folder-and-templates
contract: team/contracts/wizard-prompt-strategy-folder-and-templates.md
status: ready-for-review
owner: claude-opus-4-7
claimed_at: 2026-05-21
worktree: .worktrees/wizard-prompt-strategy-folder-and-templates
branch: task/wizard-prompt-strategy-folder-and-templates
---

# Status

## 2026-05-21 — implementation complete

Single-file edit to `crates/xvision-dashboard/prompts/wizard.md`.

### Changes

- **Tool list refresh** — added `list_strategies_folder`,
  `read_strategies_file`, and `list_strategy_ideas` to the `## Your
  tools` section with usage guidance. The latter two reference tools
  whose registration ships in later V2F tracks; the wizard runtime
  gracefully ignores unknown tool descriptions, and the contract
  explicitly allows naming them now.
- **New section: `## Strategies folder`** — describes the
  `$XVN_HOME/strategies/` layout with the five subfolders (`notes/`,
  `docs/`, `strategy-files/`, `evals/`, `library/`), when to consult
  each, and that the folder is a reference for inspiration, not a
  prerequisite.
- **Expanded template library** — `create_strategy` description now
  refers to "the expanded set of starter templates under `/agents/new`"
  rather than hardcoding a count, since track 3 (template-library
  expansion) lands separately and the final count is 7–9.
- **Closes loop on #275** — explicit "templates and strategies-folder
  contents are optional references" language added in three places:
  the `list_templates` description, the new strategies-folder section,
  and a new hard rule ("Never tell the user they must pick a
  template").
- **No-popups compliance** — added explicit style rule that
  confirmations live inline in the chat, not in modals/popups/dialogs.
- **Slot-name clarification** — touched up the `update_slot` blurb
  to reflect the 2026-05-12 terminology that slot names are
  user-defined free text (intern/regime/trader are conventions).
- **Tool-error guidance** — added a tail to the "tool errors" rule
  saying that empty / unavailable strategies-folder tool responses
  are non-events, since the folder is optional.

### Search-and-replace audit

Searched the new prompt for prerequisite language ("must pick", "must
select", "required template"). None present. The relaxation language
from #275 is preserved and reinforced.

### Verification

```
cargo test -p xvision-dashboard
```

Result: **282 passed, 0 failed**. The prompt is `include_str!`d into
`wizard_loop.rs`; no snapshot test asserts on its content, so no
snapshot delta was needed.

### Out-of-scope (untouched)

- `crates/xvision-dashboard/src/wizard_loop.rs` (track 1 owns tool
  registration).
- `crates/xvision-engine/src/agents/templates.rs` (track 3 owns
  template additions).
- `crates/xvision-engine/src/strategies_folder/**` (track 1).
- Frontend, CLI, migrations.
