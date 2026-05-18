# agent-conductor scripts

Standalone Node scripts that support the agent-conductor control plane.
Each is runnable with `node` directly — no build step. They share
dependencies via `package.json`; install once with `npm install` and
the daemon's own package (set up by a later contract) lives separately
under `tools/agent-conductor/{src,bin}`.

Scripts:

| Script | Purpose |
|---|---|
| `validate-schema.mjs` | Validate `team/schema/board.schema.json` as JSON Schema 2020-12, or `--check-examples` an entire directory of task documents. Landed with `agent-cicd-board-schema`. |
| `parse-board.mjs` | Pure function: parse a markdown board (`team/board.md` or `team/board-v2.md`) into a list of typed task rows. No I/O beyond what the caller passes in. |
| `migrate-board.mjs` | One-time migration: read markdown boards + contract front-matter, validate against the schema, populate GitHub Issues + Project v2 items. Dry-run by default until both `--project` and `--repo` are supplied. |

## migrate-board.mjs — operator workflow

Prerequisites (once):

```bash
gh auth status                                       # logged in
gh auth refresh -s project                           # `project` scope
source .op_env                                       # 1Password env, if used
# Create the Project v2 board following .github/projects/agent-cicd-board.md
# and note its number.
```

Dry-run first:

```bash
node tools/agent-conductor/scripts/migrate-board.mjs \
  --dry-run \
  --board team/board.md \
  --board team/board-v2.md \
  > /tmp/migrate-plan.json
jq 'length' /tmp/migrate-plan.json   # row count
jq '.[0]' /tmp/migrate-plan.json     # spot-check the first entry
```

The dry-run output is an array of task descriptors, each one a valid
`board.schema.json` document (validated in-process before being
emitted). The internal validation aborts the run with a non-zero exit
and a pointed error if any descriptor fails — nothing is printed and
no live mutations happen.

Live run:

```bash
node tools/agent-conductor/scripts/migrate-board.mjs \
  --board team/board.md \
  --board team/board-v2.md \
  --project <N> \
  --repo latentwill/xvision
```

For each parsed row the script:

1. Finds an Issue by the `track:<slug>` label. Creates one (with
   `[<lane>] <track>` title and a contract pointer in the body) if
   absent.
2. Adds the Issue to the Project if not already attached.
3. Sets each Project field (`status`, `lane`, `track`, `branch`,
   `worktree`, `intake_doc`, `review_status`, `deploy_status`,
   `pr`) — but **only when the existing Project value is either
   missing or already matches the markdown.** On disagreement, the
   Project state wins and the script prints a warning to stderr.

The script is **idempotent**: a no-change second run produces zero
Issue creations, zero Project-item adds, and zero field writes.

## Status mapping

Contract front-matter uses the lower-case
`ready / claimed / in-progress / pr-open / needs-rebase / merged /
archived / blocked / deferred / scope-violation` vocabulary. The
schema's enum is the upper-case state-machine names. The migration
applies this map:

| Contract token | Schema enum |
|---|---|
| `ready` | `READY` |
| `claimed` | `CLAIMED` |
| `in-progress` | `CODING` |
| `pr-open` | `PR_OPEN` |
| `needs-rebase` | `CODING` |
| `merged` | `MERGED` |
| `archived` | `ARCHIVED` |
| `blocked` | `BACKLOG` |
| `deferred` | `BACKLOG` |
| `scope-violation` | `BACKLOG` |

Unknown tokens default to `BACKLOG`.

## Rollback

The script never edits `team/board.md`, `team/board-v2.md`, or any
contract file. To roll back a migration:

```bash
# Delete the Project entirely (markdown boards remain authoritative).
gh project delete <N> --owner latentwill

# Or remove the Issues this script created (they all carry track:<slug> labels).
gh issue list -R latentwill/xvision --label "track:" --json number -q '.[].number' \
  | xargs -I{} gh issue close {} -R latentwill/xvision
```

## Testing

```bash
node --test tools/agent-conductor/scripts/parse-board.test.mjs
node --test tools/agent-conductor/scripts/migrate-board.test.mjs
```

Tests use `node:test` (built into Node ≥ 18) and a hand-written fake
client — no network, no real `gh` calls, no `nock`.

## A note on the contract's verification line

The `agent-cicd-migrate-board` contract lists this verification:

```
node tools/agent-conductor/scripts/validate-schema.mjs \
  team/schema/board.schema.json /tmp/migrate-plan.json
```

The current `validate-schema.mjs` has two modes (validate the schema
itself, or `--check-examples <dir>`) and does not accept that exact
two-positional signature. The same guarantee is delivered in-process:
`migrate-board.mjs` runs the dry-run plan through the same ajv-compiled
schema before printing, and `migrate-board.test.mjs` asserts the plan
output validates. If a CLI-level check is wanted later, extending
`validate-schema.mjs` to accept `--schema <s> <doc>` is a small
follow-up.
