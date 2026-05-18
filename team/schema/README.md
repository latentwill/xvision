# Board schema (v1)

`board.schema.json` is the source of truth for the agent-conductor task
object. It is consumed by the migration script (`migrate-board.mjs`),
the daemon (validation on every poll), and any external client that
reads the Project v2 board.

Schema dialect: JSON Schema 2020-12. Schema `$id`:
`https://xvision.local/schemas/board/v1.json`.

Examples live under `examples/`: `backlog-task.json`,
`coding-task.json`, `archived-task.json`. The validator
(`tools/agent-conductor/scripts/validate-schema.mjs`) runs in two
modes:

```bash
node tools/agent-conductor/scripts/validate-schema.mjs team/schema/board.schema.json
node tools/agent-conductor/scripts/validate-schema.mjs --check-examples team/schema/examples/
```

## Field → GitHub Project v2 mapping

GitHub Project v2 supports a fixed set of field types: `TEXT`,
`NUMBER`, `DATE`, `SINGLE_SELECT`, `ITERATION`. This table records the
type each schema field MUST be created as in the GH Project board.
Operators creating a new Project (see
`.github/projects/agent-cicd-board.md`) follow this table verbatim.

| Schema field | Required | GH Project field type | Single-select options |
|---|---|---|---|
| `status` | yes | SINGLE_SELECT | `BACKLOG`, `READY`, `CLAIMED`, `CODING`, `PR_OPEN`, `REVIEWING`, `CHANGES_REQUESTED`, `FIXING`, `APPROVED`, `MERGE_READY`, `MERGED`, `DEPLOYED`, `ARCHIVED` |
| `lane` | yes | SINGLE_SELECT | `foundation`, `leaf`, `integration` |
| `track` | yes | TEXT | — |
| `owner_agent` | no | TEXT | — |
| `branch` | no | TEXT | — |
| `worktree` | no | TEXT | — |
| `pr` | no | NUMBER | — |
| `review_status` | no | SINGLE_SELECT | `none`, `requested`, `blocking`, `approved` |
| `deploy_status` | no | SINGLE_SELECT | `none`, `queued`, `building`, `deployed`, `failed`, `rolled_back` |
| `intake_doc` | no | TEXT | — |
| `created_at` | derived | (GH-native: Issue `createdAt`) | — |
| `updated_at` | derived | (GH-native: Issue `updatedAt`) | — |

Notes:

- `track`, `branch`, `worktree`, `intake_doc`, `owner_agent` are TEXT
  because they are user-defined strings, not closed enums. The daemon
  validates conventions (e.g. `branch` starts with the configured
  prefix) at write time, not via schema.
- `created_at` and `updated_at` mirror the GitHub Issue's native
  timestamps; the daemon does not create Project fields for them.
- Single-select option ordering in the Project UI follows the table
  order above. Daemon writes use the option label exactly as listed —
  case-sensitive.

## Versioning

Major bumps reserved for breaking field or enum changes. The current
schema is `v1`. Future versions live alongside (`v2.json`, etc.); the
daemon's `instance.config_version` records which version a given
control plane targets.
