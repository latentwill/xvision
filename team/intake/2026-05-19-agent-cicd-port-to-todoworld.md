# Agent CI/CD — Phase-1 state snapshot + port-to-todoworld handoff

> Self-contained handoff for porting the agent-cicd control plane out
> of xvision into the **todoworld** repo. Written 2026-05-19 after the
> four Phase-1 contracts merged. Read this first; everything else is
> source-of-truth links from here.

## TL;DR

xvision's `tools/agent-conductor/` is a host-repo-agnostic Phase-1
control plane. It claims `READY` tasks from a GitHub Project v2
board, manages worktrees, spawns Claude Code workers, detects opened
PRs, and archives merged work. **All four Phase-1 contracts have
merged.** Three things still gate "live mode" anywhere:

1. **Concrete `GhClient` implementation.** Phase-1 daemon-skeleton
   ships *interfaces* (`GhClient` for claim, `ArchiveGhClient` for
   archive) — no concrete client that actually talks to GitHub. The
   migrate-board script has a real `gh`-CLI client; the daemon does
   not.
2. **Operator-machine actions.** `launchd` (macOS) / `systemd`
   (linux) install of the daemon, then unset `AGENT_CONDUCTOR_SHADOW`.
3. **The package is not yet extracted.** Living under
   `tools/agent-conductor/` inside xvision; the
   `agent-cicd-extract-package` contract is `status: deferred`
   pending Phase-2 review-routing.

For todoworld: you can either (a) **copy the daemon over** as
`tools/agent-conductor/` in todoworld and point it at todoworld's
own Project v2, or (b) **wait for the extract** and `npx
agent-conductor init` once Phase-2 ships. (a) is faster; (b) is
correct long-term.

## Repo layout (xvision today)

```
xvision/
├── agent-conductor.config.json           # instance config (live, points at Project v2 #1)
├── agent-conductor.config.ts             # self-documenting TS type-checked sibling
├── .github/projects/agent-cicd-board.md  # one-time Project v2 setup runbook
├── team/
│   ├── board.md                          # active execution board (markdown)
│   ├── board-v2.md                       # V2 roadmap board
│   ├── contracts/                        # per-track contracts (frontmatter + body)
│   ├── intake/                           # forward-looking decomposition intakes
│   ├── queue/                            # append-only inter-track messages
│   ├── schema/board.schema.json          # JSON Schema 2020-12 for board tasks
│   ├── status/                           # per-track current status (worker-owned)
│   └── archive/                          # closed waves
└── tools/agent-conductor/
    ├── bin/agent-conductor               # node shim → dist/cli/index.js
    ├── lint/no-host-repo-references.mjs  # modularity guard (greps src/ for repo terms)
    ├── docs/
    │   ├── shadow-run.md                 # operator ritual
    │   └── shadow-run-report-template.md # report scaffolding
    ├── launchd/com.xvision.agent-conductor.plist  # macOS launchd template
    ├── prompts/worker.md                 # worker kickoff prompt template
    ├── scripts/                          # node scripts (no build step)
    │   ├── migrate-board.mjs             # markdown → Project v2 items
    │   ├── parse-board.mjs               # markdown row → typed structure
    │   └── validate-schema.mjs           # ajv-driven schema validator
    ├── src/                              # daemon TypeScript (ESM)
    │   ├── archive/flow.ts               # MERGED → ARCHIVED interface
    │   ├── claim/primitive.ts            # READY → CLAIMED (git push as claim)
    │   ├── cli/index.ts                  # commander CLI
    │   ├── config/load.ts                # config loader + validation
    │   ├── daemon/{lock.ts,digest.ts}    # PID lock + append-only digest
    │   ├── modes/env.ts                  # SHADOW / ENABLE / POLL flags
    │   ├── state/machine.ts              # Phase-1 transition planner (pure)
    │   ├── status/envelope.ts            # v1 status envelope builder
    │   └── types.ts                      # domain types — host-repo agnostic
    └── test/                             # vitest (66 passing)
```

## What's portable vs xvision-specific

The daemon source (`tools/agent-conductor/src/`) is **fully
host-repo-agnostic** and lint-enforced (`lint/no-host-repo-references.mjs`
greps `src/` for a denylist: `xvision`, `latentwill`, `kinamix`,
etc. — fails CI on any hit). Everything host-specific lives in the
config file.

