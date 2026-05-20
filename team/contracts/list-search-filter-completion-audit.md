---
track: list-search-filter-completion-audit
lane: foundation                                                # produces the migration checklist for `list-search-filter-missing-surfaces`
wave: docs-lists-metric-polish-2026-05-21
worktree: .worktrees/list-search-filter-completion-audit
branch: task/list-search-filter-completion-audit
base: origin/main
status: ready
depends_on: []
blocks:
  - list-search-filter-missing-surfaces                         # the migration track consumes this audit's output
stacking: none
allowed_paths:
  - docs/superpowers/audits/2026-05-21-list-surfaces-audit.md   # NEW — the deliverable
forbidden_paths:
  - frontend/web/src/**                                         # audit only; no code changes
  - crates/**                                                   # audit only
  - team/contracts/**                                           # don't open follow-up contracts; that's the conductor's call
  - team/board.md                                               # don't update active board; conductor
  - team/OWNERSHIP.md                                           # don't update ownership; conductor
interfaces_used:
  - "@/components/lists"                                        # ResponsiveListCard, useListState, useListUrlState — what the audit looks for
  - "@/components/primitives/ListPagination"                    # useServerPagination — the legacy paginated pattern
parallel_safe: true                                             # audit writes one new file; no overlap
parallel_conflicts: []
verification:
  - test -f docs/superpowers/audits/2026-05-21-list-surfaces-audit.md
  - grep -q "## Surfaces" docs/superpowers/audits/2026-05-21-list-surfaces-audit.md
acceptance:
  - **Deliverable lands at `docs/superpowers/audits/2026-05-21-list-surfaces-audit.md`.** Single markdown file. Frontmatter or top-of-file should name the audit, date, author, and which intake it traces back to (`team/intake/2026-05-21-docs-lists-metric-polish.md`).
  - **Inventory table.** One row per list-like surface in the dashboard SPA, capturing: route or component path, what the list contains, current primitive (`<ResponsiveListCard>` / `<Card>` + inline JSX / bespoke / `<MList...>`), search (yes/no/partial), filters (yes/no/partial — name them if partial), sort (yes/no — default-key noted), URL state (`useListUrlState` adoption), mobile parity (mobile component path or "uses desktop"), owning contract (if any), and **migration decision** (Migrate via `list-search-filter-missing-surfaces`; Not-a-list — explain; Already-migrated — link to merged PR; Deferred — explain).
  - **Mandatory surfaces.** The audit covers at minimum: `/eval-runs`, `/strategies`, `/scenarios`, `/agents`, `/eval-runs/compare`, agent-runs / trace lists (whatever lives under `frontend/web/src/routes/agent-runs*` or `frontend/web/src/features/agent-runs/`), decisions / trade ledger / open positions / journal **if any of those exist or are planned**, home/control-tower mini-lists, docs navigation, settings sub-lists, providers/brokers configuration lists, the chat-rail run/strategy/scenario lists, and any inspector "Sub-tables" (e.g. decisions table inside eval-runs-detail). For each "if it exists" surface, confirm yes/no and link the path or say "no such surface today".
  - **Overlap with active phase-2 contracts noted.** Rows whose migration is already owned by `list-migrate-eval-runs` (merged #399), `list-migrate-strategies` (merged #400), or `list-migrate-decisions-and-tail` (still active) are flagged as covered. The audit does not re-spec their migrations.
  - **Recommendation section.** A short closing section that groups remaining surfaces by suggested track size (one PR vs needs-its-own-contract) for the conductor's next decomposition pass. The conductor will turn the audit into one or more `list-search-filter-missing-surfaces` PRs; the audit doesn't open them itself.
  - **No code changes.** This contract's `allowed_paths` is exactly the one markdown file. If the worker finds a surface that's broken (e.g. a list that crashes), document it as a future track in the audit recommendation section.

---

# Scope

Track #2 of `team/intake/2026-05-21-docs-lists-metric-polish.md`. The
phase-2 list migrations were scoped to four high-traffic routes
(`/eval-runs`, `/strategies`, `/scenarios`, `/agents`). The operator's
expectation is stronger: **every** list-like surface in the dashboard
should have search/filter/sort. Before opening more migration tracks,
we need a single audit document so we know what "missing" actually
covers and so workers don't duplicate effort.

Foundation lane — this track blocks `list-search-filter-missing-surfaces`
because the migration track consumes this audit's checklist.

# Out of scope

- Writing migration code. That's `list-search-filter-missing-surfaces`.
- Re-spec'ing the four routes already on the phase-2 list migration
  contracts. Note them as covered, move on.
- Building or designing a second list system. Per intake §"Out of
  scope": "Creating a second list component."
- Updating any team/ file. Conductor owns those. The audit lives in
  `docs/superpowers/audits/`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-search-filter-completion-audit status
git -C .worktrees/list-search-filter-completion-audit log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-search-filter-completion-audit -b task/list-search-filter-completion-audit origin/main
```

# Notes

Recommended starting query to find list-like surfaces (broad grep):

```bash
grep -rn 'map(\|\.map(\|<table\|<Card' frontend/web/src/routes/ frontend/web/src/features/ frontend/web/src/components/chat/ | grep -i 'list\|row\|item' | head -50
```

Then narrow with `grep -l 'ResponsiveListCard\|useListState\|ListPagination'`
to identify already-migrated vs not.

The output doc is in `docs/superpowers/audits/` (a new subfolder is
fine if it doesn't exist) to keep it adjacent to specs/plans and out
of `team/`. Conductor will reference it from
`team/board.md` when opening `list-search-filter-missing-surfaces`.
