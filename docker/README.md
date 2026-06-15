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
| `ghcr.io/latentwill/xvision:0.36.0` | **Pinned release.** Published automatically from the `v0.36.0` git tag via `docker.yml` (semver pattern). Use a pinned `X.Y.Z` tag for reproducible deploys; the running container's `GET /api/version` must match the tag. Note: a tag push publishes `X.Y.Z` + `sha-<short>` but does **not** move `:latest` (that only refreshes on a default-branch dispatch). |
| `ghcr.io/latentwill/xvision:latest` | Default. SPA baked in. Built from `Dockerfile.deploy` on `workflow_dispatch`. Includes the on-chain identity/marketplace (alloy v2) stack via the dashboard's marketplace routes — env-gated at runtime (dormant/503 without `XVN_RPC_URL` etc.). |
| `ghcr.io/latentwill/xvision:main`   | Same as `:latest`, refreshed on every main-branch dispatch. |
| `ghcr.io/latentwill/xvision:sha-<short>` | Immutable per-build tag (e.g. `sha-0d09363`). |
| `ghcr.io/latentwill/xvision:identity` | Explicit full-identity build: `xvision-identity` as a direct workspace member (full CLI identity verbs, not just the dashboard routes). Larger build. Opt-in via `workflow_dispatch` input. |

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
| `/data` | `XVN_HOME`: `xvn.db`, strategies, config, secrets, traces, vectors. **Persist this.** | rw, named volume |
| `/config` | Baked-in copy of `config/*.toml`; mount your own to override | ro recommended |
| `/strategies` | Seed copy of the `strategies/` tree; copied into `/data/strategies` without overwriting existing files | ro recommended |

Packaged parquet probes from `data/probes/` are baked into the image under
`/opt/xvision/data/probes/` and copied into `/data/probes/` on container boot
without overwriting existing files. This keeps backtest fixture lookup tied to
scenario cache keys while still allowing operators to persist or replace probe
assets in the data volume.

## Environment variables

| Var | Purpose | Default |
|---|---|---|
| `XVN_AUTOMIGRATE` | If `1`, run `xvn init --xvn-home $XVN_HOME` before exec | `0` |
| `XVN_DATA_DIR` | Override the data dir | `/data` |
| `XVN_HOME` | Runtime home for dashboard DB, strategies, config, and secrets | `$XVN_DATA_DIR` |
| `XVN_CONFIG_DIR` | Override the config dir | `/config` |
| `XVN_PROBES_DIR` | Override the parquet probe lookup dir | `$XVN_DATA_DIR/probes` |
| `XVN_SEED_PROBES_DIR` | Override the packaged probe seed dir | `/opt/xvision/data/probes` |
| `XVN_SEED_STRATEGIES_DIR` | Override the packaged strategy seed dir | `/strategies` |
| `ANTHROPIC_API_KEY` | Required for `dashboard serve` (wizard + chat-rail SSE) | none |
| `CREDENTIAL_SECRET` | 32-byte hex; encrypts persisted broker keys | none |
| `APCA_API_KEY_ID` / `APCA_API_SECRET_KEY` / `APCA_API_BASE_URL` | Alpaca paper creds | base URL defaults to paper |
| `ORDERLY_KEY` / `ORDERLY_SECRET` / `ORDERLY_ACCOUNT_ID` / `ORDERLY_BASE_URL` | Orderly testnet creds | none |
| `BYREAL_PRIVATE_KEY` / `BYREAL_NETWORK` / `BYREAL_ACCOUNT` | Byreal perps creds (executes on Hyperliquid via `npx @byreal-io/byreal-perps-cli`); used by `xvn fire-trade --venue byreal` etc. | `NETWORK` defaults to mainnet; `ACCOUNT` optional |
| `MANTLE_RPC_URL` / `MANTLE_DEPLOYER_KEY` | Mantle access (identity image only) | none |
| `RUST_LOG` | Tracing filter | unset |
| `XVISION_OBSERVABILITY_RETENTION` | Agent-run retention mode (`full_debug`, `redacted`, `hash_only`) | `full_debug` |
| `XVN_MEMORY_DB` | Cortex memory SQLite path. The entrypoint defaults it onto the writable data volume so memory survives container recreation | `$XVN_HOME/memory.db` |
| `XVN_MEMORY_EMBEDDER` | Set to `local` to use the offline, deterministic `LocalEmbedder` (no network; **degraded recall quality** — dev/offline only). Any other value is ignored | unset |
| `XVN_MEMORY_EMBEDDER_PROVIDER` | Name of a configured provider to use as the embeddings backend (OpenAI-compatible `/embeddings`). Explicit opt-in — wins even when the provider is not api.openai.com | unset |
| `XVN_MEMORY_EMBEDDER_MODEL` | Override the embedding model id | `text-embedding-3-small` |
| `XVN_MEMORY_EMBEDDER_BASE_URL` | Force a no-auth custom OpenAI-compatible `/v1` endpoint (e.g. `http://localhost:11434/v1`). Wins over the dashboard Memory card's custom source; honors `OPENAI_API_KEY` if set | unset |
| `OPENAI_API_KEY` / `OPENAI_BASE_URL` | Direct OpenAI embedder credentials. Used when no `XVN_MEMORY_EMBEDDER`/`..._PROVIDER` override is set | base URL defaults to `https://api.openai.com/v1` |

### Cortex memory embedder

The memory layer ("Cortex") provisions an embedder WITHOUT a hard OpenAI
dependency. The source is resolved in this order:

