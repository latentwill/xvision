---
from: conductor
to: agent-cicd-migrate-board
utc: 2026-05-19T00:00:00Z
topic: setup-gaps-found-during-first-run
---

First live run of `tools/agent-conductor/scripts/migrate-board.mjs` against
a fresh Project v2 + `latentwill/xvision` revealed three setup-side gaps.
All three were worked around by hand to get the Phase-1 cohort onto
Project v2 #1 ("agent-cicd"), but each deserves a real fix upstream so
the next consumer (or `agent-cicd-extract-package`) doesn't hit the same
wall.

## Gap 1 — default `Status` field name is reserved

The `.github/projects/agent-cicd-board.md` runbook says to create a
SINGLE_SELECT field named `status` (lowercase). GitHub Projects v2
ships every new Project with a default field named `Status` (capital
S) that:

- cannot be deleted (`gh project field-delete` → `Only custom fields
  can be deleted`)
- cannot be renamed (the `updateProjectV2Field { name }` mutation
  returns success but the name silently stays `Status`)
- cannot have a sibling created with a case-only-different name
  (`createProjectV2Field name=status` → `Name has already been taken`)

So in practice every Project v2 has a field named `Status` (capital S)
and the schema-mandated name `status` (lowercase) is unattainable.

The migrate script builds its field lookup as
`byName[f.name] = …` (case-sensitive), then reads
`project.fields['status']`. The lookup misses; status updates skip.

**Fix proposed:**

```js
// scripts/migrate-board.mjs, getProjectInfo()
byName[f.name.toLowerCase()] = { ... };
```

One-line, no other call sites affected (all consumers use lowercase
keys). Applied locally in `/tmp/xvision-migrate-board` to unblock
this run; needs to land in the canonical script via a PR.

Also: update `.github/projects/agent-cicd-board.md` to call out that
the default `Status` field is what the migration uses; replace its
options (13-state schema enum) via the `updateProjectV2Field`
mutation; don't try to delete or rename it.

The same case-insensitive lookup will be needed in the daemon's
`GhClient` implementation when Phase-1 wires the real concrete
client. The current Phase-1 skeleton has `GhClient` as an interface
only, so the issue surfaces in the migrate script first.

## Gap 2 — labels are not auto-created

`migrate-board.mjs` calls `gh issue create --label track:<slug>
--label lane:<lane>` and aborts if either label is missing on the
repo. On a fresh repo none of `track:*` or `lane:foundation`,
`lane:leaf`, `lane:integration` exist.

The setup runbook does not list label creation as a prerequisite.

**Fix proposed:** the script should either

- Pre-create labels lazily (try `gh issue create`; on
  `not found`-type error, `gh label create` then retry once), or
- Pre-flight the labels at the start of the run (build the set
  from descriptors, ensure each exists, skip those already present).

The second is cleaner and avoids per-issue retries.

Worked around in this run by bulk-creating the labels by hand:

```bash
gh label create 'lane:foundation' -R latentwill/xvision --color 0e8a16
gh label create 'lane:integration' -R latentwill/xvision --color 1d76db
gh label create 'lane:leaf' -R latentwill/xvision --color fbca04
# + one track:<slug> label per parsed track
```

## Gap 3 — `team/board-v2.md` parser silently drops rows

`team/board-v2.md` uses em-dash + middle-dot row separators:

```
- [v2a-driver-tour](contracts/v2a-driver-tour.md) — leaf · ready · independent
```

`scripts/parse-board.mjs` expects ASCII hyphens (`- - -`) per the
format established in `team/board.md`. The em-dash/middle-dot rows
match nothing and are silently filtered out. First-run output:

```
$ jq 'length' /tmp/migrate-plan.json
7         # all from board.md; 0 from board-v2.md
```

The two board-v2 entries (`v2a-driver-tour`, `v2a-in-app-docs`) never
made it onto the Project.

**Fix proposed:** normalize separator characters before parsing
(`s/[—·]/-/g` or accept both in the row regex). Add fixture
coverage at `scripts/fixtures/board-v2-sample.md` so this
regresses loudly.

Worked around in this run by leaving the two v2a entries off the
Project; they can be added by hand or via the migrate script
once parser is fixed and the script is re-run idempotently.

## Snapshot of the working setup

For the next person re-running this:

- Project: v2 #1, owner `latentwill`, title `agent-cicd`.
- Fields present and validated: `Status` (case-folded by patched
  script), `lane`, `review_status`, `deploy_status`, `track`,
  `owner_agent`, `branch`, `worktree`, `intake_doc`, `pr`.
- Issues #307–#313 carry the 7 parsed tracks; statuses set to
  `PR_OPEN` / `READY` / `BACKLOG` per the markdown board.
- `agent-conductor.config.json` at repo root points at project 1.

## Suggested track follow-up

File a single follow-up `agent-cicd-migrate-board-setup-gaps` track
addressing all three fixes plus a fixture-driven regression test. It
should be a leaf, parallel-safe, and small (touched files:
`tools/agent-conductor/scripts/migrate-board.mjs`,
`tools/agent-conductor/scripts/parse-board.mjs`,
`.github/projects/agent-cicd-board.md`, plus tests).
