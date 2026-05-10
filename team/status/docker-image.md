---
track: docker-image
worktree: /Users/edkennedy/Code/xvision/.worktrees/docker-image
branch: feature/docker-image
phase: phase-b-docker-image
last_updated: 2026-05-10T16:00:00Z
owner: claude-opus session 3 (docker-image track)
---

# What I'm doing right now

Picking up the docker-image plan
(`docs/superpowers/plans/2026-05-10-docker-image.md`). Goal is a reproducible
slim image of `xvn` published to GHCR for v1 QA & testing. The plan is
independent of all three Phase A tracks — image just packages
`default-members` of the workspace.

Local `docker` is not installed; CI added in plan Task 8 is the verification
authority. Local-only verification steps (plan Tasks 4, 5, 10) deferred to CI
and documented in the PR description.

## Plan task progress

- [ ] T1: `.dockerignore`
- [ ] T2: `docker/entrypoint.sh`
- [ ] T3: `Dockerfile` (multi-stage, chef + builder + runtime)
- [ ] T6: `docker-compose.yml` + `.env.example`
- [ ] T7: `docker/README.md`
- [ ] T8: `.github/workflows/docker.yml`
- [ ] T9: `CLAUDE.md` `## Docker` section
- [ ] PR open → CI green on `docker / build-default`

# Blocked on

Nothing. Plan is independent of Phase A.

# Next up

1. Land all file-creation tasks as commits on `feature/docker-image`.
2. Open PR `build(docker): slim runtime image + GHCR publish workflow`.
3. Confirm `docker / build-default` job goes green in PR CI.
4. Post `docker-image__<utc>__phase-b-pr-open.md` to queue.
