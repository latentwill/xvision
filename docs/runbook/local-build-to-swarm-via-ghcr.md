# Local-build → GHCR → swarm deploy

The validated recipe for shipping a fresh xvision image to xvn (dev) and
xvnej (prod) on the Hetzner swarm box. Canonical method as of
2026-05-24 — established the hard way over four iterations on
2026-05-23.

## Why this shape

The raw Mac↔swarm Tailscale link is bandwidth-capped at ~**77 KB/s**
(measured: `dd if=/dev/zero bs=1M count=20 | ssh -o Compression=no root@100.120.48.1 'cat >/dev/null'`
→ 4:19 for 20 MB). At that rate, a 375 MB image takes ~80 min one-way.
Three approaches that don't work:

- `scripts/deploy-image.sh --push` (raw `ssh host docker load`) — caps
  at single-stream window throughput, same ~30 KB/s.
- `docker -H ssh://root@…` transport — same wire bottleneck.
- Local registry on swarm + SSH forward tunnel — same wire bottleneck
  on push; reverse tunnel + Mac registry + swarm pull — same wire
  bottleneck on pull.

GHCR is the only path that bypasses the slow link: Mac↔GitHub and
swarm↔GitHub both have fast public-internet bandwidth, completely
independent of Tailscale.

## The recipe

### 0. PATH gotcha

Non-interactive shells (incl. the Claude Code task harness) don't pick
up `/usr/local/bin/docker`. Prefix every docker invocation with
`PATH="/usr/local/bin:$PATH"`, or set it once at the top of a
multi-command block.

### 1. Fresh worktree at origin/main (ignore dirty main checkout)

The main checkout almost always has WIP. Build from a clean snapshot
of `origin/main` instead.

```bash
git fetch origin
git worktree add /tmp/xvision-deploy-main origin/main
cd /tmp/xvision-deploy-main
git log -1 --format='HEAD %h %s'   # sanity check
```

### 2. Local build (build only — NO --push)

`scripts/deploy-image.sh` with no args builds + tags
`xvision:deploy-<sha>` and `xvision:deploy-latest`. Don't pass
`--push` — that triggers the slow Mac→swarm transfer. ~10–15 min wall
on Apple Silicon (cross-build linux/amd64 via qemu); most layers
cached on subsequent builds.

```bash
PATH="/usr/local/bin:$PATH" bash scripts/deploy-image.sh > /tmp/xvision-deploy.log 2>&1
tail -10 /tmp/xvision-deploy.log
```

**Pipeline-exit-code trap.** Don't pipe the script through
`tee | tail` — the pipeline's exit code becomes `tail`'s (always 0),
masking upstream build failures. The Claude Code task harness runs
commands under `/bin/zsh -c '…'`, where bash's `${PIPESTATUS[0]}` does
NOT work (zsh uses lowercase 1-indexed `$pipestatus[1]`). Cleanest fix:
skip the pipe entirely and redirect, as shown above. Caught 2026-05-24
when a broken commit on `origin/main` was "successfully" built twice
in a row and only failed at the `docker tag` step downstream.

### 3. Push to GHCR

Tag for GHCR and push both `:deploy-<sha>` (immutable) and `:main`
(what swarm pulls). Mac is already logged into GHCR
(`~/.docker/config.json`).

```bash
SHA=$(git rev-parse --short HEAD)   # e.g. 90e5721
PATH="/usr/local/bin:$PATH" docker tag xvision:deploy-${SHA} ghcr.io/latentwill/xvision:deploy-${SHA}
PATH="/usr/local/bin:$PATH" docker tag xvision:deploy-${SHA} ghcr.io/latentwill/xvision:main
PATH="/usr/local/bin:$PATH" docker push ghcr.io/latentwill/xvision:deploy-${SHA}
PATH="/usr/local/bin:$PATH" docker push ghcr.io/latentwill/xvision:main
# Both should print the same `digest: sha256:…` line.
```

Push typically takes 2–5 min depending on which layers are new.

### 4. Swarm pull + retag (no compose action yet)

Swarm already has GHCR creds at `/root/.docker/config.json`.

```bash
ssh root@100.120.48.1 "
  docker pull ghcr.io/latentwill/xvision:deploy-${SHA}
  docker tag ghcr.io/latentwill/xvision:deploy-${SHA} xvision:deploy-${SHA}
  docker tag ghcr.io/latentwill/xvision:deploy-${SHA} xvision:deploy-latest
"
```

