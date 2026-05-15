# Deploy — GHCR + tailscale stacks on extndly-dev

How `xvn.tail2bb69.ts.net` and `xvnej.tail2bb69.ts.net` are deployed, and the pitfalls that have actually bitten.

## Image build

GHCR image: `ghcr.io/latentwill/xvision:latest` (and `:identity` for the WITH_IDENTITY=1 variant).

Workflow: `.github/workflows/docker.yml`. Triggers — tag push `v*.*.*` OR `workflow_dispatch`. **Does not** auto-trigger on push-to-main (locked down 2026-05-10 to save Actions minutes).

To rebuild `:latest`:

```bash
gh workflow run docker.yml --ref main -f dockerfile=Dockerfile.deploy -f build_identity=false
```

`Dockerfile.deploy` is the SPA-baked variant; plain `Dockerfile` is CLI-only. The workflow defaults the `file:` fallback to `Dockerfile.deploy` for both jobs (set in PR #73).

Verify a fresh image has the SPA embedded:

```bash
docker run --rm --entrypoint /usr/bin/grep ghcr.io/latentwill/xvision:latest \
  -aoE "assets/index-[A-Za-z0-9_]{6,}\.(js|css)" /usr/local/bin/xvn | sort -u
```

## Stacks on extndly-dev

Two near-identical compose stacks at `/root/deploy/stacks/{xvn,xvnej}/`:

- `xvn/` → personal node, `https://xvn.tail2bb69.ts.net`
- `xvnej/` → QA node, `https://xvnej.tail2bb69.ts.net`

Each stack: `ts-{xvn,xvnej}` (tailscale sidecar, runs `tailscale serve` per `serve.json`) + `{xvn,xvnej}-app` (the xvision image, listens on `:8788`). App shares the sidecar's netns.

## Local image deploy

For local-image deploys, no Docker registry auth is required. Build the image
locally, then stream it over SSH to the host Docker daemon.

Before deploying, load the standard dev-server key into the local SSH
agent/keychain:

```bash
ssh-add --apple-use-keychain ~/.ssh/id_ed25519
ssh -i ~/.ssh/id_ed25519 -o IdentitiesOnly=yes root@100.120.48.1 'echo ok $(hostname)'
```

Then send the image:

```bash
scripts/deploy-image.sh --push root@100.120.48.1
```

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

## Remote control path

Live-node agent control is tailscale-only in the current design. Use:

- `https://xvn.tail2bb69.ts.net`
- `https://xvnej.tail2bb69.ts.net`

The preferred remote path is the dashboard-served typed `xvn` job surface, not
generic SSH orchestration. See
`docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md` for the
remote CLI contract and follow-up security boundary.

## Pitfalls (each bit at least once)

- **Netns dependency**: if the `ts-*` sidecar restarts (or you `docker compose restart` it), dependent containers using `network_mode: service:ts-*` lose their netns. Always `--force-recreate` the app afterwards.
- **Cert lazy-fetch hangs**: `tailscale serve` lazy-provisions the `*.ts.net` cert on first request, sometimes hanging with `TLS internal_error`. Force issuance: `docker exec ts-<name> tailscale cert <dnsname>`.
- **Auth via URL is fragile**: the tailscale healthcheck recycles the container and rotates the interactive auth URL. Use a `TS_AUTHKEY` from <https://login.tailscale.com/admin/settings/keys> written to `.env`.
- **GHCR push**: needs a classic PAT (`ghp_…`), not a fine-grained `github_pat_…`. The fine-grained ones silently fail. Use 1Password `Olympus / Github Classic Token (No Admin/Delete)`.
- **Workflow-file pushes**: the default `gh` auth on extndly-dev has only `repo, read:org, gist` — no `workflow` scope. To push `.github/workflows/*.yml` changes, use the classic PAT directly: `git push https://x-access-token:$PAT@github.com/...`.
- **RAM**: extndly-dev is 3.7 GiB. xvision Rust builds OOM the box. Always build via GHCR; never `cargo build` locally.
- **The rust-embed route lookup bug** (PR #75): `#[derive(RustEmbed)] #[folder = "static/"]` keys files relative to the folder (e.g. `assets/foo.js`). Axum's `Path<String>` extractor on `/assets/*path` strips the prefix, so `serve_static` must reattach `assets/` before calling `Assets::get`. Don't undo this.
