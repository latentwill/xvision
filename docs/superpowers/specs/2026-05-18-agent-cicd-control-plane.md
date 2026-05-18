# Agent CI/CD Control Plane — Spec

Date: 2026-05-18
Status: Draft for implementation
Author: Ed (operator) + spec scribe

## Goal

Turn the current copy-paste-between-sessions agent workflow into a production
pipeline. Agents coordinate through **state, branches, PRs, comments, and CI
gates** — never socially. A small local daemon (`agent-conductor`) drives the
lifecycle around the agents; the agents themselves stay dumb workers in
isolated worktrees.

The output is a closed loop:

```
Intake (issue / bug / QA finding)
   → Board entry (machine-readable)
   → Conductor dispatches worker into clean worktree
   → Worker pushes branch + opens PR
   → Codex / Claude review on the PR
   → Review comments routed back as fix tasks
   → CI gates pass
   → Merge to main
   → Self-hosted runner builds image + deploys over SSH
   → Health check confirms or rolls back
```

This spec is xvision-specific and builds on the conventions locked in
`docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`
(contracts, conductor role, OWNERSHIP, CONFLICT_ZONES, worktrees, queue).
It does not replace those; it automates the loop *around* them.

## Non-goals

- Replacing GitHub Actions with something custom. Actions remains the CI/CD
  substrate; we add a local controller in front of it.
- Auto-merging without human in the loop in Phase 1 or 2. The merge gate is
  enforced by checks; the merge decision stays human until Phase 3.
- Letting agents deploy from a branch. Deploy is **main-only**, and only via
  the controller workflow that respects the deployment guardrails in
  `CLAUDE.md` (local image build preferred, GHCR fallback).
- Replacing the existing `team/contracts/` system. Contracts remain the
  source of truth for scope; the board layer just becomes
  machine-readable on top.

## Current-state evidence

| Artifact | Reality | Friction |
|---|---|---|
| `team/board.md`, `team/board-v2.md` | Markdown, human-edited | Not machine-readable; conductor edits by hand; no clean state transitions |
| `team/queue/<track>__<utc>__<topic>.md` | Append-only claim/PR-open files | Works as a log, but not a state machine; conductor reads it to derive state |
| `team/status/<track>.md` | Per-track phase + notes | Free text inside a small vocabulary; not enforced |
| `.worktrees/<track>/` | One worktree per active track | Already enforced; keep |
| PR review | Manual: operator pastes Codex/Claude review output into a worker | The copy-paste step — primary toil |
| CI | `cargo test --workspace`, frontend build run ad hoc per branch | Not gated; no merge protection wired |
| Deploy | `scripts/deploy-image.sh` run by hand from operator's machine | Works but disconnected from the board |
| Stale branches | ~160 on origin | No archive policy after merge |

## Target operating model

### The board becomes the state machine

Move the board from prose markdown into a machine-readable surface. Two
options on the table — pick exactly one in Phase 1:

| Option | Pros | Cons |
|---|---|---|
| **GitHub Issues + Projects (recommended)** | Native to GH; same place as PRs; webhooks for free; survives operator's machine restart | Per-org rate limits; requires graceful offline handling; project schemas live outside repo |
| Local SQLite + GitHub mirror | Full control of schema; replayable; offline-first | We own a sync problem; reviewers can't see state without the daemon |

**Recommendation: GitHub Projects (v2) with one project per repo, plus a
JSON schema in `team/schema/board.schema.json` documenting required fields.**
The conductor and `agent-conductor` daemon read and write the project via
`gh api graphql`. Local mirror is an optimization, not a requirement.

### Task object (strict)

Every task is an Issue with these fields (Project v2 fields, all required
where marked ✱):

| Field | Type | Notes |
|---|---|---|
| `status` ✱ | enum | See state machine below |
| `lane` ✱ | enum: Foundation / Leaf / Integration | Same lanes as the overhaul spec |
| `track` ✱ | string | Maps 1:1 to `team/contracts/<track>.md` |
| `owner_agent` | string | e.g. `claude-code:opus-4.7`, `codex:o4`, or empty if unclaimed |
| `branch` | string | `agent/<track>` |
| `worktree` | string | `.worktrees/<track>` |
| `pr` | int / null | GH PR number once opened |
| `review_status` | enum: none / requested / blocking / approved | Derived from PR review state |
| `deploy_status` | enum: none / queued / building / deployed / failed / rolled_back | Set by deploy workflow |
| `intake_doc` | string | Path to intake/spec/QA report that spawned this |
| `created_at`, `updated_at` | timestamps | GH-native |