Pull is fast (~30s–2min for full image, seconds for layer-cached
pulls).

### 5. Recreate stacks **separately** (dev first, prod second)

**Do NOT bundle dev + prod into one chained ssh command.** "Deploy to
xvn" historically means dev only; prod (xvnej) requires explicit
per-deploy authorization. The Claude Code auto-classifier enforces
this — bundled `docker compose up -d` against both stacks in one ssh
command will be denied. Staging avoids the denial AND gives you the
chance to verify dev before promoting.

```bash
# Dev first.
ssh root@100.120.48.1 "cd /root/deploy/stacks/xvn && docker compose up -d"
ssh root@100.120.48.1 "sleep 12 && docker ps --filter name=xvn-app --format 'table {{.Names}}\t{{.Status}}' && docker inspect xvn-app --format '{{.Image}} {{json .State.Health.Status}}'"

# Confirm xvn-app is `Up (healthy)` and the image id matches what was
# just pulled. Then prod — only after explicit go-ahead.
ssh root@100.120.48.1 "cd /root/deploy/stacks/xvnej && docker compose up -d"
ssh root@100.120.48.1 "sleep 12 && docker ps --filter name=xvnej-app --format 'table {{.Names}}\t{{.Status}}' && docker inspect xvnej-app --format '{{.Image}} {{json .State.Health.Status}}'"
```

Both `xvn-app` and `xvnej-app` should end at `Up (healthy)` with the
same image id (the linux/amd64 platform variant of the GHCR manifest
list digest you just pushed).

### 6. Cleanup

```bash
git worktree remove /tmp/xvision-deploy-main --force
rm -f /tmp/xvision-deploy.log /tmp/xvision-ghcr-push.log
```

Keep local `xvision:deploy-<sha>` tags around — they cost ~375 MB each
but layer-share, and the layer cache speeds up the next build
dramatically.

## Verification invariants

After step 5, all three of these should match:

- Manifest digest from `docker push` output: `sha256:<X>`
- Manifest digest from `ssh root@100.120.48.1 docker pull` output:
  `sha256:<X>` (same X)
- `docker inspect <container> --format '{{.Image}}'` on swarm: a
  `sha256:<Y>` that's the linux/amd64 platform variant — same content,
  different surface form. Cross-check by re-pulling and verifying no
  layer redownload.

If `xvn-app` reports `xvn 0.<N>.<M>` from its healthcheck log, that's
the version baked into the image and the smoke-test passed.

## Failure modes seen in practice

- **`scripts/deploy-image.sh --push` hangs at 13–28 KB/s.** That's the
  raw link; do not retry, switch to this GHCR path.
- **`docker push localhost:5000/…` fails with
  `dial tcp [::1]:5000: i/o timeout`.** `localhost` resolved to IPv6
  but the tunnel only listens on IPv4. Use `127.0.0.1:5000`.
- **`docker push 127.0.0.1:5000/…` fails with `connection refused`.**
  Docker Desktop's daemon runs in a VM where `127.0.0.1:5000` is the
  VM's loopback, not the Mac's. Either flip to a reverse tunnel with
  the registry on Mac, OR (better) just use GHCR.
- **Auto-classifier blocks bundled dev+prod ssh.** Split into two ssh
  commands; ask explicitly before recreating xvnej.
- **Build container OOMs on swarm.** Don't build on swarm. The Hetzner
  box is 4 GB and Rust workloads OOM. Build on Mac, ship via GHCR.

## Common variants

- **No new commits on `origin/main` since last deploy.** Skip
  everything — the currently-running image is already current.
- **GHCR build via Actions instead of local.** `bash scripts/deploy-ghcr.sh`
  (workflow_dispatch). Slower (~15–20 min) and burns Actions minutes.
  Only use when there's no working Docker on the build host.
- **Hot-swap latest tag only.** If you only need `:deploy-latest`
  swapped without bumping the sha pin in compose, the existing
  recreate flow above does that — both stacks pin
  `xvision:deploy-latest`.

## Related

- `docker/README.md` — image env vars, mounts, compose layout
- `scripts/deploy-image.sh` — the local build script (and the
  `--push` form that does NOT work for this host pair)
- `scripts/deploy-ghcr.sh` — the GHCR-build-via-Actions fallback
- `docs/runbook/dashboard-auth.md`, `docs/runbook/observability-otel.md`
  — sibling runbooks