| Layer | Portable | Notes |
|---|---|---|
| `src/state/machine.ts` (Phase-1 planner) | ✅ pure | takes `BoardTask` + `ObservedReality`, returns `PlannedTransition`. No I/O. |
| `src/types.ts` (domain types) | ✅ generic | `TaskStatus`, `TaskLane`, `BoardTask`, `AgentConductorConfig` — no host references. |
| `src/cli/index.ts` (commander surface) | ✅ generic | `start \| stop \| pause \| resume \| status \| watch \| cancel` |
| `src/config/load.ts` | ✅ generic | walks up from cwd for `agent-conductor.config.{json,ts}` |
| `src/daemon/digest.ts` | ✅ generic | per-UTC-date append-only digest |
| `src/modes/env.ts` | ✅ generic | three env flags drive shadow / enable / poll-override |
| `src/claim/primitive.ts` | ✅ interface | `GhClient` / `GitClient` / `Spawn` / `Fs` are interfaces. No concrete impls in Phase-1. |
| `src/archive/flow.ts` | ✅ interface | `ArchiveGhClient` interface. No concrete impl. |
| `scripts/migrate-board.mjs` | ✅ generic | takes `--board`, `--project`, `--repo`. xvision boards happen to use markdown — todoworld could swap parser. |
| `scripts/parse-board.mjs` | ✅ generic | string → structured. Both `- - -` and `— · ·` row separators. |
| `scripts/validate-schema.mjs` | ✅ generic | ajv 2020-12 validator |
| `team/schema/board.schema.json` | ✅ generic | the 13-state task schema |
| `team/board.md` / `team/board-v2.md` | xvision content | markdown content; format is portable. |
| `team/contracts/*` | xvision content | the contract format and frontmatter are portable; bodies are xvision-specific. |
| `.github/projects/agent-cicd-board.md` | xvision-flavored | references `latentwill` owner; instructions are portable. |
| `agent-conductor.config.{json,ts}` | xvision-specific | name/repo/project numbers. |
| `launchd/com.xvision.agent-conductor.plist` | xvision-named | template substitutions handle this — name lives in plist file id. |

Cleanest port: copy `tools/agent-conductor/{src,scripts,bin,lint,prompts,docs,launchd,test,package.json,tsconfig.json}` + `team/schema/board.schema.json` + `.github/projects/agent-cicd-board.md` into todoworld. Write a fresh `agent-conductor.config.json`. Done.

## The four Phase-1 contracts (all merged)