The Issue body holds the contract pointer + a "Do not edit" banner — the
contract file is the source of truth for scope, paths, and acceptance.

### State machine

```
BACKLOG
  → READY            (contract committed; dependencies met)
  → CLAIMED          (worker process attached; worktree created)
  → CODING           (commits landing on branch)
  → PR_OPEN          (branch pushed; PR opened with the contract linked)
  → REVIEWING        (Codex/Claude review requested)
  → CHANGES_REQUESTED (review left blocking comments)
  → FIXING           (worker resumed; addressing comments on same branch)
  → APPROVED         (review approval + CI green)
  → MERGE_READY      (rebased on main, no conflicts, gates green)
  → MERGED           (merged to main; branch deleted)
  → DEPLOYED         (image built and deployed; health check passed)
  → ARCHIVED         (issue closed; worktree pruned; queue files moved to team/archive/)
```

Transitions:

- `BACKLOG → READY`: conductor sets this after contract + ownership check.
- `READY → CLAIMED`: `agent-conductor` claims an unlocked task.
- `CLAIMED → CODING`: first commit pushed to `agent/<track>`.
- `CODING → PR_OPEN`: `gh pr create` succeeds with the contract linked in body.
- `PR_OPEN → REVIEWING`: Codex auto-review trigger or `@codex review` comment.
- `REVIEWING → CHANGES_REQUESTED | APPROVED`: from PR review state.
- `CHANGES_REQUESTED → FIXING`: conductor opens a fix subtask comment on the
  PR with the review findings list; resumes the worker with that prompt.
- `FIXING → REVIEWING`: re-review on next push.
- `APPROVED → MERGE_READY`: all required checks green AND base is `main`.
- `MERGE_READY → MERGED`: human (or Phase-3 merge queue) merges.
- `MERGED → DEPLOYED`: deploy workflow on `main` succeeds.
- `* → ARCHIVED`: terminal cleanup.

Backward transitions are explicit: `MERGE_READY → CHANGES_REQUESTED` is
allowed if late review finds something; `DEPLOYED → MERGED` is allowed if a
rollback happens, with `deploy_status = rolled_back`.

### The `agent-conductor` daemon

A small TypeScript or Python process, run locally on the operator's machine
(launchd / pm2 / `screen`). Lives in `tools/agent-conductor/` in this repo.

Loop (poll every N seconds; default 30):

1. Pull current board state from GH Projects.
2. For each task, compute next transition based on observed reality:
   - Worktree exists? Branch exists locally? Branch pushed? PR open?
     Review state? Checks status? Merge state?
3. For each transition:
   - `READY → CLAIMED`: pick the next unclaimed READY task whose
     OWNERSHIP/CONFLICT_ZONES do not collide with any in-flight task.
     Create the worktree via `git worktree add .worktrees/<track> -b agent/<track> origin/main`.
     Launch a Claude Code session in that worktree with a prompt built from
     the contract + intake doc.
   - `CODING → PR_OPEN`: detect the worker pushed; ensure PR has the
     contract link, lane label, and `track:<track>` label.
   - `PR_OPEN → REVIEWING`: post `@codex review` (or trigger the equivalent
     GH Action) and request a Claude self-review via Action.
   - `CHANGES_REQUESTED → FIXING`: collect blocking comments via
     `gh api repos/<o>/<r>/pulls/<n>/comments`; build a fix prompt
     ("address these review findings, push to same branch"); resume the
     worker (new Claude Code session in the same worktree, fresh context).
   - `MERGED → DEPLOYED`: dispatch the deploy workflow on the self-hosted
     runner (see below).
   - `DEPLOYED → ARCHIVED`: `git worktree remove`, archive the queue files,
     close the Issue.
4. Surface anything stuck >24h in `READY`, >2h in `REVIEWING`, or with
   `deploy_status=failed` to stdout and to today's digest file at
   `~/.cache/agent-conductor/digest-<date>.md`. No external webhook in
   v1.

