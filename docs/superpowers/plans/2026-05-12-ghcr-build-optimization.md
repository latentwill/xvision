# GHCR Build Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce xvision Docker/GHCR failure latency and provide a faster manual staging image path while preserving production release builds.

**Architecture:** Add an early CI preflight job for frontend and Rust compile checks, then parameterize the deploy Dockerfile with a Rust build profile. Keep `release` as the default production path and add `staging` only for manual test deploys.

**Tech Stack:** GitHub Actions, Docker Buildx, Cargo profiles, cargo-chef, pnpm/Vite.

---

### Task 1: Add the staging profile

**Files:**
- Modify: `Cargo.toml`

- [ ] Add `[profile.staging]` inheriting from release, with lower optimization, no LTO, and high codegen units.
- [ ] Keep `[profile.release]` unchanged for production artifacts.

### Task 2: Parameterize Docker deploy builds

**Files:**
- Modify: `Dockerfile.deploy`
- Modify: `Dockerfile`

- [ ] Add `ARG BUILD_PROFILE=release` in the builder stage.
- [ ] Use `cargo chef cook --profile "$BUILD_PROFILE"` for dependency cooking.
- [ ] Use `cargo build --profile "$BUILD_PROFILE"` for final binary builds.
- [ ] Copy from `target/$BUILD_PROFILE/xvn`.

### Task 3: Add preflight and profile inputs

**Files:**
- Modify: `.github/workflows/docker.yml`

- [ ] Add `build_profile` workflow input with `release` and `staging` options.
- [ ] Add a `preflight` job that runs pnpm build, `cargo check --bin xvn`, and a full workspace check only when the identity image will be built.
- [ ] Make `build-default` depend on `preflight`.
- [ ] Pass `BUILD_PROFILE` to Docker builds and scope cache by profile.

### Task 4: Smoke the produced image tag

**Files:**
- Modify: `.github/workflows/docker.yml`

- [ ] Emit the SHA tag as a job output from `build-default`.
- [ ] Make `smoke` pull and run that SHA tag instead of always using `latest`.

### Task 5: Harden Docker context hygiene

**Files:**
- Modify: `.dockerignore`

- [ ] Exclude frontend dependency/build/cache outputs and coverage artifacts.

### Task 6: Verify

**Commands:**
- `git diff --check`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/docker.yml"); puts "workflow yaml ok"'`
- `docker buildx build -f Dockerfile.deploy --build-arg BUILD_PROFILE=staging --target builder --load .` if local Docker has enough access and network/cache.

Expected:
- No whitespace errors.
- Workflow YAML parses.
- Docker build either succeeds or reports an environment/network limitation that must be verified in GitHub Actions.