| # | Contract | PR | Status | Substance |
|---|---|---|---|---|
| 1 | `agent-cicd-board-schema` | (merged pre-#290) | merged | JSON Schema 2020-12 for the task object + Project v2 setup runbook + validator. Blocks the other three. |
| 2 | `agent-cicd-migrate-board` | #290 | merged | One-time idempotent script: parse markdown boards + contract frontmatter, validate against schema, populate GitHub Issues + Project v2 items. |
| 3 | `agent-cicd-daemon-skeleton` | #295 | merged | Node/TS daemon at `tools/agent-conductor/`. CLI surface, three-layer status surface, instance identity, host-repo-agnostic lint boundary, Phase-1 state machine. |
| 4 | `agent-cicd-shadow-run` | #315 | merged | Cohort intake, ritual docs, replay integration test (23 tests, 100% agreement on 17 transitions across 5 historical tracks), archive deliverables (`report.md`, `digest.md`, `final-board.json`). |
| 4.5 | `agent-cicd-migrate-board-setup-gaps` | #321 | merged | Three migration script fixes uncovered during the first live run. Listed separately because it landed after #315 unblocked the live flip. |

Out-of-scope-but-related:

- `agent-cicd-extract-package` — Phase-2 work to publish to npm + `npx agent-conductor init` scaffolder. `status: deferred` pending Phase-2 review-routing landing.

## Architecture in one paragraph

The board is a GitHub Project v2 keyed on a per-task JSON Schema. Each Issue carries a `track:<slug>` label and lives at one of 13 statuses. A small Node daemon (`agent-conductor`) polls the board, plans Phase-1 transitions via a pure planner (`READY → CLAIMED`, `CLAIMED → CODING`, `CODING → PR_OPEN`, `MERGED → ARCHIVED`), and executes the side effects through a `GhClient` (graphql + REST), `GitClient` (worktrees + ref push), and a `Spawn` (Claude CLI). Shadow mode flips a single env gate; the planner is pure so shadow is vacuously safe. State transitions other than the four Phase-1 actions are logged as `observe-only` for Phase-2/3 to act on later. The claim primitive is `git push origin <baseSha>:refs/heads/<prefix><track>` because GitHub's `updateProjectV2ItemFieldValue` has no compare-and-swap; the ref-create gives us server-side atomicity.

## Setup gotchas (read before running)

These bit us on xvision's first live migrate-board run and are now fixed in #321; if you skip the script and roll your own board setup, you'll hit them again.

### Gotcha 1: GitHub forces the default `Status` field name

Every new Project v2 ships with a SINGLE_SELECT field named literally `Status` (capital S). It **cannot** be deleted (`Only custom fields can be deleted`), **cannot** be renamed (the rename mutation silently no-ops), **cannot** be shadowed by a sibling `status` field (`Name has already been taken`).

The migrate script lower-cases all field keys on insert so descriptor `status` updates match the live `Status` field. **If you write your own client, do the same.** Don't try to fight GitHub here.

To replace the default `Status` field's options with the 13-state schema enum, use the raw GraphQL mutation:

```
mutation($fieldId: ID!, $opts: [ProjectV2SingleSelectFieldOptionInput!]!) {
  updateProjectV2Field(input: { fieldId: $fieldId, singleSelectOptions: $opts }) {
    projectV2Field { ... on ProjectV2SingleSelectField { name options { name } } }
  }
}
```

Full payload in `.github/projects/agent-cicd-board.md` Step 2.5.

### Gotcha 2: `gh issue create --label X` aborts if X doesn't exist

The migrate script now pre-flights all required `track:*` and `lane:*` labels via `ensureLabels()`. Stable colour scheme: foundation=`0e8a16`, integration=`1d76db`, leaf=`fbca04`, track=`5319e7`.

### Gotcha 3: Board markdown might use em-dash row separators

xvision's `team/board.md` uses ASCII hyphens (`- - -`); `team/board-v2.md` uses em-dash + middle-dot (`— · ·`). The parser normalises both. If todoworld's boards use a third variant, extend the normaliser.

### Gotcha 4: scripts/ and src/ are separate packages

`tools/agent-conductor/scripts/` has its own `package.json` (ajv, ajv-formats — no TS). `tools/agent-conductor/src/` (the daemon) is the bigger TS package. **You need to `npm install` in both.** The daemon's `npm install` does not transitively pull in scripts/.

### Gotcha 5: `tools/agent-conductor/src/` ships interfaces, not concrete clients

`src/claim/primitive.ts` and `src/archive/flow.ts` define `GhClient` and `ArchiveGhClient` interfaces. Phase-1 has no concrete implementation that actually calls `gh api graphql`. The migrate-board script's `makeGhClient` in `scripts/migrate-board.mjs` is a near-template but lives in a different package. Before the daemon can run live, **someone has to wire the concrete client**. This is the next contract to write.

## Porting paths for todoworld

### Path A — copy now (fastest, recommended)

1. `cp -r` of:
   - `tools/agent-conductor/` (whole tree)
   - `team/schema/board.schema.json`
   - `.github/projects/agent-cicd-board.md` (rename to match the todoworld owner)
2. In todoworld, run the Project v2 setup runbook (Steps 1–3 of `.github/projects/...`). Note the project number.
3. Create `tools/agent-conductor/launchd/com.todoworld.agent-conductor.plist` from xvision's plist (rename the plist `Label` and substitutions).
4. Write `agent-conductor.config.json` at todoworld's repo root:
   ```json
   {
     "version": "v1",
     "name": "todoworld",
     "repo": { "owner": "<owner>", "name": "todoworld" },
     "project": { "owner": "<owner>", "number": <N> },
     "paths": {
       "worktreeRoot": ".worktrees",
       "queueDir": "team/queue"
     },
     "branch": { "prefix": "agent/" },
     "pollIntervalS": 30,
     "contractsDir": "team/contracts",
     "schemaPath": "team/schema/board.schema.json"
   }
   ```
5. `cd tools/agent-conductor && npm install && npm run build && npm test` — should hit 66 passing.
6. `gh auth refresh -s project,read:project`.
7. Run dry-run migrate against todoworld's own board markdown:
   `node tools/agent-conductor/scripts/migrate-board.mjs --dry-run --board team/board.md`
8. When ready: live migrate, then write the concrete `GhClient` (next-contract).

What you don't need from xvision: `team/board*.md` (todoworld has its own), `team/contracts/*` (todoworld writes its own), `agent-conductor.config.ts` (the JSON sibling is enough; the TS exists only for type-checking).

### Path B — wait for `agent-cicd-extract-package`

The deferred Phase-2 contract publishes the daemon as `agent-conductor` on npm with an `npx agent-conductor init` scaffolder. Initial version `1.0.0`. Once it ships, todoworld would:

```bash
cd todoworld
npx agent-conductor init       # drops config stub, schema, plist template, setup doc
$EDITOR agent-conductor.config.json
agent-conductor status --json --shadow
```

This is the long-term right answer but blocked on Phase-2 review-routing landing first.

## Next contract to write (for either xvision or todoworld going live)

**`agent-cicd-ghclient-impl`** — concrete `GhClient` and `ArchiveGhClient` implementations.

Sketch:

- `lane: integration`, `depends_on: agent-cicd-migrate-board-setup-gaps`.
- Allowed paths: `tools/agent-conductor/src/clients/**`, `tools/agent-conductor/test/clients/**`, plus a small wire-up in `src/cli/index.ts`.
- Implements `GhClient` (claim primitive: `findProjectItem`, `setFieldValue`, `pushRefAsClaim`, `rollbackClaim`) and `ArchiveGhClient` (archive flow: `closeIssue`, `setFieldValue=ARCHIVED`).
- Re-uses `scripts/migrate-board.mjs::makeGhClient` patterns for the gh-CLI subprocess shape; lifts those to the daemon's package.
- Must respect the case-insensitive `Status` field rule from Gotcha 1.
- Has its own zero-mutation test variant for shadow mode (the daemon's `mutationTripwire` pattern from the shadow-run integration test is the template).

When this lands, xvision and any forked todoworld can go live by installing the launchd plist and unsetting `AGENT_CONDUCTOR_SHADOW`.

## Verification commands (run in either repo)

```bash
# 1. The daemon package builds + tests.
cd tools/agent-conductor
npm install && npm run build && npm test && npm run typecheck && npm run lint

# 2. The scripts package tests.
cd scripts && npm install && node --test parse-board.test.mjs migrate-board.test.mjs

# 3. Schema validator self-checks.
node tools/agent-conductor/scripts/validate-schema.mjs team/schema/board.schema.json
node tools/agent-conductor/scripts/validate-schema.mjs --check-examples team/schema/examples/

# 4. Parser sees both row formats.
node -e 'import("./tools/agent-conductor/scripts/parse-board.mjs").then(m => console.log(m.parseBoard("## A\n### X\n- [t1](contracts/t1.md) - leaf - ready - hyphen.\n- [t2](contracts/t2.md) — leaf · ready · em-dash.\n").length))'
# Expected: 2

# 5. Migrate dry-run against repo boards.
node tools/agent-conductor/scripts/migrate-board.mjs --dry-run --board team/board.md
```

## Source-of-truth links

| Topic | Path |
|---|---|
| Phase-1 spec | `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md` |
| Process spec (board / contracts / conductor) | `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md` |
| Shadow-run ritual | `tools/agent-conductor/docs/shadow-run.md` |
| Shadow-run report archive | `team/archive/agent-cicd-phase-1-shadow/{report,digest,final-board}.md/json` |
| Board task schema | `team/schema/board.schema.json` |
| Daemon README | `tools/agent-conductor/README.md` |
| First-run gap-fix history | `team/queue/agent-cicd-migrate-board__20260519T000000Z__setup-gaps-found-during-first-run.md` |
| Extract-package contract (deferred) | `team/contracts/agent-cicd-extract-package.md` |

## What's NOT in Phase-1 (and where it lives)

- **Review-comment routing** (`CHANGES_REQUESTED → FIXING`) — Phase-2; no contract cut yet.
- **Auto-merge** (`gh pr merge`) — Phase-2.
- **Deploy workflow dispatch** — Phase-3.
- **Multi-repo Hermes routing** beyond instance identity in the status envelope — Phase-2.
- **Concrete `GhClient` / `ArchiveGhClient`** — between Phase-1 skeleton and Phase-2 routing; needs its own track (see "Next contract to write" above).

## When in doubt

`tools/agent-conductor/lint/no-host-repo-references.mjs` is the boundary contract. If you find yourself wanting to reference `xvision`, `latentwill`, `todoworld`, or any specific repo from `src/`, you're modeling something wrong — push it into config.