The daemon must be **idempotent and crash-safe**: state lives on GitHub,
not in the daemon. Restarting the daemon must not duplicate work.
Concurrency control is by the GH Issue's `status` field (atomic via
GraphQL `updateProjectV2ItemFieldValue` conditional update); plus a local
file lock per-track to prevent two daemons from claiming the same task.

### Status surface — remote read access

Hermes (or any other agent / operator tool) needs to read the control
plane's current state over SSH without rebuilding the daemon's logic.
Three layers, all live from Phase 1.

#### 1. CLI for SSH-driven clients (primary)

`agent-conductor status` and `agent-conductor watch` are the canonical
read interface. Hermes does:

```bash
ssh ed@build-host agent-conductor status --json
ssh ed@build-host agent-conductor watch --json   # newline-delimited stream
```

Output schema (machine-stable, versioned, derived from
`board.schema.json`). The top-level `instance` block disambiguates
multiple control planes — Hermes can connect to several SSH hosts each
running their own `agent-conductor` and route by `instance.name`.

```json
{
  "envelope": {
    "schema": "agent-conductor.status/v1",
    "ts": "2026-05-18T12:34:56Z"
  },
  "instance": {
    "name": "xvision",
    "repo": "latentwill/xvision",
    "project": { "owner": "latentwill", "number": 7 },
    "host": "build-host.local",
    "daemon_version": "0.1.0",
    "config_path": "/Users/ed/Code/xvision/agent-conductor.config.ts",
    "config_hash": "sha256:9b1c…e44a",
    "config_version": "v1"
  },
  "daemon": {
    "pid": 4242,
    "started_at": "2026-05-18T10:25:00Z",
    "uptime_s": 7610,
    "paused": false,
    "shadow": false,
    "poll_interval_s": 30,
    "enable_flag": true
  },
  "tasks": [
    {
      "track": "qa-trace-broker-spans",
      "status": "CODING",
      "lane": "integration",
      "branch": "agent/qa-trace-broker-spans",
      "worktree": ".worktrees/qa-trace-broker-spans",
      "pr": null,
      "review_status": "none",
      "deploy_status": "none",
      "owner_agent": "claude-code:opus-4.7",
      "last_commit_ts": "2026-05-18T12:30:12Z",
      "stuck": false
    }
  ],
  "stuck": [],
  "digest_tail": ["2026-05-18T12:34:56Z claim ok qa-trace-broker-spans", "..."]
}
```

Identity field contract for multi-repo / multi-host Hermes routing:

| Field | Source | Stability |
|---|---|---|
| `envelope.schema` | Hardcoded in daemon | Bumped on breaking output changes (`v1` → `v2`) |
| `instance.name` | `config.name` (required) | Operator-chosen short slug, e.g. `xvision`, `kinamix`. Stable per repo |
| `instance.repo` | `config.repo.owner/name` | Stable per repo |
| `instance.project.number` | `config.project.number` | Stable per repo + Project |
| `instance.host` | `os.hostname()` | Stable per machine |
| `instance.daemon_version` | `package.json` version of the daemon binary | Bumps on each daemon release |
| `instance.config_path` | Absolute path to the loaded config file | Stable per install |
| `instance.config_hash` | sha256 of the loaded config file contents | Changes when config edited |
| `instance.config_version` | `config.version` (default `v1`) | Operator-bumped on config-shape changes |

Hermes should key on `instance.name` for routing decisions (it's the
human-facing slug); `instance.repo` + `instance.host` together form a
globally unique identifier across all running instances. The
`config_hash` lets Hermes detect "operator changed config since I last
spoke to this instance" without parsing the config.

A repo running this control plane MUST set `name` in its config; the
daemon refuses to start if `name` is missing or empty. Two daemons on
the same host with the same `name` fail to start (cache-dir collision
detection via lock file PID + instance name).

`watch` emits one JSON object per poll cycle (default every 30s).
TTY-without-`--json` mode renders a small ink TUI: in-flight tasks,
last digest line, daemon health. Piped or `--json` → machine output.
Same command, two renderers.

#### 2. `state.json` snapshot (passive, file-based)

After each poll the daemon atomically writes
`~/.cache/agent-conductor/state.json` with the same shape as the CLI's
`status --json` output. Hermes (or any external tool) can `scp` it:

```bash
scp ed@build-host:~/.cache/agent-conductor/state.json /tmp/xvision-state.json
```

