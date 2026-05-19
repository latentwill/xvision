---
track: agent-cicd-migrate-board
lane: integration
wave: agent-cicd-phase-1
worktree: .worktrees/agent-cicd-migrate-board
branch: task/agent-cicd-migrate-board
base: origin/main
status: ready
depends_on:
  - agent-cicd-board-schema  # needs team/schema/board.schema.json committed
blocks:
  - agent-cicd-shadow-run
stacking: none
allowed_paths:
  - tools/agent-conductor/scripts/migrate-board.mjs
  - tools/agent-conductor/scripts/migrate-board.test.mjs
  - tools/agent-conductor/scripts/parse-board.mjs
  - tools/agent-conductor/scripts/parse-board.test.mjs
  - tools/agent-conductor/scripts/package.json
  - tools/agent-conductor/scripts/README.md
  - tools/agent-conductor/scripts/fixtures/board-sample.md
  - tools/agent-conductor/scripts/fixtures/board-v2-sample.md
forbidden_paths:
  - team/board.md
  - team/board-v2.md
  - team/contracts/**
  - team/schema/**
  - crates/**
  - frontend/web/**
  - migrations/**
  - tools/agent-conductor/src/**
  - tools/agent-conductor/bin/**
interfaces_used:
  - team/schema/board.schema.json (validate output before write)
  - GitHub GraphQL API (gh api graphql)
  - team/board.md, team/board-v2.md (read-only)
  - team/contracts/*.md (read-only, for contract pointer + lane)
parallel_safe: true
parallel_conflicts: []
verification:
  - node tools/agent-conductor/scripts/parse-board.test.mjs
  - node tools/agent-conductor/scripts/migrate-board.test.mjs
  - node tools/agent-conductor/scripts/migrate-board.mjs --dry-run --board team/board.md --board team/board-v2.md > /tmp/migrate-plan.json
  - node tools/agent-conductor/scripts/validate-schema.mjs team/schema/board.schema.json /tmp/migrate-plan.json
acceptance:
  - "`tools/agent-conductor/scripts/parse-board.mjs` exports `parseBoard(markdownSource)` that returns a typed list of `{ track, lane, status, contractPath, oneLineSummary, wave }` rows derived from a board markdown file. Handles both `team/board.md` and `team/board-v2.md` formats (the existing `- [track](contracts/...) - lane - status - summary.` shape)."
  - "`tools/agent-conductor/scripts/migrate-board.mjs` is a Node CLI taking `--board <path>` (repeatable), `--dry-run` (print the would-be Project items as JSON), `--project <project-number>` (target Project v2), and `--repo <owner/name>`."
  - "Without `--dry-run`, the script: (a) reads contract front-matter for each parsed row to enrich with `depends_on`, `branch`, `worktree`; (b) creates a GitHub Issue per track if one doesn't already exist (lookup by `track:<slug>` label); (c) attaches the Issue to the Project; (d) sets `status`, `lane`, `track`, `branch`, `worktree`, `intake_doc` fields from the contract."
  - "Idempotent: re-running with no board changes results in zero new Issues, zero field updates, and exit 0. Verified by `migrate-board.test.mjs` running the migration twice against a fixture and asserting the second run is a no-op."
  - "Validation: every Project item the script would create is checked against `team/schema/board.schema.json` before any GraphQL mutation. Schema-invalid rows fail the run with a clear error and a non-zero exit. Tested with a bad fixture."
  - "Conflict handling: if an Issue already exists with `track:<slug>` label but `status` on the Project disagrees with the board markdown, the script does NOT overwrite — it prints a warning to stderr and continues. The migration trusts the existing Project state; the board markdown is only authoritative for new tracks."
  - "Tests run in pure Node (no network) using fixtures under `tools/agent-conductor/scripts/fixtures/`. The GraphQL calls are abstracted behind a small `client` interface so tests inject a fake. No `nock`, no real `gh` calls in tests."
  - "`tools/agent-conductor/scripts/README.md` documents: prerequisites (`gh auth status` OK, Project v2 number known), the dry-run-then-live workflow, and the rollback path (delete the Project items; markdown boards untouched)."
  - "Markdown boards (`team/board.md`, `team/board-v2.md`) are NOT deleted, NOT renamed, NOT edited. Per the spec: keep markdown live until the daemon has run a full week without drift."
---

# Scope

One-time migration script: reads the active markdown boards
(`team/board.md`, `team/board-v2.md`), enriches each row with its
contract front-matter, validates against the v1 schema, and creates
the corresponding Issues + Project v2 items on GitHub. Idempotent so
re-runs are safe during the shadow-mode period.

Implements step 3 of the spec's "Migration / first moves" section.

# Out of scope

- Modifying or deleting the markdown boards. They stay live through
  Phase 1 and 2 per spec.
- Any daemon, polling, or transition automation (that's
  `agent-cicd-daemon-skeleton`).
- Migrating archived waves under `team/archive/`. Archived ≠ active;
  the Project only mirrors the active board.
- Rewriting `parse-board.mjs` to support arbitrary future board
  formats. Two formats, real and committed today.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-cicd-migrate-board status
git -C .worktrees/agent-cicd-migrate-board log --oneline -3 origin/main..HEAD
# Confirm: agent-cicd-board-schema is merged into origin/main first
git -C .worktrees/agent-cicd-migrate-board log origin/main -- team/schema/board.schema.json | head -5
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-cicd-migrate-board -b task/agent-cicd-migrate-board origin/main
```

# Notes

The Project number to target is set by the operator after running the
manual setup from `.github/projects/agent-cicd-board.md`. Don't
hardcode a Project number anywhere in the script — pass via flag.
