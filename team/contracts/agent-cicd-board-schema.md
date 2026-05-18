---
track: agent-cicd-board-schema
lane: foundation
wave: agent-cicd-phase-1
worktree: .worktrees/agent-cicd-board-schema
branch: task/agent-cicd-board-schema
base: origin/main
status: ready
depends_on: []
blocks:
  - agent-cicd-migrate-board
  - agent-cicd-daemon-skeleton
stacking: none
allowed_paths:
  - team/schema/board.schema.json
  - team/schema/README.md
  - team/schema/examples/*.json
  - tools/agent-conductor/scripts/validate-schema.mjs
  - .github/projects/agent-cicd-board.md
forbidden_paths:
  - team/board.md
  - team/board-v2.md
  - team/contracts/**
  - crates/**
  - frontend/web/**
  - migrations/**
interfaces_used:
  - GitHub Projects v2 GraphQL (read-only at this stage)
parallel_safe: true
parallel_conflicts: []
verification:
  - node tools/agent-conductor/scripts/validate-schema.mjs team/schema/board.schema.json
  - node tools/agent-conductor/scripts/validate-schema.mjs --check-examples team/schema/examples/
  - bash scripts/board-lint.sh
acceptance:
  - "`team/schema/board.schema.json` exists, is a valid JSON Schema 2020-12 document, and defines every field in the spec's task-object table: `status`, `lane`, `track`, `owner_agent`, `branch`, `worktree`, `pr`, `review_status`, `deploy_status`, `intake_doc`, `created_at`, `updated_at`."
  - "`status` enum is exactly the spec state machine: `BACKLOG`, `READY`, `CLAIMED`, `CODING`, `PR_OPEN`, `REVIEWING`, `CHANGES_REQUESTED`, `FIXING`, `APPROVED`, `MERGE_READY`, `MERGED`, `DEPLOYED`, `ARCHIVED`. No extras."
  - "`lane` enum is exactly `foundation`, `leaf`, `integration`."
  - "`review_status` enum is exactly `none`, `requested`, `blocking`, `approved`."
  - "`deploy_status` enum is exactly `none`, `queued`, `building`, `deployed`, `failed`, `rolled_back`."
  - "Required fields per the spec: `status`, `lane`, `track`. All others optional/nullable."
  - "Three example task documents under `team/schema/examples/`: a fresh `BACKLOG` task, an in-flight `CODING` task with a PR, a terminal `ARCHIVED` task. Each example validates against the schema."
  - "`team/schema/README.md` documents the schema's field-by-field mapping to GitHub Project v2 fields (which are single-select vs text vs number vs date). One table, no narrative."
  - "`.github/projects/agent-cicd-board.md` records the manual setup steps for the Project v2 board (which fields to create, which options per single-select), in operator-runnable form. This is documentation only — no automation in this contract."
  - "`tools/agent-conductor/scripts/validate-schema.mjs` is a tiny ajv-based validator runnable with `node` (no build step). Uses `ajv` + `ajv-formats` pinned via inline `npm install --no-save` in the script header, or a single `package.json` in `tools/agent-conductor/scripts/`. Whichever path is chosen, no Rust workspace impact."
  - "No other files touched. No daemon code yet. No migration script yet. No changes to `team/board.md` or `team/board-v2.md`."
---

# Scope

Defines the machine-readable task object for the agent-conductor control
plane and records the GitHub Project v2 setup steps. This is the
foundation contract for Phase 1 of
`docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md` —
nothing else can start until the schema is committed.

Two artifacts:

1. **JSON Schema** at `team/schema/board.schema.json` — the source of
   truth for task field types and enums. Any future tooling
   (daemon, migration script, audits) validates against this.
2. **Project setup doc** at `.github/projects/agent-cicd-board.md` —
   step-by-step operator instructions for creating the Project v2
   board with the right fields and options. Manual, one-time. No
   automation in this contract.

The validator script is small and exists only to support `verification`
and future contracts; it is not the daemon.

# Out of scope

- Creating the actual GH Project board (operator does that manually
  using the setup doc; tracked outside the contract system).
- Migrating any existing `team/board.md` content into the Project
  (that's `agent-cicd-migrate-board`).
- Any daemon, polling, or transition logic (that's
  `agent-cicd-daemon-skeleton`).
- Touching the live boards `team/board.md` / `team/board-v2.md`.
- Changing the `team/contracts/_template.md` format. The Project board
  is a parallel surface; contracts stay where they are.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-cicd-board-schema status
git -C .worktrees/agent-cicd-board-schema log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-cicd-board-schema -b task/agent-cicd-board-schema origin/main
```

# Notes

Schema version `$id` should be
`https://xvision.local/schemas/board/v1.json` so we can bump cleanly later.
JSON Schema 2020-12 dialect.