Atomic write: write to `state.json.tmp`, `fsync`, `rename`. Readers
never see a partial document. Last-write-wins, no locking required for
the reader.

#### 3. Digest tail (operator-facing)

`~/.cache/agent-conductor/digest-<YYYY-MM-DD>.md` is the audit log —
markdown, append-only, one line per transition or stuck-task event.
Hermes can `tail -F` it over SSH for human-readable streaming context.
Rotated daily by filename.

#### Explicitly not in v1

- **No localhost HTTP server.** Add one only when a real client needs
  it (a tailnet dashboard, a CI-side status check). CLI + state.json
  covers Hermes today and is one less surface to authenticate.
- **No webhook out.** The daemon never POSTs anywhere. If a future
  client needs push, it polls `watch --json` or tails state.json's
  mtime.

### GitHub as the message bus

- **Worker prompt input**: PR description, contract file, and a single
  pinned "instructions" comment on the PR. Worker re-reads these on every
  session start; nothing in the daemon's memory.
- **Review output**: PR review comments. Codex and Claude both leave
  review comments natively via their GH integrations.
- **Fix tasks**: a single conductor-generated "Review findings to address"
  comment on the PR, written when transitioning to `FIXING`. The fix prompt
  references this comment by URL. The worker's job is to address the items
  in that comment and reply ✅ inline as it goes.
- **Status mirror**: the daemon writes a one-line status comment per
  transition (collapsed under a `<details>` block) so the PR has an
  audit trail without spamming.

### Merge gate (required checks on `main`)

Configure branch protection on `main`:

- `ci / rust-test` — `cargo test --workspace`
- `ci / rust-clippy` — `cargo clippy --workspace -- -D warnings`
- `ci / frontend-build` — `npm run build` in `frontend/web/`
- `ci / frontend-lint`
- `review / codex` — Codex review has no blocking findings
- `review / claude` — Claude self-review has no blocking findings
- Branch must be up-to-date with `main` (rebase, not merge commits)
- No dirty / untracked files in worker worktree at PR-open time
  (worker-side precheck; reported as a failed check if violated)

The conductor's `APPROVED → MERGE_READY` transition trusts these checks
and does not duplicate them; if GH says green, it's green.

### Self-hosted runner + deploy

Phase-3 work, but design it now so Phases 1-2 do not paint into a corner.

- **Runner host**: operator's MacBook initially, registered as a
  repository-level self-hosted runner with labels `self-hosted,
  xvision-build, macos`.
- **Workflow trigger**: `push` to `main`, plus `workflow_dispatch` for
  manual rebuilds. Never on PR branches.
- **Workflow steps** (matches `scripts/deploy-image.sh --push`):
  1. Checkout `main` at the merge commit.
  2. `scripts/deploy-image.sh --push <deploy-host>` — local Rust+Vite
     build, then `docker save | ssh deploy docker load`, per the
     CLAUDE.md guardrail (local image build is preferred path; GHCR is
     fallback only when no local build host is available).
  3. SSH `docker compose up -d` on the deploy host.
  4. Poll `GET /healthz` on the deploy host until it returns
     `{"sha": "<expected>", "uptime_s": >5, "ok": true}` or the
     90-second timeout expires.
  5. Compare running image digest to expected digest. Mismatch, or
     healthcheck not green within timeout → automated rollback to
     previous tag, then re-poll `/healthz`.
  6. Post `DEPLOYED` (or `rolled_back`) back to the Issue, and append a
     line to today's digest at
     `~/.cache/agent-conductor/digest-<date>.md`.
- **No deploys from agent branches.** Agents never have credentials for
  the deploy host. The runner does, via a job-scoped secret.

GHCR remains the documented fallback (see `scripts/deploy-ghcr.sh`); the
workflow has a manual-dispatch input `path=ghcr|local` so the operator
can flip when the MacBook is unavailable.

### Worktree lifecycle

Already enforced by current process. New rules:

- The daemon owns creation and teardown. Humans should not need to
  `git worktree add` for an agent-driven task.
- Daemon enforces `CARGO_TARGET_DIR=$HOME/.cargo-target/xvision` in any
  worktree it creates (per CLAUDE.md local-build cache discipline).
- On `ARCHIVED`, the daemon runs `git worktree remove --force` and deletes
  the local branch. Origin branch is deleted by GH after merge (already
  enabled).
