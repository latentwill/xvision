---
from: llm-providers-6
to: all
topic: claim
created_at: 2026-05-10T21:45:00Z
ack_required: false
---

# `llm-providers-6` track claimed (Phase 5 — UI design lock + migration note)

Session 3 (continuing the Plan #7 thread; Phases 1, 2, 3, and Phase 4
T13–T16 already merged via PRs #14/#16/#20/#22/#25/+#27 in flight) takes
the four doc-only tasks that close the plan. Worktree
`.worktrees/llm-providers-6`, branch
`feature/llm-providers-phase-5-design-lock`.

## Scope (Phase 5 — T17, T18, T19, T20; doc only)

- **T17** — `docs/design/ui-elements.md` §13.1: replace "LLM keys" subsection
  with the full Providers registry section (table + add modal + synthetic
  row marker). Spec lock for the dashboard rebuild.
- **T18** — §4.2.2: split the slot form's `Model` row into `Provider` +
  `Model` (combobox); add cost-cue chip note above the live preview pane
  quoting the BriefingCache rule from spec §3.5.
- **T19** — §5 row action menu: add `Fork with different model →` (focused
  fork — opens Inspector with Trader Provider+Model select pre-focused).
  §2.3.3: append model-fork variant to the lineage cue copy.
- **T20** — `docs/cli-non-surfaced.md`: index entry for `xvn provider …`.
  New `docs/migrations/2026-05-10-providers-config.md`: explains that
  existing configs load unchanged (auto-derived synthetic row), the new
  CLI surface, and the new `xvn ab-compare` arm-spec syntax.

Four commits, one PR — closes Plan #7 entirely.

## Files this track touches

- `docs/design/ui-elements.md` (3 commits — §13.1, §4.2.2, §5/§2.3.3)
- `docs/cli-non-surfaced.md` (one bullet)
- `docs/migrations/2026-05-10-providers-config.md` (new file; new dir)

Zero overlap with currently-open PRs (PR #27 is code-only, no doc files).

## What's left after this

Plan 2a — MCP server + verbs + tool dispatch + polish.
Plan 2d — Dashboard + Wizard.
