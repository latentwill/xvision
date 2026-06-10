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
ARG BUILD_PROFILE=release

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
        cargo chef cook --profile "$BUILD_PROFILE" --recipe-path recipe.json --workspace --features xvision-engine/chain-attest ; \
    else \
        cargo chef cook --profile "$BUILD_PROFILE" --recipe-path recipe.json ; \
    fi

COPY . .

# Build the binary. Default build uses default-members (xvn only, no identity).
# Note: do NOT cache-mount /xvision/target here — it would shadow the cooked
# artifacts that cargo-chef just baked into the previous layer, forcing every
# build to recompile the dep graph from scratch.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$WITH_IDENTITY" = "1" ]; then \
        cargo build --profile "$BUILD_PROFILE" --workspace --bin xvn --features xvision-engine/chain-attest ; \
    else \
        cargo build --profile "$BUILD_PROFILE" --bin xvn ; \
    fi && \
    cp "target/$BUILD_PROFILE/xvn" /usr/local/bin/xvn

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
    && mkdir -p /data /config /strategies /opt/xvision/data/probes \
    && chown -R xvision:xvision /data /config /strategies /opt/xvision

COPY --from=builder /usr/local/bin/xvn /usr/local/bin/xvn
COPY --chown=xvision:xvision config/        /config/
COPY --chown=xvision:xvision strategies/    /strategies/
COPY --chown=xvision:xvision data/probes/   /opt/xvision/data/probes/
COPY --chown=xvision:xvision docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

USER xvision
WORKDIR /home/xvision

VOLUME ["/data"]

ENV XVN_DATA_DIR=/data \
    XVN_HOME=/data \
    XVN_CONFIG_DIR=/config \
    APCA_API_BASE_URL=https://paper-api.alpaca.markets

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD xvn --version || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["--help"]
