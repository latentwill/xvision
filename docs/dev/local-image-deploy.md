# Local Image Deploy

Use this path when you want to deploy xvision without paying the GHCR/GitHub
Actions cost for every test image.

## When to use it

- Use `scripts/deploy-image.sh` for fast dev/staging deploys from a trusted
  build machine.
- Use GHCR (`scripts/deploy-ghcr.sh`) when multiple servers need to pull the
  same immutable image, or when the image must be reproducible from GitHub
  Actions logs.
- Do not run `cargo` or production image builds on small deploy hosts. Build on
  the local/control machine and send only the finished runtime image.

## Build only

```bash
cd /Users/edkennedy/Code/xvision
scripts/deploy-image.sh
```

This builds `Dockerfile.deploy`, embeds the Vite dashboard, and tags the local
image as both `xvision:deploy-<sha>` and `xvision:deploy-latest`.

## Build and push over SSH

Before pushing, make sure the standard dev-server SSH key is loaded into the
local SSH agent/keychain:

```bash
ssh-add --apple-use-keychain ~/.ssh/id_ed25519
```

If that succeeds, verify the host before starting the image transfer:

```bash
ssh -i ~/.ssh/id_ed25519 -o IdentitiesOnly=yes root@100.120.48.1 'echo ok $(hostname)'
```

This local-image path does not need Docker registry auth. It only needs access
to the local Docker daemon and SSH access to the target host.

```bash
# Most VPS hosts are amd64.
scripts/deploy-image.sh --push root@your-server

# ARM servers need an explicit platform.
scripts/deploy-image.sh --push root@your-server --platform linux/arm64
```

The script streams the built image over SSH with `docker save | gzip | docker
load`. No registry is involved. The remote host only needs Docker and SSH
access.

After the image is loaded, any Docker Compose or Coolify service that points at
`xvision:deploy-latest` must be recreated or redeployed so the running
container actually picks up the new image.

## Dashboard token for test deployments

The dashboard refuses to boot on a non-loopback bind such as
`0.0.0.0:8788` unless `XVN_DASHBOARD_TOKEN` is set. This also applies to the
current Tailscale test stacks, because the app container still binds inside the
sidecar network namespace on `0.0.0.0:8788`.

For local-only development, bind to `127.0.0.1:8788` and no token is required.
For SSH/Docker/Tailscale test deployments, put a generated value in the remote
stack's `.env`:

```bash
XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
```

Treat this as a temporary shared secret for the testing surface, not a finalized
user-auth model. Rotate it if it is pasted into logs or shared too broadly.

## Server compose image

Point the server's Compose file at the loaded image:

```yaml
services:
  xvn:
    image: xvision:deploy-latest
    ports:
      - "8788:8788"
    environment:
      XVN_AUTOMIGRATE: "1"
      XVN_DASHBOARD_TOKEN: ${XVN_DASHBOARD_TOKEN}
      XVN_DATA_DIR: /data
      XVN_HOME: /data
    volumes:
      - xvision-data:/data
```

Restart on the server:

```bash
docker compose up -d
docker compose logs -f xvn
```

For Coolify-managed apps, use the Coolify redeploy action instead of
`docker compose up -d`, but keep the image tag pointed at
`xvision:deploy-latest`.

## Notes

- The local `cargo build --release` binary is a macOS dev binary. It proves the
  Rust workspace builds, but it is not the deploy artifact for Linux servers.
- `scripts/deploy-image.sh` builds a Linux container image locally and sends
  that image to the server.
- Cross-building `linux/amd64` from Apple Silicon uses Docker buildx/QEMU and
  can be slower than a native build. Use `--platform linux/arm64` only when the
  server is ARM.
