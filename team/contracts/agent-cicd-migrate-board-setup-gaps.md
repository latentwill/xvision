---
track: agent-cicd-migrate-board-setup-gaps
lane: leaf
wave: agent-cicd-phase-1
worktree: .worktrees/agent-cicd-migrate-board-setup-gaps
branch: task/agent-cicd-migrate-board-setup-gaps
base: origin/main
status: merged
depends_on: []
blocks:
  - agent-cicd-live-flip  # the live shadow run + plist install cannot start with these gaps open
stacking: none
allowed_paths:
  - tools/agent-conductor/scripts/migrate-board.mjs
  - tools/agent-conductor/scripts/parse-board.mjs
  - tools/agent-conductor/scripts/migrate-board.test.mjs
  - tools/agent-conductor/scripts/parse-board.test.mjs
  - tools/agent-conductor/scripts/fixtures/**
  - .github/projects/agent-cicd-board.md
forbidden_paths:
  - tools/agent-conductor/src/**
  - tools/agent-conductor/test/**
  - tools/agent-conductor/docs/**
  - tools/agent-conductor/bin/**
  - team/board.md
  - team/board-v2.md
  - team/contracts/**
  - team/schema/**
  - crates/**
  - frontend/web/**
  - migrations/**
interfaces_used:
  - team/schema/board.schema.json (parser output must remain schema-valid)
  - .github/projects/agent-cicd-board.md (setup runbook this contract documents)
parallel_safe: true
parallel_conflicts: []
verification:
  - (cd tools/agent-conductor && node --test scripts/migrate-board.test.mjs scripts/parse-board.test.mjs)
  - node tools/agent-conductor/scripts/parse-board.mjs --board team/board-v2.md | jq 'length' # must be >= 2 (em-dash rows now parse)
  - node tools/agent-conductor/scripts/migrate-board.mjs --dry-run --board team/board.md --board team/board-v2.md > /tmp/plan.json && jq 'length' /tmp/plan.json
acceptance:
  - "Gap 1 (case-insensitive Status field lookup): scripts/migrate-board.mjs stores Project fields under lowercase keys (`byName[f.name.toLowerCase()]`). Setup runbook (.github/projects/agent-cicd-board.md) documents that GitHub's default `Status` field cannot be deleted or renamed and that the migration relies on its options being replaced via the `updateProjectV2Field` mutation. A unit test fixture covers the case where the live field name is `Status` and the descriptor key is `status` — must produce a successful field-update plan."
  - "Gap 2 (auto-create labels): scripts/migrate-board.mjs pre-flights the label set before creating issues. For each unique `track:<slug>` and `lane:<lane>` label in the parsed descriptors, the script ensures the label exists via `gh label create --force` (or `gh label list` + create-on-missing). Label colors match the workaround set used in the first run (foundation=0e8a16, integration=1d76db, leaf=fbca04, track=5319e7). A unit test verifies the pre-flight runs before any `gh issue create` call."
  - "Gap 3 (em-dash row parser): scripts/parse-board.mjs accepts rows that use em-dash + middle-dot separators (`— · ·`) in addition to ASCII hyphens (`- - -`). A fixture at scripts/fixtures/board-v2-sample.md covers the case; the existing board-sample.md fixture must continue to parse as before. Running `node scripts/parse-board.mjs --board team/board-v2.md` returns the two v2a rows, not zero."
  - "End-to-end smoke: running the dry-run migration against the current `team/board.md` + `team/board-v2.md` returns 9 descriptors (7 from board.md + 2 from board-v2.md), all validating against `team/schema/board.schema.json`. Re-running the live migration against the existing Project v2 #1 is idempotent — no duplicate issues created, status field updates land correctly under the lowercase-key path."
  - "Setup runbook update: `.github/projects/agent-cicd-board.md` adds a `## Step 2.5 — Replace default Status options` section documenting the `updateProjectV2Field { singleSelectOptions }` mutation as the way to bring the default Status field in line with the schema enum. Step 4 (record Project number) gains a note that the `pr` NUMBER field requires no extra setup."
parallel_with_conductor: true
---

# Scope

Three migration-side fixes uncovered during the first live run of
`tools/agent-conductor/scripts/migrate-board.mjs` against a fresh
Project v2 board on 2026-05-19. All three were worked around by
hand to get the Phase-1 cohort onto Project v2 #1; this contract
lands the canonical fixes so the next consumer (and the eventual
extract-package consumer) doesn't hit the same wall.

Background: `team/queue/agent-cicd-migrate-board__20260519T000000Z__setup-gaps-found-during-first-run.md`

## Gap 1 — default `Status` field name is reserved

GitHub Projects v2 ships every new Project with a default field
named `Status` (capital S) that cannot be deleted, cannot be
renamed (the `updateProjectV2Field { name }` mutation echoes
success but the name stays `Status`), and cannot be shadowed by a
sibling `status` field (`createProjectV2Field` returns "Name has
already been taken").

The migration script's `getProjectInfo()` builds
`byName[f.name] = …` (case-sensitive), then reads
`project.fields['status']` (lowercase). The lookup misses; all
status field updates skip with `field missing on Project`.

**Fix:** lower-case the key on insert. Already proven correct as
a one-liner in the temp worktree used for the first run.

## Gap 2 — labels are not auto-created

`migrate-board.mjs` calls `gh issue create --label track:<slug>
--label lane:<lane>` and aborts if either label is missing on the
target repo. On a fresh repo none of `track:*` / `lane:foundation`
/ `lane:leaf` / `lane:integration` exist. The setup runbook does
not list label creation as a prerequisite, and the migration
script does not pre-create.

**Fix:** pre-flight the label set at the start of the run. Build
the unique set of `track:<slug>` + `lane:<lane>` labels from the
parsed descriptors; for each one, call `gh label list` to check
existence and `gh label create --force` if missing. Run this once,
before any `gh issue create`. Worked around in the first run by
hand-creating 10 labels.

## Gap 3 — `team/board-v2.md` parser silently drops rows

`team/board-v2.md` uses em-dash + middle-dot row separators
(`— · ·`) instead of ASCII hyphens (`- - -`). The parser's
regex assumes hyphens; em-dash rows match nothing and are
silently dropped. First-run dry-run output: 7 rows (5 board.md +
0 board-v2.md), not 9.

**Fix:** normalize separator characters before regex matching, or
expand the regex to accept either form. Fixture coverage at
`scripts/fixtures/board-v2-sample.md` so the next regression
fails loudly.

## Out of scope

- The eventual concrete `GhClient` implementation under
  `tools/agent-conductor/src/` (Phase-1 daemon-skeleton only
  ships interfaces). When that lands it will also need the
  case-insensitive lookup; that's a separate contract under
  the daemon's `src/`, not this scripts-only one.
- Schema or board content changes — this is a tooling fix, not
  a board change.
- Phase-2/3 work (review routing, deploy dispatch).

## Verification

See `verification:` block in frontmatter. The end-to-end check
re-runs the dry-run against the live boards and asserts the
descriptor count is 9 (not 7).

## Acceptance

See `acceptance:` block in frontmatter. The four items track 1:1
with the three gaps + the setup-runbook update.
