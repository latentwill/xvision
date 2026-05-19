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
`Title` field stays as-is. The default `Status` field is special —
see Step 2.5 below.

```bash
PROJECT_NUMBER=<from step 1>

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

## Step 2.5 — replace the default `Status` field options

GitHub Projects v2 ships every new Project with a default
SINGLE_SELECT field literally named `Status` (capital S). It
**cannot** be deleted (`gh project field-delete` returns
"Only custom fields can be deleted"), **cannot** be renamed (the
`updateProjectV2Field { name }` mutation echoes success but the
name silently stays `Status`), and **cannot** be shadowed by a
sibling field named `status` (`gh project field-create` returns
"Name has already been taken").

So instead of creating a fresh `status` field, the migration
re-uses the default `Status` and replaces its options with the
13-state schema enum via the raw GraphQL mutation
`updateProjectV2Field { singleSelectOptions }`:

```bash
# Find the default Status field id.
FIELD_ID=$(gh project field-list $PROJECT_NUMBER --owner latentwill \
  --format json | \
  jq -r '.fields[] | select(.name == "Status") | .id')

# Build the options payload separately so the JSON survives shell
# quoting and matches the schema enum verbatim.
cat > /tmp/update-status.json <<EOF
{
  "query": "mutation(\$fieldId: ID!, \$opts: [ProjectV2SingleSelectFieldOptionInput!]!) { updateProjectV2Field(input: { fieldId: \$fieldId, singleSelectOptions: \$opts }) { projectV2Field { ... on ProjectV2SingleSelectField { name options { name } } } } }",
  "variables": {
    "fieldId": "$FIELD_ID",
    "opts": [
      {"name":"BACKLOG","color":"GRAY","description":""},
      {"name":"READY","color":"BLUE","description":""},
      {"name":"CLAIMED","color":"BLUE","description":""},
      {"name":"CODING","color":"YELLOW","description":""},
      {"name":"PR_OPEN","color":"YELLOW","description":""},
      {"name":"REVIEWING","color":"YELLOW","description":""},
      {"name":"CHANGES_REQUESTED","color":"ORANGE","description":""},
      {"name":"FIXING","color":"ORANGE","description":""},
      {"name":"APPROVED","color":"GREEN","description":""},
      {"name":"MERGE_READY","color":"GREEN","description":""},
      {"name":"MERGED","color":"PURPLE","description":""},
      {"name":"DEPLOYED","color":"PURPLE","description":""},
      {"name":"ARCHIVED","color":"GRAY","description":""}
    ]
  }
}
EOF
gh api graphql --input /tmp/update-status.json | \
  jq '.data.updateProjectV2Field.projectV2Field.options'
```

The migration script `scripts/migrate-board.mjs` lower-cases all
project field keys on load, so a descriptor's
`status: 'READY'` resolves against the capital-S `Status` field
without further configuration on either side.

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
