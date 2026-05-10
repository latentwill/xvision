# xvision Docker image

The `xvision:dev` image bundles the `xvn` CLI (default-members of the
xvision Cargo workspace) on top of `debian:bookworm-slim`.

## Tags

| Tag | Description |
|---|---|
| `ghcr.io/latentwill/xvision:latest` | Default. Excludes `xvision-identity`. |
| `ghcr.io/latentwill/xvision:identity` | Includes the alloy v2 / on-chain identity stack. Larger build, same binary. |
| `ghcr.io/latentwill/xvision:<git-sha>` | Immutable SHA tag for every push to `main`. |

## Quick start

```bash
docker pull ghcr.io/latentwill/xvision:latest
docker run --rm ghcr.io/latentwill/xvision:latest --version
```

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
docker compose build
docker compose run --rm xvn --help
docker compose --profile identity build xvn-identity
```

## Building locally

```bash
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
DOCKER_BUILDKIT=1 docker build --build-arg WITH_IDENTITY=1 -t xvision:dev-identity .
```

## Troubleshooting

- **`error while loading shared libraries: libsqlite3.so.0`** — runtime base is
  missing libsqlite3. Make sure the runtime stage `apt-get install`s
  `libsqlite3-0` (the Dockerfile in this repo does).
- **TLS errors hitting Alpaca/Orderly** — confirm `ca-certificates` is
  installed in the runtime stage.
- **Image is huge (>500 MB)** — the most common cause is a missing
  `.dockerignore` letting `target/` or `models/` into the build context.
