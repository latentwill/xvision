# agent-conductor

Local control plane that claims READY tasks from a GitHub Project v2 board,
manages worktrees, spawns Claude Code workers, detects opened PRs, and
archives merged work. Phase-1 skeleton — Phase-2 review routing and Phase-3
deploy dispatch land in separate contracts.

The daemon is intentionally host-repo-agnostic. Every host-specific value
(instance name, repo, project number, worktree root, branch prefix, paths)
comes from the loaded config file; `src/` contains zero references to any
particular repo. A `lint/no-host-repo-references.mjs` script greps `src/`
for a denylist of host-repo terms and exits non-zero on any hit — wired
into `npm run lint`.

## Layout

```
tools/agent-conductor/
├── bin/agent-conductor              # node shim — delegates to dist/cli/index.js
├── dist/                            # build output (gitignored)
├── launchd/com.xvision.agent-conductor.plist  # template for macOS launchd
├── lint/no-host-repo-references.mjs # modularity boundary guard
├── prompts/worker.md                # kickoff prompt template
├── src/                             # daemon source (TypeScript, ESM)
│   ├── archive/flow.ts              # MERGED → ARCHIVED
│   ├── claim/primitive.ts           # READY → CLAIMED (ref-creation primitive)
│   ├── cli/index.ts                 # commander-based CLI
│   ├── config/load.ts               # JSON / TS config loader with validation
│   ├── daemon/lock.ts               # PID-file lock
│   ├── daemon/digest.ts             # append-only per-day digest
│   ├── modes/env.ts                 # SHADOW / ENABLE / POLL env flags
│   ├── state/machine.ts             # Phase-1 transition planner
│   ├── status/envelope.ts           # v1 status envelope builder
│   └── types.ts                     # domain types
└── test/                            # vitest suites (10 files, 42 tests)
```

## Install

```bash
cd tools/agent-conductor
npm install
npm run build
```

Node 20+ required. No Rust toolchain is involved — this package is fully
independent of the host repo's Cargo workspace.

## Config

The daemon needs an `agent-conductor.config.{json,ts}` file. By default it
walks up from `cwd`; pass `--config <path>` to override. Required fields:

| key | shape | notes |
|---|---|---|
| `name` | non-empty string | instance name; daemon refuses to start without it |
| `repo` | `{ owner, name }` | host GitHub repo |
| `project` | `{ owner, number }` | Project v2 location |
| `paths.worktreeRoot` | string | relative path the daemon adds worktrees under |
| `paths.queueDir` | string | per-host queue marker directory |
| `branch.prefix` | string | prepended to `<track>` for the agent branch |
| `contractsDir` | string | path to per-track contract files |
| `schemaPath` | string | path to the board task JSON Schema |

Optional: `version` (default `v1`), `pollIntervalS` (default 30),
`paths.cacheDir` (default `~/.cache/agent-conductor`).

## CLI surface

```
agent-conductor --version
agent-conductor --help
agent-conductor [--config <path>] start
agent-conductor [--config <path>] stop
agent-conductor [--config <path>] pause
agent-conductor [--config <path>] resume
agent-conductor [--config <path>] status [--json]
agent-conductor [--config <path>] watch [--json] [--interval <s>]
agent-conductor [--config <path>] cancel <track>
```

`status` and `watch` emit a v1 envelope (see below). Without `--json` and
on a TTY they render a compact text summary; with `--json` or when stdout
is piped, output is machine-readable.

## Status envelope (v1)

Three surfaces, one schema (`agent-conductor.status/v1`):

1. `agent-conductor status --json` — single object to stdout
2. `agent-conductor watch --json` — newline-delimited objects until SIGINT
3. `<cacheDir>/state.json` — atomically rewritten each poll
   (write → fsync → rename; the poll loop wires this up)

```json
{
  "envelope": { "schema": "agent-conductor.status/v1", "ts": "..." },
  "instance": {
    "name": "...",
    "repo": "<owner>/<name>",
    "project": "<owner>:<number>",
    "host": "...",
    "daemon_version": "...",
    "config_path": "...",
    "config_hash": "<sha256>",
    "config_version": "v1"
  },
  "daemon": {
    "pid": null,
    "started_at": null,
    "state": "stopped|starting|running|paused|stopping",
    "shadow": false,
    "enabled": true,
    "poll_interval_s": 30,
    "last_poll_at": null,
    "next_poll_at": null
  },
  "tasks": [],
  "stuck": [],
  "digest_tail": []
}
```

