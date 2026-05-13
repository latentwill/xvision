---
track: ghcr-build-optimization
worktree: /root/deploy/xvision/.worktrees/ghcr-build-optimization
branch: ghcr-build-optimization
phase: committed
last_updated: 2026-05-13T02:05:00Z
owner: codex
---

# What changed

Picked up the execution-board track for GHCR build optimization. The planned
staging-profile, Docker build-arg, workflow preflight, profile-scoped cache,
current-image smoke, and Docker context hygiene pieces are present. The branch
also contains the remaining implementation slice over `origin/main`: the Docker
preflight installs pnpm before `actions/setup-node` enables pnpm caching.

# Verification

- `git diff --check` passed.
- `git diff --check origin/main...HEAD` passed.
- `python3 -c 'import yaml; yaml.safe_load(...)'` parsed `.github/workflows/docker.yml`.

# Blocked on

Full Rust and Docker build verification is not run on this deploy host per
`CLAUDE.md`; GitHub Actions is the authority for those checks. Ruby is not
installed here, so the plan's Ruby YAML parse command was substituted with the
available Python YAML parser.

# CI-only follow-up

Dispatch the workflow with explicit inputs when remote confirmation is needed:

```bash
scripts/deploy-ghcr.sh --ref ghcr-build-optimization --build-profile staging --no-watch
```
