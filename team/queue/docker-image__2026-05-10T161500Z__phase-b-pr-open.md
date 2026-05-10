---
from: docker-image
to: all
topic: phase-b-pr-open
created_at: 2026-05-10T16:15:00Z
ack_required: false
---

# Docker image — PR #6 open

PR: https://github.com/latentwill/xvision/pull/6
Branch: `feature/docker-image`
Worktree: `.worktrees/docker-image`

## What landed

All file-creation tasks of `docs/superpowers/plans/2026-05-10-docker-image.md`:

1. `.dockerignore` — keeps build context tiny (drops `target/`, `models/`, `data/`, `notebooks/`, `docs/`)
2. `docker/entrypoint.sh` — bootstraps `/data`, optional `xvn store migrate` via `XVN_AUTOMIGRATE=1`, then `exec xvn`
3. `Dockerfile` — three-stage (cargo-chef plan → cook → build → debian:bookworm-slim runtime) with `WITH_IDENTITY=0|1` build arg
4. `docker-compose.yml` + `.env.example` — local dev convenience with named `data` volume
5. `docker/README.md` — operator guide (tags, mounts, env vars, troubleshooting)
6. `.github/workflows/docker.yml` — `build-default`, `build-identity`, `smoke` jobs publishing to `ghcr.io/latentwill/xvision`
7. `CLAUDE.md` — `## Docker` section pointing at the image

## Verification status

- `cargo build --workspace` — green on baseline (no Rust changes)
- workflow YAML parses (`python3 -c 'import yaml; yaml.safe_load(...)'`)
- `docker/entrypoint.sh` mode `100755` recorded in git index
- **Local docker not installed** → plan Tasks 4 / 5 / 10 (smoke checks against built images) deferred to CI. The `docker / build-default` PR check is the verification authority for whether the Dockerfile builds end-to-end.

## What downstream consumers get on merge

- `ghcr.io/latentwill/xvision:latest` — default-members; no on-chain identity
- `ghcr.io/latentwill/xvision:identity` — workspace build with `xvision-identity`
- `ghcr.io/latentwill/xvision:<git-sha>` — immutable per-commit tag
- `docker compose run --rm xvn …` for local dev convenience
- The `smoke` job re-runs `xvn --version` + `xvn store stats --db /data/store.db` against the published `:latest` tag after every merge to `main`

## QA usage

```bash
docker pull ghcr.io/latentwill/xvision:latest
docker run --rm \
  -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  --env-file .env \
  ghcr.io/latentwill/xvision:latest \
  store stats --db /data/store.db
```

See `docker/README.md` for the full env-var / mount reference.

## No file collisions

This PR does not touch:
- `crates/xvision-engine/` (engine-api PR #4)
- `crates/xvision-execution/` (broker-surface in flight)
- `crates/xvision-dashboard/`, `frontend/web/`, or `crates/xvision-cli/commands/dashboard.rs` (frontend-foundation in flight)

It modifies `team/MANIFEST.md` (added B.11 row + corrected frontend-foundation owner). If another track has a conflicting MANIFEST edit, rebase order is at coordinator's discretion.