1. `XVN_MEMORY_EMBEDDER=local` → the offline deterministic `LocalEmbedder`
   (logs a degraded-quality warning; for dev / CI / air-gapped use only).
   `XVN_MEMORY_EMBEDDER_BASE_URL=<url>` → a forced no-auth custom endpoint.
2. `XVN_MEMORY_EMBEDDER_PROVIDER=<name>` → that configured provider's
   `base_url` + key (OpenAI-compatible `/embeddings`), even when it is not
   api.openai.com.
3. `OPENAI_API_KEY` set → the OpenAI env path (`OPENAI_BASE_URL` overrides
   the host).
4. The dashboard Memory card config (`$XVN_HOME/config/memory.toml`):
   `off` → no embedder; `local` → `LocalEmbedder`; `custom` + a base URL →
   a no-auth OpenAI-compatible endpoint at that URL; `<provider>` → that
   provider.
5. Auto-detect: a configured, keyed provider whose `base_url` points at the
   real api.openai.com. Conservative — non-OpenAI providers are never
   auto-picked (they may lack an `/embeddings` endpoint); use the explicit
   opt-in in step 2 for those.
6. Otherwise the offline `Local` embedder (memory works out of the box;
   semantic quality is degraded vs. a real provider). Only an explicit `off`
   (env `XVN_MEMORY_EMBEDDER=off` or the dashboard Memory card) yields no
   embedder, with recall/record degrading to a no-op (never crashes).

**One-step local embeddings (Custom endpoint, no provider).** The fastest
path: `ollama pull nomic-embed-text` (or `qwen3-embedding`), then in
**Settings → General → Memory** set **Embedder source** = **Custom endpoint
(OpenAI-compatible)**, **Custom endpoint base URL** = `http://localhost:11434/v1`
(include the trailing **`/v1`**), and **Embedding model** = `nomic-embed-text`.
No provider registration, no API key — the custom path is **no-auth only**
(the base URL persists in `memory.toml`, which is not a secrets file). For an
**authenticated** endpoint, register a provider instead (next paragraph). You
can also force this from the environment with `XVN_MEMORY_EMBEDDER_BASE_URL`.

**Local embeddings via a registered Ollama provider (no API key).**
`ollama pull nomic-embed-text` (or `qwen3-embedding`, `mxbai-embed-large`,
`bge-m3`, …), then add an **Ollama** provider in Settings → Providers with
base_url `http://localhost:11434/v1` (the `/v1` is **required** — the embedder
POSTs `{base_url}/embeddings`). Ollama is a no-auth kind, so it resolves with
an empty key. Pick it as the **Embedder source** and set the **Embedding
model** in the Memory card. Use this path (over the custom endpoint) when the
endpoint needs a key or you want it reusable across surfaces.

**Embedding-model precedence:** `XVN_MEMORY_EMBEDDER_MODEL` (env) →
`embedder_model` in `$XVN_HOME/config/memory.toml` (the dashboard Memory
card) → `text-embedding-3-small` (default). The dimension differs per model;
the store records each observation's real vector length, so this is handled
automatically. The embedder id is model-aware (`openaicompat:<model>`), so
embeddings from different models stay in separate vector spaces — don't
switch embedders mid-corpus (or `xvn memory forget` and re-embed).

The memory store lives at `XVN_MEMORY_DB` (defaulted onto the data volume by
the entrypoint). Inspect health with `xvn memory status` (or the `memory`
block of `xvn doctor --json`): it reports the store path + writability, the
resolved embedder id/source, the forget grace window, and per-namespace
live-observation counts. Memory is **default-off on every surface** — these
vars provision capability only; nothing starts writing memory automatically.

The retention default ships as `full_debug` so operators can read prompts
and responses in the trace dock from the first run. For shared / client
deployments where raw payloads should not land on disk, set
`XVISION_OBSERVABILITY_RETENTION=hash_only` (or the matching block in
`$XVN_HOME/config/observability.toml`). Explicit `full_debug` (via env,
TOML, or `xvn obs retention set`) still emits the loud startup WARN; the
implicit default does not.

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

The deploy image is the same shape GHCR publishes as `:latest` from
`gh workflow run docker.yml`. Use the script when Actions credits are scarce
or when you want to skip the registry round-trip. `--push` streams the image
over SSH with `docker save | gzip | ssh ... docker load`, then tags it on the
server as `xvision:deploy-latest`.

After that, redeploy every Docker Compose or Coolify service that consumes
`xvision:deploy-latest`; loading the image alone does not restart running
containers.

For a server started from this direct path, point Compose at the loaded image:

```yaml
services:
  xvn:
    image: xvision:deploy-latest
    ports:
      - "8788:8788"
    environment:
      XVN_AUTOMIGRATE: "1"
    volumes:
      - xvision-data:/data
```

Then restart on the server:

```bash
docker compose up -d
docker compose logs -f xvn
```

See `docs/dev/local-image-deploy.md` for the operator checklist.

## Troubleshooting

- **`error while loading shared libraries: libsqlite3.so.0`** — runtime base is
  missing libsqlite3. Make sure the runtime stage `apt-get install`s
  `libsqlite3-0` (the Dockerfile in this repo does).
- **TLS errors hitting Alpaca/Orderly** — confirm `ca-certificates` is
  installed in the runtime stage.
- **Image is huge (>500 MB)** — the most common cause is a missing
  `.dockerignore` letting `target/` or `models/` into the build context.
