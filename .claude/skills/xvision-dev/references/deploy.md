# xvision deploy (contributor view)

Two paths exist. **Local image build is preferred.** GHCR is reserved for
remote deployment when no local build host is available.

## Preferred — local image build, ship over SSH

Builds the Rust workspace + Vite SPA on the local build host and transfers the
~150 MB runtime image to the target. Avoids GitHub Actions minutes and the
OOM trap on the 3.7 GiB `extndly-dev` host.

```bash
scripts/deploy-image.sh                          # build only, tag xvision:deploy-<sha>
scripts/deploy-image.sh --push root@host         # build + transfer + docker load
scripts/deploy-image.sh --with-identity          # include xvision-identity (Mantle)
scripts/deploy-image.sh --platform linux/arm64   # ARM hosts (Graviton, Oracle ARM)
```

The target host only needs `docker`. After the image lands, the consuming
service (Compose, Coolify) must be recreated/redeployed so the running
container switches to the new image.

`--push` is bracketed by a remote-disk preflight and a post-load
cleanup (`scripts/deploy-image.sh`, commit `8fd7d48`, PR #377):

- **Preflight**: refuse at ≥95% root-fs used (warn at ≥85%). Driven
  by the 2026-05-20 incident where deploy succeeded at image-load
  but `xvn-app` entered a restart loop because `/` was at 100% and
  SQLite couldn't write the migration journal. The error message
  prints the standard reclaim targets: `journalctl --vacuum-size`,
  `docker image prune -f`, inspecting `docker images xvision`, and
  removing stale `/root/deploy/xvision/.worktrees/*/target/` trees.
- **Cleanup**: drop the prior `:deploy-latest` sha tag iff it points
  at a different image than the one just loaded **and** no
  container still references it. Other `xvision:*` tags (including
  whatever `xvnej-app` is pinned to and `ghcr.io/*` mirrors) are
  untouched.

Env overrides:

```
XVN_DEPLOY_DISK_REFUSE_PCT=95      # default
XVN_DEPLOY_DISK_WARN_PCT=85        # default
XVN_DEPLOY_SKIP_DISK_CHECK=1       # bypass preflight
XVN_DEPLOY_SKIP_CLEANUP=1          # keep prior :deploy-latest tag
```

Before pushing, load the dev-server key into the local SSH agent/keychain:

```bash
ssh-add --apple-use-keychain ~/.ssh/id_ed25519
ssh -i ~/.ssh/id_ed25519 -o IdentitiesOnly=yes root@100.120.48.1 'echo ok $(hostname)'
```

## Fallback — GHCR via GitHub Actions

Use only when no local build host can run the full Rust+Vite build (e.g.
iterating on a remote dev box with no Docker locally).

```bash
scripts/deploy-ghcr.sh
# or directly:
gh workflow run docker.yml --ref main \
  -f dockerfile=Dockerfile.deploy \
  -f build_identity=false \
  -f build_profile=release
```

Workflow: `.github/workflows/docker.yml`. Triggers — tag push `v*.*.*` OR
`workflow_dispatch`. **Does not** auto-trigger on push-to-main (locked down
2026-05-10 to save Actions minutes).

Verify a fresh image has the SPA embedded:

```bash
docker run --rm --entrypoint /usr/bin/grep ghcr.io/latentwill/xvision:latest \
  -aoE "assets/index-[A-Za-z0-9_]{6,}\.(js|css)" /usr/local/bin/xvn | sort -u
```

## Stacks on extndly-dev

Two near-identical compose stacks at `/root/deploy/stacks/{xvn,xvnej}/`:

- `xvn/` → personal node, `https://xvn.tail2bb69.ts.net`
- `xvnej/` → QA node, `https://xvnej.tail2bb69.ts.net`

Each stack: `ts-{xvn,xvnej}` (tailscale sidecar, runs `tailscale serve` per
`serve.json`) + `{xvn,xvnej}-app` (the xvision image on `:8788`). The app
shares the sidecar's netns.

Redeploy:

```bash
cd /root/deploy/stacks/{xvn,xvnej}
docker compose pull
docker compose up -d --force-recreate {xvn,xvnej}
```

Smoke:

```bash
for h in xvn.tail2bb69.ts.net xvnej.tail2bb69.ts.net; do
  curl -sk -o /dev/null -w "$h /api/health: %{http_code}\n" "https://$h/api/health"
done
```

## Rules that apply to both paths

- On remote/deploy hosts (small VPS, Coolify nodes), **never** run `cargo`,
  `cargo build`, `cargo check`, or `cargo test`. No toolchain installed; the
  3.7 GiB box OOMs.
- On remote/deploy hosts, **never** build Docker images. Builds happen
  locally or in GHCR.
- **Always** `source .op_env` before using `gh` or `op`.
- **Always** verify rollout by checking the running container's image digest
  matches the digest you just built (local) or published (GHCR).
- These rules **don't** apply to local dev workstations running normal
  `cargo test` / `docker compose build`.

## Pitfalls (each has bitten before)

- **Netns dependency**: if the `ts-*` sidecar restarts (or you
  `docker compose restart` it), dependent containers using
  `network_mode: service:ts-*` lose their netns. Always `--force-recreate`
  the app afterwards.
- **Cert lazy-fetch hangs**: `tailscale serve` lazy-provisions the `*.ts.net`
  cert on first request, sometimes hanging with `TLS internal_error`. Force
  issuance: `docker exec ts-<name> tailscale cert <dnsname>`.
- **Auth via URL is fragile**: the tailscale healthcheck recycles the
  container and rotates the interactive auth URL. Use a `TS_AUTHKEY` from
  <https://login.tailscale.com/admin/settings/keys> written to `.env`.
- **GHCR push**: needs a classic PAT (`ghp_…`), not a fine-grained
  `github_pat_…`. The fine-grained ones silently fail. Use 1Password
  `Olympus / Github Classic Token (No Admin/Delete)`.
- **Workflow-file pushes**: the default `gh` auth on `extndly-dev` has only
  `repo, read:org, gist` — no `workflow` scope. To push `.github/workflows/*.yml`
  changes, use the classic PAT directly:
  `git push https://x-access-token:$PAT@github.com/...`.
- **RAM**: extndly-dev is 3.7 GiB. Rust builds OOM the box. Always build via
  the local-image path or GHCR; never `cargo build` on the host.
- **The rust-embed route lookup bug** (PR #75): `#[derive(RustEmbed)]
  #[folder = "static/"]` keys files relative to the folder (`assets/foo.js`).
  Axum's `Path<String>` extractor on `/assets/*path` strips the prefix, so
  `serve_static` must reattach `assets/` before calling `Assets::get`.
  Don't undo this.

## Image variants

- `ghcr.io/latentwill/xvision:latest` — default-members workspace, no
  on-chain identity stack.
- `ghcr.io/latentwill/xvision:identity` — `WITH_IDENTITY=1` variant, includes
  `xvision-identity` (Mantle/ERC-8004).

Tag policy: `:latest` tracks the most recent `v*.*.*` tag. Never push
`:latest` from a feature branch.