## Environment flags

| var | default | effect |
|---|---|---|
| `AGENT_CONDUCTOR_SHADOW` | `0` | when truthy, every transition is print-only — no GraphQL mutations, no `git worktree` calls, no `claude` spawns |
| `AGENT_CONDUCTOR_ENABLE` | `1` | when falsy, `start` logs the kill switch and exits 0 without acquiring the lock |
| `AGENT_CONDUCTOR_POLL_S` | (unset) | overrides `config.pollIntervalS` |

## State machine

Phase-1 acts on four transitions only:

| from | to | trigger |
|---|---|---|
| `READY` | `CLAIMED` | host-conflict-free + ref-creation primitive succeeds |
| `CLAIMED` | `CODING` | worktree + branch present (worker started) |
| `CODING` | `PR_OPEN` | GitHub PR number observed |
| `MERGED` | `ARCHIVED` | PR merged on the default branch |

Every other observed transition (`CHANGES_REQUESTED`, `FIXING`, `APPROVED`,
`MERGE_READY`, `DEPLOYED`, …) is logged to the digest as
`observe-only` but never executed by Phase-1.

## Claim primitive

`updateProjectV2ItemFieldValue` has no compare-and-swap input, so the
Project field write CANNOT be the claim. The atomic primitive is a
non-force ref-create: `git push origin <baseSha>:refs/heads/<prefix><track>`.
The flow is:

1. host-conflict check (caller skips on overlap)
2. local advisory lock at `<cacheDir>/claims/<track>.lock` (fast path)
3. **server-side claim** — `git push` of `refs/heads/<prefix><track>`
4. refuse + rollback if `<worktreeRoot>/<track>` already exists dirty
5. `git fetch` + `git worktree add`
6. write `<queueDir>/<track>__<utc>__claimed.md`
7. update Project: `status=CLAIMED`, `owner_agent=<name>:<host>`,
   `branch`, `worktree`
8. spawn the worker via `claude`; append worker PID to the queue marker
9. verify-after-write: re-read the Project item; roll back on mismatch

Rollback only deletes the remote ref when its tip still equals the base
sha — we never clobber a peer daemon's work.

## Archive flow

`MERGED → ARCHIVED`:

1. `git worktree remove --force <worktreeRoot>/<track>`
2. `git branch -D <prefix><track>` (local)
3. move `<queueDir>/<track>__*.md` into `<queueDir>/archive/<YYYY-MM-DD>/`
4. update Project field `status=ARCHIVED`
5. close the GitHub Issue if open

## Tests

```bash
npm test           # vitest run — 10 files, 42 tests
npm run typecheck  # tsc --noEmit
npm run lint       # modularity boundary lint
```

Concurrency tests for the claim primitive use an in-memory
`GitClient`/`GhClient` that serializes ref creation so we can prove
exactly one of two racing daemons advances past step 3, the other backs
off with `reference already exists` and writes nothing. A third test
asserts verify-after-write rollback when the Project echoes a foreign
`owner_agent`.

## launchd install (operator action)

```bash
# Substitute placeholders before installing.
sed -e "s|{{HOME}}|$HOME|g" \
    -e "s|{{BIN_PATH}}|$(pwd)/bin/agent-conductor|g" \
    -e "s|{{REPO_PATH}}|$(git rev-parse --show-toplevel)|g" \
    launchd/com.xvision.agent-conductor.plist \
    > "$HOME/Library/LaunchAgents/com.xvision.agent-conductor.plist"

launchctl bootstrap "gui/$(id -u)" \
    "$HOME/Library/LaunchAgents/com.xvision.agent-conductor.plist"

# Uninstall
launchctl bootout "gui/$(id -u)/com.xvision.agent-conductor"
rm "$HOME/Library/LaunchAgents/com.xvision.agent-conductor.plist"
```

The plist asserts `AGENT_CONDUCTOR_ENABLE=1` explicitly so the launchd
template itself is the operator's opt-in. To disable without uninstalling,
flip the value in the plist and `launchctl bootout` + `bootstrap` again.

## Out of scope (Phase-2/3)

- Review-comment routing (`CHANGES_REQUESTED → FIXING`)
- Auto-merge (`gh pr merge`)
- Deploy workflow dispatch
- Conductor-driven board edits beyond Project field writes

These live in separate contracts.
