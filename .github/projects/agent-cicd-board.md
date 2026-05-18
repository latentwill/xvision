# Agent CI/CD board — GitHub Project v2 setup

Operator runbook for creating the Project v2 board that the
`agent-conductor` daemon reads and writes. One-time setup; no
automation here. Schema source of truth:
`team/schema/board.schema.json`.

## Prerequisites

- `gh auth status` shows you authenticated against `latentwill/xvision`
  with the `project` scope (re-auth with `gh auth refresh -s project`
  if not).
- Source `.op_env` so `gh` and `op` use the expected environment:
  `source .op_env`.

## Step 1 — create the Project

```bash
gh project create --owner latentwill --title "agent-cicd"
```

Note the printed Project number; you will pass it as `--project <N>`
to the migration script and record it in `agent-conductor.config.ts`
as `project.number`.

## Step 2 — create the fields

Per the field-to-Project mapping in `team/schema/README.md`, create
each field as the listed type. For SINGLE_SELECT fields, the options
must match the schema enums exactly (case-sensitive). The native
`Title` field stays as-is; the `Status` field below replaces (or
re-uses) the default `Status` column.

```bash
PROJECT_NUMBER=<from step 1>

# status (replace the default Status column's options if it already exists)
gh project field-create $PROJECT_NUMBER --owner latentwill \
  --name "status" --data-type SINGLE_SELECT \
  --single-select-options "BACKLOG,READY,CLAIMED,CODING,PR_OPEN,REVIEWING,CHANGES_REQUESTED,FIXING,APPROVED,MERGE_READY,MERGED,DEPLOYED,ARCHIVED"

# lane
gh project field-create $PROJECT_NUMBER --owner latentwill \
  --name "lane" --data-type SINGLE_SELECT \
  --single-select-options "foundation,leaf,integration"

# review_status
gh project field-create $PROJECT_NUMBER --owner latentwill \
  --name "review_status" --data-type SINGLE_SELECT \
  --single-select-options "none,requested,blocking,approved"

# deploy_status
gh project field-create $PROJECT_NUMBER --owner latentwill \
  --name "deploy_status" --data-type SINGLE_SELECT \
  --single-select-options "none,queued,building,deployed,failed,rolled_back"

# text fields
for f in track owner_agent branch worktree intake_doc; do
  gh project field-create $PROJECT_NUMBER --owner latentwill \
    --name "$f" --data-type TEXT
done

# number field
gh project field-create $PROJECT_NUMBER --owner latentwill \
  --name "pr" --data-type NUMBER
```

## Step 3 — verify

```bash
gh project field-list $PROJECT_NUMBER --owner latentwill --format json | \
  jq '[.fields[] | {name, dataType, options: (.options // [] | map(.name))}]'
```

Compare the output against `team/schema/README.md`'s mapping table.
Every required field present, every single-select's options matching
the schema enums verbatim.

## Step 4 — record the Project number

Add to `agent-conductor.config.ts` at the repo root (created in a
later contract):

```typescript
project: { owner: 'latentwill', number: <PROJECT_NUMBER> },
```

Until that config exists, the migration script (`migrate-board.mjs`,
landed in a separate contract) accepts `--project <N>` on the CLI.

## Rollback

Delete the Project entirely if setup goes wrong; the markdown boards
(`team/board.md`, `team/board-v2.md`) remain authoritative and are
untouched by setup. `gh project delete $PROJECT_NUMBER --owner latentwill`.

## Modular notes

This runbook is xvision-specific (`--owner latentwill`, board
filenames). When the daemon extracts to a standalone npm package, an
analogous `init` scaffolder will generate a per-repo version of this
file from the consuming repo's `agent-conductor.config.ts`.