- Daemon refuses to claim a track whose worktree already exists dirty —
  reports to the operator instead of nuking work.

### Lifecycle control

Three lifecycles share the system. Keep them distinct.

#### Task lifecycle — how work starts and stops

**Initiation** is one of three entry points. The daemon never invents
tasks except via path 3.

1. **Operator intake**: free-form report dropped into
   `team/intake/<date>-<topic>.md`. The human conductor decomposes it,
   writes one or more `team/contracts/<track>.md`, opens the Issues,
   and leaves them in `BACKLOG`.
2. **QA / bug report**: same path as operator intake — lands in
   `team/intake/` as a wave, decomposed by the conductor.
3. **Scope-drift spawn**: a reviewer (Codex / Claude / human) adds the
   `scope-drift` label to a review comment that can't be fixed on the
   current PR. The daemon opens a new `BACKLOG` Issue linked to the
   originating PR and stops there — the conductor still owns the
   `BACKLOG → READY` transition.

`BACKLOG → READY` is a **human gate**, always. The conductor flips the
status once the contract is committed, OWNERSHIP / CONFLICT_ZONES are
clean, and dependencies are met. The daemon will not auto-claim a
`BACKLOG` task no matter how long it sits.

**Termination** — five ways a task ends:

| Path | Trigger | Daemon action |
|---|---|---|
| Normal completion | `DEPLOYED` healthcheck green | Transition to `ARCHIVED`: `git worktree remove`, archive `team/queue/` files for the track, close Issue |
| Operator cancel | `cancelled` label set on Issue | SIGTERM the worker, close the PR with a "cancelled by operator" comment, prune worktree, transition to `ARCHIVED` |
| Worker stall | >2h with no commits in `CODING` or `FIXING`, or >24h in `READY`, or >2h in `REVIEWING` | Surface to today's digest. No auto-kill. Operator decides nudge vs cancel |
| Scope drift | `scope-drift` label on a review comment | Close PR, spawn a fresh `BACKLOG` Issue for the new scope (entry point 3), archive the original |
| Hard deploy failure | Rollback fires, then same SHA fails again | Park in `deploy_status=failed`. Operator unblock required before any further deploy on `main` |

#### Worker lifecycle — inside a single task

- **Spawn (`CLAIMED → CODING`)**: daemon creates the worktree, then
  launches `claude` with cwd = worktree and an initial prompt built
  from the contract + intake doc + the standing "open a PR when ready"
  footer. Worker PID and start time recorded in
  `team/queue/<track>__<utc>__claimed.md`.
- **Resume (`CHANGES_REQUESTED → FIXING`)**: daemon spawns a **fresh**
  Claude Code session in the same worktree. Prompt references the
  conductor-posted "Review findings to address" PR comment by URL.
  Fresh session, not a continuation — context comes from the PR and
  worktree state, never from prior session memory.
- **Clean exit**: worker exits 0 after `gh pr create`. Daemon detects
  the open PR on next poll and transitions to `PR_OPEN`.
- **SIGTERM** (operator cancel, daemon shutdown, or stall-kill on
  operator request): daemon sends SIGTERM, waits 30s, escalates to
  SIGKILL. Uncommitted work in the worktree is **preserved**; the
  worktree itself is not pruned until `ARCHIVED`. Operator can inspect.
- **Crash**: worker exits non-zero. Daemon logs to digest, leaves the
  worktree intact, **does not auto-retry**. Operator inspects and
  either re-claims or cancels.

#### Daemon lifecycle — the controller itself

CLI surface at `tools/agent-conductor/bin/agent-conductor`:

| Command | Effect |
|---|---|
| `agent-conductor start` | Acquire `~/.cache/agent-conductor/lock`. Refuse if held. Begin polling. Logs to stdout + digest file |
| `agent-conductor stop` | SIGTERM the daemon. Daemon finishes the current poll cycle, SIGTERMs in-flight workers (30s → SIGKILL), releases lock, exits. Worktrees and branches survive |
| `agent-conductor pause` | In-memory flag: suppress new `CLAIMED` transitions. Keep observing and surfacing state. Use to drain rather than freeze |
| `agent-conductor resume` | Clear the pause flag |
| `agent-conductor status` | Print current board snapshot + in-flight workers + last digest line. Read-only |
| `agent-conductor cancel <track>` | Equivalent to setting the `cancelled` label — convenience wrapper |

