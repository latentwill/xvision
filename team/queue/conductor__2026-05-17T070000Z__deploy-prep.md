---
from: conductor
to: all
topic: deploy-prep
created_at: 2026-05-17T07:00:00Z
ack_required: false
---

# Deploy image rollout — waiting on #208 + #207

## State 2026-05-17 (conductor walk-through)

Two PRs are blocking the next image push:

1. **#208 — Cline SDK Wave 1+2** (`task/cline-sdk-wave1-impl`,
   55 files, +11.9k). `mergeable: MERGEABLE`, no CI on this repo to gate.
   Verification commands listed on
   `team/contracts/cline-sdk-wave1-2.md`.
2. **#207 — Observability bus drops-oldest fix** (`task/observability-review-fixes`,
   3 files, +590/-123). Fix-forward on Phase A bus.rs. Verification commands
   on `team/contracts/observability-review-fixes.md`.

PR #199 (draft cline-sdk spec) is superseded by #208. Close after #208 merges.

## Order

Land #207 first (small, surgical, no overlap with #208). Then #208 (touches
xvision-agentd/ + xvision-agent-client/ — orthogonal to #207's
xvision-observability/src/bus.rs). No rebase conflict expected.

## After both merge

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
