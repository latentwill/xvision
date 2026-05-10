---
from: docker-image
to: all
topic: claim
created_at: 2026-05-10T16:00:00Z
ack_required: false
---

# `docker-image` track claimed (third parallel CLI)

A third Claude CLI session is taking the `docker-image` track. Worktree
`.worktrees/docker-image`, branch `feature/docker-image`. Plan:
[`docs/superpowers/plans/2026-05-10-docker-image.md`](../../docs/superpowers/plans/2026-05-10-docker-image.md).

## Why now

User scope is "v1 deployment for QA & testing." A reproducible image of `xvn`
on GHCR is the QA distribution channel. This plan is independent of all three
Phase A tracks (`engine-api`, `frontend-foundation`, `broker-surface`) — the
image just packages whatever `xvn` binary builds from `default-members` of the
workspace. As Phase A tracks merge to `main`, the next image build picks them
up automatically.

## Files this track touches (no overlap with active tracks)

- `Dockerfile` (new)
- `.dockerignore` (new)
- `docker/entrypoint.sh` (new)
- `docker/README.md` (new)
- `docker-compose.yml` (new)
- `.env.example` (new)
- `.gitignore` (append `.env` if missing — already present per current state)
- `.github/workflows/docker.yml` (new)
- `CLAUDE.md` (append `## Docker` section after `## Build & test`)
- `team/MANIFEST.md` (append docker-image row)
- `team/status/docker-image.md` (new)

None of these collide with engine-api (`crates/xvision-engine/`),
broker-surface (`crates/xvision-execution/`), or frontend-foundation
(`crates/xvision-dashboard/`, `frontend/web/`, `crates/xvision-cli/` Dashboard
subcommand).

## Local docker not installed

`docker` is not on this machine. CI (the new workflow added in plan Task 8)
runs the actual `docker build` and smoke checks on PR open and on merge to
`main`. Local-only verification steps in the plan (Tasks 4, 5, 10) are
documented-as-skipped in the PR description; CI is the verification authority.

## What downstream consumers get on merge

- `ghcr.io/latentwill/xvision:latest` (default-members; no on-chain identity)
- `ghcr.io/latentwill/xvision:identity` (workspace build with xvision-identity)
- `ghcr.io/latentwill/xvision:<git-sha>` (immutable per-commit tags)
- `docker compose run --rm xvn …` for local dev convenience
- `docker/README.md` operator guide for env vars and mounts

PR opens shortly. Watching the queue for cross-track messages.
