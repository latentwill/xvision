---
track: agent-cicd-daemon-skeleton
lane: foundation
wave: agent-cicd-phase-1
worktree: .worktrees/agent-cicd-daemon-skeleton
branch: task/agent-cicd-daemon-skeleton
base: origin/main
status: ready
depends_on:
  - agent-cicd-board-schema  # needs team/schema/board.schema.json committed
blocks:
  - agent-cicd-shadow-run
stacking: none
allowed_paths:
  - tools/agent-conductor/package.json
  - tools/agent-conductor/tsconfig.json
  - tools/agent-conductor/README.md
  - tools/agent-conductor/bin/agent-conductor
  - tools/agent-conductor/src/**
  - tools/agent-conductor/test/**
  - tools/agent-conductor/launchd/com.xvision.agent-conductor.plist
  - tools/agent-conductor/prompts/worker.md
  - tools/agent-conductor/lint/no-host-repo-references.mjs
  - agent-conductor.config.ts
  - .gitignore
forbidden_paths:
  - team/board.md
  - team/board-v2.md
  - team/contracts/**
  - team/schema/**
  - crates/**
  - frontend/web/**
  - migrations/**
  - tools/agent-conductor/scripts/**  # migrate-board lives here, separate track
interfaces_used:
  - team/schema/board.schema.json (load + validate observed state)
  - GitHub GraphQL API (gh api graphql) for Project reads + field updates
  - GitHub REST API (gh api) for PR + check status
  - `claude` CLI (spawned as a child process per task)
  - `git worktree` (subprocess)
  - filesystem: `~/.cache/agent-conductor/{lock,digest-<date>.md}`
  - team/queue/<track>__<utc>__*.md (write claim/PR-open markers, per existing convention)
parallel_safe: true
parallel_conflicts: []
verification:
  - (cd tools/agent-conductor && npm install)
  - (cd tools/agent-conductor && npm run typecheck)
  - (cd tools/agent-conductor && npm run lint)
  - (cd tools/agent-conductor && npm test)
  - (cd tools/agent-conductor && node bin/agent-conductor --help) # prints CLI surface
  - (cd tools/agent-conductor && node lint/no-host-repo-references.mjs src/) # boundary lint
  - (cd tools/agent-conductor && node bin/agent-conductor status --json --config ../../agent-conductor.config.ts | node -e 'const s = require("fs").readFileSync(0,"utf8"); const j = JSON.parse(s); if (!j.instance?.name || !j.envelope?.schema) process.exit(1)')
acceptance:
  - "`tools/agent-conductor/` is a self-contained TypeScript Node package. `npm install` + `npm test` pass without any Rust toolchain. Not part of any Cargo workspace."
  - "CLI surface at `tools/agent-conductor/bin/agent-conductor` implements: `start`, `stop`, `pause`, `resume`, `status [--json]`, `watch [--json]`, `cancel <track>`, `--help`, `--version`. Each subcommand has a unit test covering the parser; `start` and `stop` have integration tests covering lock acquisition/release. `status` and `watch` have golden-file tests asserting the v1 envelope schema."
  - "Status surface — three layers, all required: (1) `agent-conductor status --json` prints the v1 envelope (envelope + instance + daemon + tasks + stuck + digest_tail) to stdout; (2) `agent-conductor watch --json` emits one such object per poll, newline-delimited, until SIGINT; (3) every poll atomically writes `<cacheDir>/state.json` (write to `state.json.tmp`, fsync, rename). TTY-without-`--json` mode for `status` and `watch` renders a small ink TUI; piped/`--json` always emits machine output."
  - "Instance identity in the status output is required: `instance.name` (from config; daemon refuses to start if missing/empty), `instance.repo`, `instance.project`, `instance.host` (`os.hostname()`), `instance.daemon_version` (from `package.json`), `instance.config_path` (absolute), `instance.config_hash` (sha256 of config file contents), `instance.config_version` (from `config.version`, default `v1`). Envelope top-level: `envelope.schema = \"agent-conductor.status/v1\"` and `envelope.ts`."
  - "Daemon `start` acquires `~/.cache/agent-conductor/lock` (PID + start time). If the lock is held by a live PID, exits 1 with a clear error including the holding PID. Stale lock (PID not alive) is reclaimed."
  - "Daemon polls every `pollIntervalS` (from config, default 30, env-overridable via `AGENT_CONDUCTOR_POLL_S`). Each poll fetches the Project board via `gh api graphql`, validates each item against `board.schema.json`, and computes the next transition for each task based on observed reality (worktree exists? branch exists? branch pushed? PR open? checks status?)."
  - "Config file loading: daemon loads `agent-conductor.config.ts` (or `.json`) from a path given by `--config <path>` or auto-discovered by walking up from cwd. Validated against `AgentConductorConfig` type at startup; invalid config exits non-zero with a pointed message naming the failing field. xvision's config lives at the repo root (`/agent-conductor.config.ts`) with `name: 'xvision'` and the repo/project coordinates filled in."
  - "Modularity boundary — enforced: `tools/agent-conductor/src/**` contains zero references to `xvision`, `team/`, `.worktrees/`, `latentwill`, project number 7, hardcoded branch prefixes, or any other host-repo-specific value. The check is mechanical: `tools/agent-conductor/lint/no-host-repo-references.mjs` greps the source for a denylist of terms and exits non-zero on hit. Runs in `npm run lint`. Tests use fixture configs, not the live xvision config."
  - "Phase-1 transitions implemented (and ONLY these): `READY → CLAIMED`, `CLAIMED → CODING`, `CODING → PR_OPEN`, `MERGED → ARCHIVED`. The state machine module recognizes every other transition as a no-op-with-log, so the daemon won't act on them but will surface stuck states in the digest. Tested per transition."
  - "Phase-1 `READY → CLAIMED` flow uses **ref-creation as the atomic claim primitive**. `updateProjectV2ItemFieldValue` has no compare-and-swap input, so the Project field write CANNOT itself be the claim. Order is: (1) check `OWNERSHIP` + `CONFLICT_ZONES` against in-flight tasks' allowed_paths; skip if any conflict. (2) Acquire local advisory lock `~/.cache/agent-conductor/claims/<track>.lock` (fast-path; not the authority). (3) **Server-side claim**: `git push origin <base-sha>:refs/heads/agent/<track>` as a non-force ref-create. Success = this daemon owns the claim; failure (`reference already exists` / non-fast-forward from GitHub) = back off and pick the next candidate. NO side effects yet. (4) Refuse to claim if `.worktrees/<track>` already exists dirty — surface to digest, and `git push origin :refs/heads/agent/<track>` to retract our just-created ref iff its tip still equals `<base-sha>`. (5) `git fetch origin agent/<track>` then `git worktree add .worktrees/<track> agent/<track>`. (6) Write `team/queue/<track>__<utc>__claimed.md` with `instance.name`, `instance.host`, daemon PID. (7) Update Project: `status=CLAIMED`, `owner_agent=<instance.name>:<host>`, `branch=agent/<track>`, `worktree=.worktrees/<track>`. (8) Spawn `claude` with cwd=worktree and the contract-built prompt; append worker PID to the queue marker. (9) **Verify-after-write**: immediately re-read the Project item. If `owner_agent` does not match this daemon's identity, roll back (kill worker, `git worktree remove --force`, push-delete the ref iff the remote tip still equals `<base-sha>`)."
  - "Concurrency tests cover the ref-creation primitive explicitly: (a) Two daemons race to claim the same READY track against a mocked GH GraphQL+REST server that serializes `refs/heads/agent/<track>` creation — assert exactly one daemon advances past step 3, the other backs off with a clean `reference already exists` error and zero side effects. (b) Daemon crash simulated between step 3 (ref created) and step 7 (Project written): on next poll, the daemon observes `status=READY` AND remote ref exists at `<base-sha>` AND no queue marker — recognizes the orphaned claim and adopts it without creating a duplicate ref. (c) Operator races the daemon by creating `agent/<track>` via `gh` first: daemon's push fails at step 3, daemon does NOT write a queue marker, does NOT spawn a worker, does NOT touch the Project — surfaced to digest as `manual claim observed`."
  - "Phase-1 `MERGED → ARCHIVED` flow: (1) detect PR merged on `main`; (2) `git worktree remove --force .worktrees/<track>`; (3) `git branch -D agent/<track>` locally; (4) move `team/queue/<track>__*.md` into `team/queue/archive/<date>/`; (5) update Project field `status=ARCHIVED`; (6) close the Issue if not already closed."
  - "Shadow mode: env `AGENT_CONDUCTOR_SHADOW=1` makes ALL transitions print-only (write to stdout + digest, no GraphQL mutations, no `git worktree` calls, no `claude` spawns). Default is live. Shadow mode is what `agent-cicd-shadow-run` will exercise."
  - "Kill switch: env `AGENT_CONDUCTOR_ENABLE=0` (or unset, with the launchd plist requiring it to be `1`) makes the daemon log and exit 0 on start. Documented in README."
  - "Digest: each poll appends one entry to `~/.cache/agent-conductor/digest-<YYYY-MM-DD>.md` summarizing transitions executed, transitions deferred (and why), and stuck tasks (>24h READY, >2h REVIEWING, >2h with no commits in CODING/FIXING). Rotation is by filename; no log shipping."
  - "Crash-safe restart: on `start`, the daemon reads board state, scans `.worktrees/`, reads `team/queue/*__claimed.md`, and reconciles before resuming polling. A test simulates a kill mid-poll and asserts the next start produces no duplicate work."
  - "Concurrency: a unit test confirms two-daemon attempt fails — the second exits with the holding PID in the error message."
  - "Observation-only on Phase-2 / Phase-3 transitions: when the daemon sees `CHANGES_REQUESTED`, `FIXING`, `APPROVED`, `MERGE_READY`, `DEPLOYED`, etc., it logs the observation to the digest but does NOT act. Tested."
  - "`tools/agent-conductor/launchd/com.xvision.agent-conductor.plist` template included with placeholders for `HOME` and the binary path. README documents the install (`launchctl bootstrap`) and uninstall steps. The plist itself is committed but not installed by the contract — operator action."
  - "`.gitignore` adds `tools/agent-conductor/node_modules/`, `tools/agent-conductor/dist/`."
  - "No Phase-2 work in this contract: no review-comment routing, no `FIXING` prompt synthesis, no `gh pr merge` calls, no deploy workflow dispatch."
---

# Scope

Skeleton for the agent-conductor daemon described in
`docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`. Phase-1
scope only: claim ready tasks, manage worktrees, launch Claude Code
workers, detect PR open, archive merged tasks. Everything past
`PR_OPEN` is **observed but not acted on** until Phase 2.

The daemon must be crash-safe (state lives on GitHub), idempotent across
restarts, and protected by a single PID-file lock. Shadow mode is built
in from day one so `agent-cicd-shadow-run` can validate behavior against
the real migrated board without touching live state.

# Out of scope

- Review-comment routing (`CHANGES_REQUESTED → FIXING`) — Phase 2 contract.
- Auto-merge logic — Phase 3, and even then it's gated by a label.
- Deploy workflow dispatch — Phase 3.
- The migration script (`agent-cicd-migrate-board`, separate track).
- Schema definition (`agent-cicd-board-schema`, foundation).
- Editing `team/board.md`, `team/board-v2.md`, contracts, or schema.
- Cargo / Rust workspace integration. The daemon is a Node tool.
- Building or deploying anything.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-cicd-daemon-skeleton status
git -C .worktrees/agent-cicd-daemon-skeleton log --oneline -3 origin/main..HEAD
# Confirm: agent-cicd-board-schema is merged into origin/main first
git -C .worktrees/agent-cicd-daemon-skeleton log origin/main -- team/schema/board.schema.json | head -5
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-cicd-daemon-skeleton -b task/agent-cicd-daemon-skeleton origin/main
```

# Notes

Worker prompt template lives in `tools/agent-conductor/src/prompts/`.
Keep it tiny — the spec says the worker reads the contract and the
pinned PR comment directly; the conductor prompt is just the kickoff.

Lockfile (`package-lock.json` or `pnpm-lock.yaml`) is committed.
Prefer `npm` to match the simplest local install; `pnpm` is fine if the
PR justifies it.
