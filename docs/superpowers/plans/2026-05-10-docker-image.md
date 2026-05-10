# xvision Docker Image Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a reproducible, slim Docker image of the `xvn` CLI (default-members of the xvision workspace) suitable for forward-paper trading runs and CI smoke jobs, plus an optional `xvn-identity` variant that includes the `xvision-identity` crate.

**Architecture:** Three-stage Dockerfile using cargo-chef for incremental dep caching: (1) `chef` plans the dependency graph, (2) `builder` compiles the release binary against pinned Rust 1.95.0, (3) `runtime` is a `debian:bookworm-slim` image containing only `xvn`, the `config/` and `strategies/` template trees, an entrypoint that bootstraps `/data`, and the minimal runtime libs (`libsqlite3-0`, `ca-certificates`, `tini`). A build arg `WITH_IDENTITY=0|1` gates the alloy-v2 stack so the default image stays small. A docker-compose file and a GHCR publish workflow round out the deliverable.

**Tech Stack:** Docker BuildKit, multi-stage builds, [cargo-chef](https://github.com/LukeMathWalker/cargo-chef) 0.1.x, `debian:bookworm-slim` runtime, `rust:1.95.0-bookworm` build base, GitHub Actions with `docker/build-push-action@v6`, GHCR.

---

## File Structure

| File | Responsibility |
|---|---|
| `Dockerfile` | Multi-stage build: cargo-chef plan → cargo-chef cook → cargo build → runtime image |
| `.dockerignore` | Keep build context tiny — exclude `target/`, `data/`, `models/`, `notebooks/`, `.git/`, etc. |
| `docker/entrypoint.sh` | Ensure `/data` and `/config` exist, run `xvn store migrate` if `XVN_AUTOMIGRATE=1`, then `exec xvn "$@"` |
| `docker/README.md` | Usage: pull, run, env vars, volumes, troubleshooting |
| `docker-compose.yml` | Local convenience: named volume for `/data`, env-file for creds, optional Postgres-style log dump |
| `.github/workflows/docker.yml` | CI build & push to `ghcr.io/latentwill/xvision` (and `:identity` tag), with QEMU+buildx for `linux/amd64` only initially |
| `CLAUDE.md` | Add a `## Docker` section under Build & test |

---

## Task 1: Add `.dockerignore`

**Files:**
- Create: `.dockerignore`

The build context is sent to the daemon on every `docker build`. Without an ignore file, a 10+ GB `target/` directory and the GGUF weights under `models/` would torpedo build performance and leak large artifacts into intermediate layers.

- [ ] **Step 1: Create `.dockerignore` with the following exact contents**

```gitignore
# VCS
.git
.gitignore
.github

# Rust build outputs
target
**/target
Cargo.lock.bak

# Runtime data (never ship as part of the image)
data
*.db
*.db-journal
*.db-wal
*.db-shm

# Local model weights — too large to bake in; mount at runtime
models

# Editor / OS noise
.vscode
.idea
.DS_Store
*.swp

# Notebooks, probes, ad-hoc scratch
notebooks
probes/m0-orderly/target

# Existing docs we don't need inside the image
docs

# Local env files
.env
.env.local
.env.*.local

# Dockerfiles themselves (copied explicitly when needed)
Dockerfile
docker-compose.yml
docker/
.dockerignore
```

- [ ] **Step 2: Verify the ignore patterns prune the context**

Run: `du -sh $(git ls-files | grep -vE '^(target|data|models|notebooks)') 2>/dev/null | tail -1`
Expected: total well under 50 MB (the actual `xvision` source tree).

- [ ] **Step 3: Commit**

```bash
git add .dockerignore
git commit -m "build(docker): add .dockerignore to keep build context small"
```

---

## Task 2: Add the entrypoint script

**Files:**
- Create: `docker/entrypoint.sh`

The entrypoint guarantees that `/data` exists, optionally runs migrations, then `exec`s `xvn` so signals propagate cleanly under tini.

- [ ] **Step 1: Create `docker/entrypoint.sh`**

```bash
#!/usr/bin/env bash
# xvision container entrypoint.
#
# Behavior:
#   - ensures /data exists (it's the canonical mount for store.db, traces, vectors)
#   - if XVN_AUTOMIGRATE=1, runs `xvn store migrate --db /data/store.db` before exec
#   - execs `xvn` with the caller's args; default arg is `--help`
#
# Env vars consumed:
#   XVN_AUTOMIGRATE       if "1", run store migrate before exec (default: 0)
#   XVN_DATA_DIR          override /data (default: /data)
#   XVN_CONFIG_DIR        override /config (default: /config)
#   APCA_API_KEY_ID       Alpaca paper key (passed through to xvn)
#   APCA_API_SECRET_KEY   Alpaca paper secret
#   APCA_API_BASE_URL     defaults to paper-api.alpaca.markets
#   ORDERLY_KEY / ORDERLY_SECRET / ORDERLY_ACCOUNT_ID / ORDERLY_BASE_URL
#   MANTLE_RPC_URL / MANTLE_DEPLOYER_KEY  (only when running the identity image)
set -euo pipefail

DATA_DIR="${XVN_DATA_DIR:-/data}"
CONFIG_DIR="${XVN_CONFIG_DIR:-/config}"

mkdir -p "$DATA_DIR"

if [[ "${XVN_AUTOMIGRATE:-0}" == "1" ]]; then
  echo "[entrypoint] running store migrate against $DATA_DIR/store.db" >&2
  xvn store migrate --db "$DATA_DIR/store.db"
fi

if [[ $# -eq 0 ]]; then
  set -- --help
fi

exec xvn "$@"
```

- [ ] **Step 2: Mark it executable in git**

Run: `chmod +x docker/entrypoint.sh && git update-index --chmod=+x docker/entrypoint.sh 2>/dev/null || true`
Expected: file mode `100755` reported by `git ls-files -s docker/entrypoint.sh`.

- [ ] **Step 3: Lint with shellcheck**

Run: `shellcheck docker/entrypoint.sh`
Expected: no output (clean). If `shellcheck` is not installed locally, skip — CI will run it.

- [ ] **Step 4: Commit**

```bash
git add docker/entrypoint.sh
git commit -m "build(docker): add entrypoint that bootstraps /data and execs xvn"
```

---

## Task 3: Write the Dockerfile (chef + builder + runtime stages)

**Files:**
- Create: `Dockerfile`

Three stages keep dep compilation cached across source-only changes. The `WITH_IDENTITY` build arg toggles whether the alloy-v2 / `xvision-identity` crate is compiled in — the default is off because that crate is excluded from `default-members` in `Cargo.toml`.

- [ ] **Step 1: Create `Dockerfile`**

```dockerfile
# syntax=docker/dockerfile:1.7

# ──────────────────────────────────────────────────────────────────────────
# Stage 1 — chef: prepare a recipe of the dep graph for caching
# ──────────────────────────────────────────────────────────────────────────
FROM rust:1.95.0-bookworm AS chef
WORKDIR /xvision
RUN cargo install cargo-chef --locked --version ^0.1

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ──────────────────────────────────────────────────────────────────────────
# Stage 2 — builder: cook deps from the recipe, then build xvn
# ──────────────────────────────────────────────────────────────────────────
FROM chef AS builder
ARG WITH_IDENTITY=0

# Build-time system deps:
#   - perl + make → required by apca's vendored-openssl
#   - libsqlite3-dev → sqlx links system libsqlite3
#   - pkg-config + cmake → polars/parquet transitive deps
#   - clang + libclang-dev → bindgen for some sys crates
RUN apt-get update && apt-get install -y --no-install-recommends \
        perl \
        make \
        cmake \
        clang \
        libclang-dev \
        pkg-config \
        libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /xvision/recipe.json recipe.json

# Cook deps. If WITH_IDENTITY=1 we cook the whole workspace so the alloy
# stack is part of the warm cache; otherwise we cook only default-members.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$WITH_IDENTITY" = "1" ]; then \
        cargo chef cook --release --recipe-path recipe.json --workspace ; \
    else \
        cargo chef cook --release --recipe-path recipe.json ; \
    fi

COPY . .

# Build the binary. Default build uses default-members (xvn only, no identity).
# Note: do NOT cache-mount /xvision/target here — it would shadow the cooked
# artifacts that cargo-chef just baked into the previous layer, forcing every
# build to recompile the dep graph from scratch.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$WITH_IDENTITY" = "1" ]; then \
        cargo build --release --workspace --bin xvn ; \
    else \
        cargo build --release --bin xvn ; \
    fi && \
    cp target/release/xvn /usr/local/bin/xvn

# ──────────────────────────────────────────────────────────────────────────
# Stage 3 — runtime: minimal slim image, non-root user
# ──────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

ARG WITH_IDENTITY=0

LABEL org.opencontainers.image.title="xvision" \
      org.opencontainers.image.description="xvision trading-pipeline CLI (xvn)" \
      org.opencontainers.image.source="https://github.com/latentwill/xvision" \
      org.opencontainers.image.licenses="Apache-2.0"

# Runtime deps:
#   - ca-certificates → TLS to Alpaca / Orderly / Mantle RPC
#   - libsqlite3-0 → sqlx loads system libsqlite3 at runtime
#   - tini → PID 1, signal forwarding, zombie reaping
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 1000 xvision \
    && useradd  --system --uid 1000 --gid xvision --home /home/xvision --create-home xvision \
    && mkdir -p /data /config /strategies \
    && chown -R xvision:xvision /data /config /strategies

COPY --from=builder /usr/local/bin/xvn /usr/local/bin/xvn
COPY --chown=xvision:xvision config/        /config/
COPY --chown=xvision:xvision strategies/    /strategies/
COPY --chown=xvision:xvision docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

USER xvision
WORKDIR /home/xvision

VOLUME ["/data"]

ENV XVN_DATA_DIR=/data \
    XVN_CONFIG_DIR=/config \
    APCA_API_BASE_URL=https://paper-api.alpaca.markets

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD xvn --version || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["--help"]
```

- [ ] **Step 2: Build the default image and confirm it produces a working binary**

Run:
```bash
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
```
Expected: build succeeds; final image present in `docker images xvision:dev`.

- [ ] **Step 3: Verify `xvn --version` exits 0 inside the container**

Run: `docker run --rm xvision:dev --version`
Expected: prints `xvn 0.1.0` (or the current `workspace.package.version`) and exits 0.

- [ ] **Step 4: Verify image size is under the budget**

Run: `docker image inspect xvision:dev --format '{{.Size}}' | awk '{ printf "%.0f MB\n", $1 / 1024 / 1024 }'`
Expected: under 250 MB. If it's over, the most likely culprit is forgetting to drop `target/` from the runtime stage (it should never appear there).

- [ ] **Step 5: Commit**

```bash
git add Dockerfile
git commit -m "build(docker): multi-stage Dockerfile producing slim xvn runtime image"
```

---

## Task 4: Verify sqlite migration works inside the container

**Files:**
- Test: ad-hoc — no file changes

This is the smoke test that proves the runtime libsqlite3 ABI matches what the binary expects, and that the entrypoint's `XVN_AUTOMIGRATE` path actually runs.

- [ ] **Step 1: Run the container with `XVN_AUTOMIGRATE=1` against a tmp volume**

Run:
```bash
mkdir -p /tmp/xvision-data && rm -f /tmp/xvision-data/store.db
docker run --rm \
  -e XVN_AUTOMIGRATE=1 \
  -v /tmp/xvision-data:/data \
  xvision:dev --help
```
Expected: entrypoint prints `[entrypoint] running store migrate against /data/store.db`, then `xvn --help` output, exit 0.

- [ ] **Step 2: Confirm the database file was created on the host volume**

Run: `ls -lh /tmp/xvision-data/store.db`
Expected: file exists, non-zero size.

- [ ] **Step 3: Confirm `xvn store stats --db /data/store.db` reads it back**

Run:
```bash
docker run --rm \
  -v /tmp/xvision-data:/data \
  xvision:dev store stats --db /data/store.db
```
Expected: output begins with `XVISION store — /data/store.db`.

- [ ] **Step 4: Confirm the container runs as non-root**

Run: `docker run --rm --entrypoint id xvision:dev`
Expected: `uid=1000(xvision) gid=1000(xvision) groups=1000(xvision)`.

- [ ] **Step 5: No commit needed — verification only.** Move to next task.

---

## Task 5: Build the identity-enabled variant

**Files:**
- Test: ad-hoc — no file changes

The default image excludes `xvision-identity`; this task verifies the `WITH_IDENTITY=1` build arg path also works end-to-end.

- [ ] **Step 1: Build the identity variant**

Run:
```bash
DOCKER_BUILDKIT=1 docker build \
  --build-arg WITH_IDENTITY=1 \
  -t xvision:dev-identity .
```
Expected: build succeeds. Note: this stage will be substantially slower the first time because the alloy v2 stack must compile.

- [ ] **Step 2: Confirm the binary still works**

Run: `docker run --rm xvision:dev-identity --version`
Expected: same version output as the default image — the binary itself is identical, but the workspace was compiled with identity included.

- [ ] **Step 3: Note the size difference**

Run:
```bash
docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}' | grep '^xvision:dev'
```
Expected: `xvision:dev-identity` is comparable in size to `xvision:dev` (since only `xvn` is copied to runtime). If it's significantly larger, something went wrong in stage 3 (e.g., copied the whole `target/` by accident).

- [ ] **Step 4: No commit needed — verification only.** Move to next task.

---

## Task 6: Add docker-compose for local convenience

**Files:**
- Create: `docker-compose.yml`

Compose lets a developer run `docker compose run --rm xvn store stats` without remembering the full `-v -e -e -e` invocation.

- [ ] **Step 1: Create `docker-compose.yml`**

```yaml
# Local-dev convenience for the xvision container.
#
# Usage:
#   cp .env.example .env             # fill in Alpaca / Orderly creds
#   docker compose build             # builds xvision:dev
#   docker compose run --rm xvn --help
#   docker compose run --rm xvn store stats --db /data/store.db
#
# The `data` named volume persists store.db across runs.

services:
  xvn:
    build:
      context: .
      args:
        WITH_IDENTITY: "0"
    image: xvision:dev
    env_file:
      - .env
    environment:
      XVN_AUTOMIGRATE: "1"
    volumes:
      - data:/data
      - ./config:/config:ro
      - ./strategies:/strategies:ro
    # Default to --help so `docker compose run --rm xvn` is harmless.
    command: ["--help"]

  xvn-identity:
    profiles: ["identity"]
    build:
      context: .
      args:
        WITH_IDENTITY: "1"
    image: xvision:dev-identity
    env_file:
      - .env
    environment:
      XVN_AUTOMIGRATE: "1"
    volumes:
      - data:/data
      - ./config:/config:ro
      - ./strategies:/strategies:ro
    command: ["--help"]

volumes:
  data:
```

- [ ] **Step 2: Create a `.env.example` documenting the runtime env vars**

```bash
# Copy this file to `.env` and fill in real values.
# .env is gitignored.

# --- Alpaca paper trading (M5 in MANUAL.md) ---
APCA_API_KEY_ID=
APCA_API_SECRET_KEY=
APCA_API_BASE_URL=https://paper-api.alpaca.markets

# --- Orderly testnet (M6 in MANUAL.md) ---
ORDERLY_KEY=
ORDERLY_SECRET=
ORDERLY_ACCOUNT_ID=
ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org

# --- Mantle (only needed for the identity image) ---
MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz
MANTLE_DEPLOYER_KEY=

# --- Optional ---
HF_TOKEN=
RUST_LOG=info
```

- [ ] **Step 3: Ensure `.env` is gitignored**

Run: `grep -qxF '.env' .gitignore || printf '\n# Local env (docker compose)\n.env\n' >> .gitignore`
Expected: `.env` appears in `.gitignore`.

- [ ] **Step 4: Smoke-test compose**

Run: `docker compose run --rm xvn --version`
Expected: prints `xvn 0.1.0`, exit 0.

- [ ] **Step 5: Commit**

```bash
git add docker-compose.yml .env.example .gitignore
git commit -m "build(docker): add compose file and .env.example for local runs"
```

---

## Task 7: Add the docker/README.md operator guide

**Files:**
- Create: `docker/README.md`

A single-source operator doc so users don't have to read the Dockerfile to figure out what mounts and env vars are expected.

- [ ] **Step 1: Create `docker/README.md`**

````markdown
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
````

- [ ] **Step 2: Commit**

```bash
git add docker/README.md
git commit -m "docs(docker): add operator guide for the xvision image"
```

---

## Task 8: Add the GitHub Actions workflow to build & push to GHCR

**Files:**
- Create: `.github/workflows/docker.yml`

CI builds on every push to `main` and on tags. PRs build but don't push.

- [ ] **Step 1: Create `.github/workflows/docker.yml`**

```yaml
name: docker

on:
  push:
    branches: [main]
    tags: ["v*.*.*"]
  pull_request:
    paths:
      - "Dockerfile"
      - ".dockerignore"
      - "docker/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "rust-toolchain.toml"
      - "crates/**"
      - ".github/workflows/docker.yml"

permissions:
  contents: read
  packages: write

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  build-default:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: docker/setup-buildx-action@v3

      - name: Log in to GHCR
        if: github.event_name != 'pull_request'
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=ref,event=branch
            type=ref,event=pr
            type=semver,pattern={{version}}
            type=sha,format=short
            type=raw,value=latest,enable={{is_default_branch}}

      - name: Build & push (default)
        uses: docker/build-push-action@v6
        with:
          context: .
          push: ${{ github.event_name != 'pull_request' }}
          build-args: |
            WITH_IDENTITY=0
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha,scope=xvision-default
          cache-to: type=gha,scope=xvision-default,mode=max

  build-identity:
    runs-on: ubuntu-latest
    needs: build-default
    if: github.event_name != 'pull_request'
    steps:
      - uses: actions/checkout@v4

      - uses: docker/setup-buildx-action@v3

      - uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          flavor: |
            suffix=-identity,onlatest=true
          tags: |
            type=ref,event=branch
            type=semver,pattern={{version}}
            type=sha,format=short
            type=raw,value=identity,enable={{is_default_branch}}

      - name: Build & push (identity)
        uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          build-args: |
            WITH_IDENTITY=1
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha,scope=xvision-identity
          cache-to: type=gha,scope=xvision-identity,mode=max

  smoke:
    runs-on: ubuntu-latest
    needs: build-default
    if: github.event_name != 'pull_request'
    steps:
      - uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Pull & smoke test latest
        run: |
          docker pull ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:latest
          docker run --rm ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:latest --version
          docker run --rm \
            -e XVN_AUTOMIGRATE=1 \
            -v $PWD/data:/data \
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:latest \
            store stats --db /data/store.db
```

- [ ] **Step 2: Validate the workflow YAML locally**

Run: `python -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/docker.yml"))'`
Expected: no output (YAML parses).

- [ ] **Step 3: Commit and push to a feature branch to trigger the PR build**

```bash
git checkout -b feature/docker-image
git add .github/workflows/docker.yml
git commit -m "ci(docker): add GHCR build & push workflow with default + identity variants"
git push -u origin feature/docker-image
```

- [ ] **Step 4: Open a PR and confirm the `build-default` job succeeds (no push)**

Use: `gh pr create --fill`
Expected: PR opens; `docker / build-default` check turns green; no images appear in GHCR (PR builds skip push).

---

## Task 9: Document the Docker workflow in CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

Future agents (and humans) need a one-liner discovery path so they don't reinvent the build.

- [ ] **Step 1: Append a `## Docker` section after the existing `## Build & test` section**

Open `CLAUDE.md` and add the following block immediately after the `cargo test --workspace` fenced block:

```markdown
## Docker

Slim runtime image of the `xvn` CLI lives at `ghcr.io/latentwill/xvision`.
Two tags: `:latest` (default-members; no on-chain identity stack) and
`:identity` (workspace build including `xvision-identity`).

Local builds:

```bash
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
docker compose run --rm xvn --version
```

See `docker/README.md` for env vars and mounts.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: point CLAUDE.md at the Docker image for runtime deploys"
```

---

## Task 10: Final end-to-end verification

**Files:**
- Test: ad-hoc — no file changes

Confirms the whole pipeline before merging.

- [ ] **Step 1: Clean local state and rebuild from scratch**

Run:
```bash
docker rmi xvision:dev xvision:dev-identity 2>/dev/null || true
docker builder prune -af
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
```
Expected: build completes with no cache, succeeds.

- [ ] **Step 2: Re-run smoke checks from Task 4**

Run:
```bash
rm -rf /tmp/xvision-data && mkdir -p /tmp/xvision-data
docker run --rm -e XVN_AUTOMIGRATE=1 -v /tmp/xvision-data:/data xvision:dev --help
docker run --rm -v /tmp/xvision-data:/data xvision:dev store stats --db /data/store.db
docker run --rm --entrypoint id xvision:dev
```
Expected: all three commands exit 0; the third reports uid=1000(xvision).

- [ ] **Step 3: Confirm CI is green on the PR**

Use: `gh pr checks`
Expected: `docker / build-default` is the only docker job (since this is still a PR); status is `pass`.

- [ ] **Step 4: Merge**

Use: `gh pr merge --squash --delete-branch`
Expected: PR merges; `main` branch CI now runs `build-default`, `build-identity`, and `smoke`. Confirm all three go green and that `ghcr.io/latentwill/xvision:latest` and `:identity` are pullable.

---

## Out of scope (explicitly)

These items are intentionally not part of this plan. Open separate plans if you want them:

- **GPU / CUDA image** for local Trader inference. The `scripts/setup_runpod.sh` flow still owns that path; baking CUDA into the standard image would balloon it from ~250 MB to multiple GB. A future `xvision:cuda` tag could wrap `nvidia/cuda:12.4.0-runtime-ubuntu22.04` and copy the same `xvn` binary.
- **Multi-arch (`linux/arm64`) builds.** The first cut targets `linux/amd64` only. Adding arm64 doubles CI time and the alloy v2 stack has historically been finicky on arm.
- **Distroless runtime base.** `gcr.io/distroless/cc-debian12` would shave another ~30 MB but breaks the shell-based entrypoint. Worth revisiting once the entrypoint stabilizes.
- **Helm chart / k8s manifests.** Out of scope until there's a concrete deploy target.
- **MCP server image.** `xvision-mcp` is in the workspace but doesn't yet have a stable bin entry point; ship that as a follow-up image once the binary lands.
