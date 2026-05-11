# xvision Docker image

The published image bundles the `xvn` CLI (default-members of the xvision
Cargo workspace) on top of `debian:bookworm-slim`. The `:latest` tag is built
from **`Dockerfile.deploy`** and has the Vite SPA baked in, so
`xvn dashboard serve` works out of the box.

Two Dockerfiles live in the repo root:

- **`Dockerfile.deploy`** — runs a Node/pnpm/Vite stage first to emit
  `crates/xvision-dashboard/static/`, then the Rust stages. `CMD` defaults to
  `["dashboard", "serve", "--bind", "0.0.0.0:8788"]`. This is what GHCR
  publishes as `:latest` from `workflow_dispatch`. Use this for any deploy
  that needs the web UI.
- **`Dockerfile`** — CLI only, no frontend stage. `CMD` is `["--help"]`. Used
  by tag-triggered release builds when you want a slim image for CI / fixture
  generation / batch jobs that never touch the dashboard.

## Tags

| Tag | Description |
|---|---|
| `ghcr.io/latentwill/xvision:latest` | Default. SPA baked in. Built from `Dockerfile.deploy` on `workflow_dispatch`. |
| `ghcr.io/latentwill/xvision:main`   | Same as `:latest`, refreshed on every main-branch dispatch. |
| `ghcr.io/latentwill/xvision:sha-<short>` | Immutable per-build tag (e.g. `sha-0d09363`). |
| `ghcr.io/latentwill/xvision:identity` | Includes the alloy v2 / on-chain identity stack. Larger build. Opt-in via `workflow_dispatch` input. |

## Quick start

```bash
# Sanity check the CLI
docker pull ghcr.io/latentwill/xvision:latest
docker run --rm ghcr.io/latentwill/xvision:latest --version

# Run the dashboard (default CMD — open http://localhost:8788)
docker run --rm -p 8788:8788 \
  -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  ghcr.io/latentwill/xvision:latest
```

The image is private on GHCR (inherits the repo's visibility). On a fresh
host: `echo $GHCR_PAT | docker login ghcr.io -u <github-user> --password-stdin`
with a PAT scoped to `read:packages`. Or flip the package visibility public
at `https://github.com/users/<user>/packages/container/xvision/settings` if
you'd rather skip the login step.

## Mounts

| Path inside container | Purpose | Mode |
|---|---|---|
| `/data` | Sqlite store, traces, vectors. **Persist this.** | rw, named volume |
| `/config` | Baked-in copy of `config/*.toml`; mount your own to override | ro recommended |
| `/strategies` | Baked-in copy of the `strategies/` tree; mount to override | ro recommended |

## Environment variables

| Var | Purpose | Default |
|---|---|---|
| `XVN_AUTOMIGRATE` | If `1`, run `xvn store migrate` before exec | `0` |
| `XVN_DATA_DIR` | Override the data dir | `/data` |
| `XVN_CONFIG_DIR` | Override the config dir | `/config` |
| `ANTHROPIC_API_KEY` | Required for `dashboard serve` (wizard + chat-rail SSE) | none |
| `CREDENTIAL_SECRET` | 32-byte hex; encrypts persisted broker keys | none |
| `APCA_API_KEY_ID` / `APCA_API_SECRET_KEY` / `APCA_API_BASE_URL` | Alpaca paper creds | base URL defaults to paper |
| `ORDERLY_KEY` / `ORDERLY_SECRET` / `ORDERLY_ACCOUNT_ID` / `ORDERLY_BASE_URL` | Orderly testnet creds | none |
| `MANTLE_RPC_URL` / `MANTLE_DEPLOYER_KEY` | Mantle access (identity image only) | none |
| `RUST_LOG` | Tracing filter | unset |

Pull from 1Password (see `MANUAL.md` M5/M6/M7) and pass via `--env-file`:

```bash
op inject -i .env.tmpl -o .env
docker run --rm --env-file .env -v xvision-data:/data \
  ghcr.io/latentwill/xvision:latest store stats --db /data/store.db
```

## Local dev

```bash
# CLI-only image (the legacy Dockerfile, no dashboard)
docker compose build
docker compose run --rm xvn --help
docker compose --profile identity build xvn-identity
```

## Building locally

CLI-only (matches the legacy GHCR tags):

```bash
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
DOCKER_BUILDKIT=1 docker build --build-arg WITH_IDENTITY=1 -t xvision:dev-identity .
```

Deploy image (SPA baked in, defaults to `dashboard serve`):

```bash
# wraps `docker buildx build -f Dockerfile.deploy` and the optional ssh-load
./scripts/deploy-image.sh                           # build only
./scripts/deploy-image.sh --push root@server        # build + transfer
./scripts/deploy-image.sh --platform linux/arm64    # for ARM servers
```

The deploy image is what GHCR publishes as `:latest` from `gh workflow run
docker.yml`. Use the script when Actions credits are scarce or when you want
to skip the registry round-trip.

## Troubleshooting

- **`error while loading shared libraries: libsqlite3.so.0`** — runtime base is
  missing libsqlite3. Make sure the runtime stage `apt-get install`s
  `libsqlite3-0` (the Dockerfile in this repo does).
- **TLS errors hitting Alpaca/Orderly** — confirm `ca-certificates` is
  installed in the runtime stage.
- **Image is huge (>500 MB)** — the most common cause is a missing
  `.dockerignore` letting `target/` or `models/` into the build context.
