---
from: conductor
to: all
topic: deploy-prep
created_at: 2026-05-17T07:00:00Z
ack_required: false
---

# Deploy image rollout — #207 + #208 merged, ready to ship

## State 2026-05-17 (post-merge)

Both blockers are on `main`:

1. **#207** merged 2026-05-17 02:27 UTC (observability bus drops-oldest fix).
2. **#208** merged 2026-05-17 02:28 UTC (Cline SDK Wave 1+2); merge commit
   `7365cc57f592f121283e9ced60220234a8bea980`.

PR #199 (draft cline-sdk spec) closed as superseded. Merged branches
deleted on origin: `task/observability-review-fixes`,
`task/cline-sdk-wave1-impl`.

## Run the deploy

1. `git fetch --prune origin && git checkout main && git pull --ff-only`
2. Run the deploy-image preferred path locally on the build host (NOT on a
   deploy host — see `CLAUDE.md` deployment guardrails):

   ```bash
   scripts/deploy-image.sh                          # build only
   scripts/deploy-image.sh --push root@<host>       # build + transfer
   ```

3. Verify the running container's image digest matches the digest just
   built/pushed.
4. Smoke-check the sidecar at runtime (deferred verification on #208):

   ```bash
   docker run --rm <tag> node /opt/xvision-agentd/dist/index.js --version
   # expect cline_sdk_version: "0.0.41"
   ```

5. On the live tailnet nodes (`xvn.tail2bb69.ts.net`,
   `xvnej.tail2bb69.ts.net`): `docker compose pull && docker compose up -d
   --force-recreate` after the image lands. App shares netns with the
   tailscale sidecar — if `ts-*` restarts, `--force-recreate` the app too.

## Do not

- Do not run `cargo build|test|check` on the remote/deploy hosts (Rust
  toolchain not installed; OOM risk on 4 GB VPS).
- Do not build Docker images on the deploy hosts.
- Do not push `workflow_dispatch` runs with the default `gh` auth on
  `extndly-dev` (lacks `workflow` scope) — use the 1Password classic PAT
  if a GHCR fallback is needed.
