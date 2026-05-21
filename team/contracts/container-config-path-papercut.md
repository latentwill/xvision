---
track: container-config-path-papercut
lane: leaf
wave: eval-honesty-2026-05-21
worktree: .worktrees/container-config-path-papercut
branch: task/container-config-path-papercut
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - Dockerfile.deploy
  - docker/entrypoint.sh
  - team/contracts/container-config-path-papercut.md
forbidden_paths:
  - "crates/**"
  - "frontend/**"
  - "docker-compose*.yml"
interfaces_used: []
parallel_safe: true
parallel_conflicts: []
verification:
  - bash -n docker/entrypoint.sh
  - docker compose config --quiet
acceptance:
  - XVN_CONFIG_PATH is removed from the ENV block in Dockerfile.deploy
  - A comment in Dockerfile.deploy explains why XVN_CONFIG_PATH is intentionally absent
  - docker/entrypoint.sh has a CONTRACT comment block before the export line explaining the ownership contract
  - docker-exec sessions resolve config via XVN_HOME fallback to /data/config/default.toml (the writable file)
  - shell lint passes (bash -n docker/entrypoint.sh)
  - compose YAML validates (docker compose config --quiet)
---

# Scope

Remove the `XVN_CONFIG_PATH=/config/default.toml` hard-set from the
`Dockerfile.deploy` `ENV` block so that `docker exec` diagnostic sessions
see the same writable config the running dashboard uses. Previously, the
`ENV` directive baked the seed (read-only) path into the container image
env, which caused `xvn provider list` (and other CLI commands run via
`docker exec`) to read from `/config/default.toml` (empty providers) while
the dashboard read from `/data/config/default.toml` (configured providers).
With `XVN_CONFIG_PATH` absent from the image env, `docker exec` sessions fall
back through the CLI's lookup chain to `$XVN_HOME/config/default.toml` =
`/data/config/default.toml`, consistent with PID 1.

Implements `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`,
track `container-config-path-papercut`.

# Root cause clarification

The brief identified `docker-compose.yml` as the likely source. Inspection
shows `docker-compose.yml` does NOT set `XVN_CONFIG_PATH`. The actual source
is the `ENV` directive in `Dockerfile.deploy` line 162, which bakes
`XVN_CONFIG_PATH=/config/default.toml` into the container image. Every
process in the container — including `docker exec` sessions — inherits this
value from the image's configured env, overriding the entrypoint's `export`.

Fix: remove `XVN_CONFIG_PATH` from the `ENV` block in `Dockerfile.deploy`.
The entrypoint's `export XVN_CONFIG_PATH="$WRITABLE_CONFIG_PATH"` becomes
the sole setter, and `docker exec` sessions without the image env var fall
back to `$XVN_HOME/config/default.toml`.

# Out of scope

- docker-compose.yml and docker-compose.deploy.yml (neither sets XVN_CONFIG_PATH)
- The seeding logic in entrypoint.sh (correct; untouched)
- Other ENV vars (XVN_HOME, XVN_DATA_DIR, XVN_AUTOMIGRATE, etc.)
- Rust CLI config-path resolution (lookup order already correct: XVN_CONFIG_PATH > $XVN_HOME/config/default.toml)
- Frontend, migrations, identity compose files

# Sync-before-work ritual

```bash
git fetch --prune origin
git -C .worktrees/container-config-path-papercut status
git -C .worktrees/container-config-path-papercut log --oneline -3 origin/main..HEAD
```

# Test plan (manual — on next deploy)

No Rust tests required; the change is config-only and the CLI's lookup order
is already correct by inspection (`crates/xvision-cli/src/commands/provider.rs:110-116`).

Maintainer verification steps after deploying the new image to xvnej-app:

1. `docker exec xvnej-app cat /proc/1/environ | tr '\0' '\n' | grep XVN_CONFIG_PATH`
   - Before fix: `XVN_CONFIG_PATH=/config/default.toml`
   - After fix: key should be absent from PID 1 env (entrypoint sets it at runtime but it is not baked into the image env)

2. `docker exec xvnej-app xvn doctor | grep config_path`
   - Expected: `config_path   /data/config/default.toml`

3. `docker exec xvnej-app xvn provider list`
   - Expected: shows configured providers (e.g. `gemini-local`), same as the dashboard's Settings → Providers page.

# Notes

2026-05-21: Root cause found in Dockerfile.deploy ENV block, not docker-compose.yml.
Option A chosen (remove XVN_CONFIG_PATH entirely). CLI fallback chain
confirmed correct by reading provider.rs:runtime_config_path.