**Boot persistence**: registered as a launchd user agent on the MacBook
(`~/Library/LaunchAgents/com.xvision.agent-conductor.plist`). Auto-restarts
on crash. Does not start on boot until `AGENT_CONDUCTOR_ENABLE=1` is
set in the user environment — the kill switch.

**Crash-safe restart**: state lives on GitHub. On start, the daemon reads
the board, observes worktree + branch + PR state, and reconciles. Any
in-flight worker survives the daemon restart (it's a child of launchd,
not of the daemon process). On reconciliation, the daemon adopts the
worker by PID from `team/queue/`.

**Two-daemon guard**: file lock at `~/.cache/agent-conductor/lock`
contains the daemon's PID. If a second daemon tries to start, it reads
the PID, verifies the process is alive, and exits with a clear error.

### Modular packaging — designed for second-repo use from day one

Goal: an operator clones any repo, runs `npx agent-conductor init`, and
has a working control plane within an hour — no fork of the daemon, no
copy-pasted source. Phase 1 builds the daemon in-tree at
`tools/agent-conductor/`, but with a strict config boundary; Phase 2
extracts to a standalone npm package.

#### The hard rule: zero xvision in daemon source

The daemon source (`tools/agent-conductor/src/**`) must contain:

- No hardcoded paths like `team/`, `.worktrees/`, `crates/`.
- No hardcoded repo coordinates (`latentwill/xvision`, project number).
- No hardcoded branch prefix (`agent/`, `task/`).
- No hardcoded CI check names.
- No xvision-specific labels, file globs, or commands.

Everything repo-specific lives in **`agent-conductor.config.ts`** at
the repo root. If the daemon needs a constant from this list, it reads
it from config. A `grep -ri xvision tools/agent-conductor/src` in a CI
job enforces the rule.

#### Config file shape (illustrative)

```typescript
// agent-conductor.config.ts — committed to repo root
import type { AgentConductorConfig } from 'agent-conductor';

export default {
  name: 'xvision',                                       // identity slug surfaced to Hermes
  version: 'v1',                                         // config-shape version, bumped on breaks
  repo: { owner: 'latentwill', name: 'xvision' },
  project: { owner: 'latentwill', number: 7 },          // GH Project v2

  paths: {
    boardSources: ['team/board.md', 'team/board-v2.md'],
    contractsDir: 'team/contracts',
    schemaPath: 'team/schema/board.schema.json',
    worktreeRoot: '.worktrees',
    queueDir: 'team/queue',
    intakeDir: 'team/intake',
  },

  branch: { prefix: 'agent/', base: 'origin/main' },

  worker: {
    engine: 'claude-code',
    promptTemplate: 'tools/agent-conductor/prompts/worker.md',
    cargoTargetDir: '~/.cargo-target/xvision',          // optional
  },

  review: {
    bots: ['codex', 'claude'],
    blockingLabel: 'changes-requested',
    scopeDriftLabel: 'scope-drift',
  },

  ci: {
    requiredChecks: [
      'ci / rust-test', 'ci / rust-clippy',
      'ci / frontend-build', 'ci / frontend-lint',
      'review / codex', 'review / claude',
    ],
  },

  deploy: {                                              // Phase 3
    enabled: false,
    healthcheck: { url: '/healthz', timeoutS: 90, expectOk: true },
  },

  conductor: { pollIntervalS: 30, cacheDir: '~/.cache/agent-conductor' },
} satisfies AgentConductorConfig;
```

The TS-config is the default; a JSON variant
(`agent-conductor.config.json`) is accepted for repos that don't want
a TS step. Validation runs at daemon start — invalid config exits
non-zero with a pointed message.

#### Stage-1 (in Phase 1, no new contract)

- Daemon lives at `tools/agent-conductor/` inside xvision.
- Config file `agent-conductor.config.ts` is committed at the xvision
  repo root.
- Daemon source has zero xvision references (enforced by lint).
- All four Phase-1 contracts already produced are consistent with this
  boundary (`agent-cicd-board-schema`, `migrate-board`,
  `daemon-skeleton`, `shadow-run`) — see `daemon-skeleton` for the
  config-loading + lint requirements.

#### Stage-2 (Phase 2 contract: `agent-cicd-extract-package`)

- `git mv tools/agent-conductor/` → new repo
  (`github.com/latentwill/agent-conductor` or similar).
- Publish to npm as `agent-conductor`.
- Add `npx agent-conductor init` scaffolder that drops:
  - `agent-conductor.config.ts` stub with sensible defaults + comments.
  - `<configured boardSources>` minimal example if missing.
  - `<configured schemaPath>` copy of `board.schema.json`.
  - `.github/projects/agent-conductor-board.md` setup doc.
  - `launchd/com.<repo>.agent-conductor.plist` (macOS) or
    `systemd/agent-conductor.service` (linux) template.
- xvision switches from in-tree daemon to `npm i -g agent-conductor`,
  keeping only its `agent-conductor.config.ts` + the schema + the
  Project setup doc in-repo.
- Standalone repo gets its own CI: typecheck, unit tests, lint
  (including the no-host-repo-references rule generalized to "no
  repo-specific terms in core/").
- Versioning: semver. Major bump on config schema break.

#### Why not extract in Phase 1?

Two-repo work before the first repo runs the daemon successfully
produces a polished abstraction over guesswork. xvision is the
reference consumer; the config shape is locked by what xvision needs.
Once it's real and stable, extraction is mechanical.

## Phasing

The trap is automating everything at once. Each phase delivers value on
its own and the next phase only starts when the previous is steady.

### Phase 1 — Board + worktree automation, manual review routing

Goal: kill the worst toil (worktree setup + board edits) while leaving the
review loop semi-manual.

Deliverables:

- `team/schema/board.schema.json` — JSON Schema for a task.
- GH Projects v2 board mirroring current `team/board.md`. One-time
  migration script: read existing markdown, create issues, attach to
  project, set fields.
- `tools/agent-conductor/` skeleton: poll board, do CLAIMED/CODING/PR_OPEN
  transitions, never auto-merge. Idempotent. Single binary / script.
- Auto-create worktree on CLAIM, auto-launch Claude Code with a prompt
  built from the contract.
- Auto-open PR with contract linked + labels.
- Auto-archive worktree on MERGED.
- Codex review trigger via existing GH Action; review *routing* still
  manual (operator paste). The daemon collects review comments and
  surfaces them in a digest, but does not yet write the fix prompt
  comment.

Exit criteria: operator can create an Issue, set lane + track, and walk
away until a PR appears for review. Worktree management has zero manual
steps.

### Phase 2 — Review-loop automation + package extraction

Goal: close the copy-paste gap *and* extract the daemon so it can run
in any repo.

Deliverables:

- Daemon implements `CHANGES_REQUESTED → FIXING`: pulls PR review
  comments, generates the structured fix prompt, posts the
  "Review findings to address" comment, and resumes the worker.
- Daemon implements `READY` dependency resolution against OWNERSHIP /
  CONFLICT_ZONES so it never claims a colliding pair of tasks.
- Stale-task surfacing: any task >24h in `READY` or >2h in `REVIEWING`
  shows up in a daily digest.
- **Package extraction**: `tools/agent-conductor/` moves to its own
  repo and is published as the `agent-conductor` npm package, with
  `npx agent-conductor init` scaffolding for second-repo adoption.
  xvision becomes a consumer of the published package; only the
  config + schema + Project setup doc remain in xvision.

Exit criteria: (a) an Issue can move from `READY` to `APPROVED` without
the operator touching a worker shell, if one review cycle resolves it.
(b) A second repo can install the published package and reach
`READY → PR_OPEN` automation with only config edits.

### Phase 3 — CI gates + self-hosted runner + deploy

Goal: close the loop to production.

Deliverables:

- All required checks on `main` (above) wired and enforced.
- MacBook registered as self-hosted runner.
- Deploy workflow on push-to-`main`: local build → SSH transfer →
  compose-up → healthcheck → rollback on failure.
- Optional: GitHub merge queue enabled if PR cadence justifies it
  (≥4 mergeable PRs sitting simultaneously >24h, per the overhaul-spec
  threshold).
- Daemon writes `DEPLOYED` / `rolled_back` status back to the Issue.

Exit criteria: a merged PR results in a running container with the
expected SHA on the deploy host, without operator intervention,
99% of the time. The 1% case (rollback) is observable and recoverable
in <5 minutes.

## Verification per phase

Each phase has an acceptance contract under
`team/contracts/agent-cicd-phase-<N>.md` when work begins. Specifically:

- **Phase 1 acceptance**: 5 consecutive tasks routed end-to-end (READY
  → MERGED) with zero manual worktree commands and zero manual board edits.
- **Phase 2 acceptance**: 5 consecutive tasks with at least one
  CHANGES_REQUESTED cycle, all resolved without operator paste.
- **Phase 3 acceptance**: 5 consecutive merges to `main` deploy
  successfully, plus one induced failure that rolls back cleanly.

## Migration / first moves

1. Lock in this spec and break it into Phase-1 contracts.
2. Create `team/schema/board.schema.json` and the GH Project v2.
3. Build the migration script `tools/agent-conductor/scripts/migrate-board.ts`
   that reads `team/board.md` + `team/board-v2.md` and creates issues +
   project items. **Do not delete the markdown boards** until the daemon
   has run a full week without drift.
4. Author `tools/agent-conductor/` Phase-1 scaffolding behind a feature
   flag (`AGENT_CONDUCTOR_ENABLE=1`) so the daemon can be turned off if it
   misbehaves without yanking the schema.
5. Run Phase 1 in shadow mode for one cohort: daemon reads, suggests
   transitions, operator confirms. After one cohort of agreement, flip to
   live transitions.

## Risks and pre-mitigations

| Risk | Mitigation |
|---|---|
| Daemon claims a task into a dirty worktree | Daemon refuses to claim if `.worktrees/<track>` exists; surfaces to operator |
| Codex review flakes / hits rate limit mid-loop | Daemon retries with exponential backoff; surfaces after 3 failures |
| Two daemons running simultaneously (operator forgot to stop one) | Local file lock at `~/.cache/agent-conductor/lock`; refuses to start if held |
| Review comments are about scope drift, not fixable on-branch | Daemon detects `scope-drift` label on a review comment; transitions to a new `BACKLOG` task rather than `FIXING` |
| Self-hosted runner host (MacBook) sleeps | Runner registered with `caffeinate` keepalive during a job; queued jobs persist |
| Deploy fails partway through | Healthcheck gates the `DEPLOYED` transition; rollback path is mandatory in workflow |
| State desync between GH and daemon | Truth is GH. Daemon recomputes from observed state every poll; never trusts cached transitions |

## Decisions (resolved 2026-05-18)

The five intake questions are now decided. Phase-1 contracts can be cut
against these.

1. **Worker engine**: Claude Code (current). No engine adapter — workers
   launch as `claude` sessions directly. If a future swap to Cline SDK
   happens, it's a follow-up spec, not a hedge we build in now.
2. **Healthcheck**: `GET /healthz` on the deploy host returns
   `{"sha": "<git-sha>", "uptime_s": <int>, "ok": <bool>}`. The deploy
   workflow waits for `ok=true` and `sha` matching the just-built image
   digest; mismatch or `ok=false` triggers rollback. Dependency-level
   checks (DB / broker / LLM) are out of scope for v1 — add them only
   when a real outage motivates it.
3. **Notifications**: local stdout + digest file at
   `~/.cache/agent-conductor/digest-<date>.md`. No external webhook in
   v1. The daemon tails recent state to stdout for an attached terminal
   and rotates the digest file daily. Adding a Slack/Discord sink is a
   one-line addition later if the digest pattern isn't enough.
4. **Merge authority**: any maintainer with write access. The audit
   gate lives in branch protection (required Codex + Claude reviews,
   required checks, rebase-on-main), not in a human merge chokepoint.
   The daemon never merges in v1; Phase-3 may add an `auto-merge` label
   gate, but that's deferred until the pipeline is otherwise stable.
5. **Naming**: `agent-conductor`. Lives at `tools/agent-conductor/`.
   Not folded into the `xvn` CLI brand — it's a control plane, not a
   user-facing CLI verb. The human conductor role in
   `team/CONDUCTOR.md` is distinct; daemon docs should always say
   "agent-conductor" in full to avoid collision.

## Related

- `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`
  — the contracts/conductor/queue substrate this builds on
- `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md`
  — agent-run engine that emits the worker traces
- `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md`
  — possible worker engine swap
- `CLAUDE.md` deployment guardrails — local image build is the preferred
  deploy path; GHCR fallback only
