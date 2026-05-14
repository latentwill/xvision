# GHCR Build Optimization — Design

> **Status:** Draft tradeoff note. Drafted 2026-05-12.
> **Purpose:** Reduce xvision's Docker/GHCR deployment cycle time and failure cost without moving heavy Rust builds back onto the 3.7 GiB `extndly-dev` server.
> **Scope:** `.github/workflows/docker.yml`, `Dockerfile.deploy`, `Dockerfile`, `Cargo.toml` build profiles, and `.dockerignore`.

---

## 1. Context

xvision deploys `ghcr.io/latentwill/xvision:latest` from `Dockerfile.deploy`. The image includes:

- the `xvn` Rust CLI
- the axum dashboard server
- the Vite SPA baked into `xvision-dashboard` via `rust-embed`
- runtime config and strategy templates

The server must remain a runtime-only target. The xvision repo guidance explicitly says not to build heavy Rust locally on `extndly-dev` because the host has about 3.7 GiB of RAM and can OOM.

Recent docker workflow evidence showed:

- failed manual runs on `main` around 17 minutes
- one successful branch run around 23 minutes
- the latest inspected failure occurred in `Build & push (default)`, specifically during Rust compilation of `xvision-dashboard`
- the failure was not GHCR upload or server pull; it was a compile error discovered late in the Docker build

That makes this a deployment-cycle problem with two separate concerns:

1. **Failure latency:** compile mistakes are discovered after an expensive Docker setup/build path.
2. **Successful build latency:** the release image still takes roughly 20-30 minutes when the full Docker build must run.

---

## 2. Goals

- Keep production deploys reproducible and optimized.
- Provide a faster manual test-deploy path for iteration.
- Catch compile errors before the expensive Docker release build.
- Ensure smoke tests validate the image produced by the current workflow run.
- Avoid pushing heavy or unnecessary files into Docker build context.
- Preserve the current GHCR/runtime deployment model.

---

## 3. Non-goals

- Do not compile on `extndly-dev`.
- Do not replace GHCR.
- Do not split the dashboard into a separate runtime service in this slice.
- Do not remove the production `release` build path.
- Do not optimize by disabling meaningful validation entirely.

---

## 4. Current Build Shape

`Dockerfile.deploy` already has the right high-level structure:

- Node stage builds the Vite SPA.
- `cargo-chef` planner/cook stages cache Rust dependencies.
- Rust builder compiles `xvn` with the embedded SPA.
- `debian:bookworm-slim` runtime contains only runtime dependencies and `xvn`.

`.github/workflows/docker.yml` already uses:

- Buildx
- GHCR login
- Docker metadata tags
- `type=gha` cache
- optional identity build
- smoke job after the default image build

The obvious "first pass" optimizations are already present. Remaining gains come from workflow design, profile selection, and failure short-circuiting.

---

## 5. Options and Tradeoffs

### Option A: Keep release-only Docker builds, add preflight checks

Add a fast preflight job before Docker:

- `pnpm install && pnpm build`
- `cargo check -p xvision-dashboard` or `cargo check --workspace --exclude xvision-identity`

**Pros**

- Catches most compile errors before Docker spends 15-20 minutes building.
- Lowest risk to production artifacts.
- Does not change runtime behavior.
- Easy to reason about in CI logs.

**Cons**

- Successful production builds remain slow.
- CI does some duplicate work: preflight checks first, Docker build later.
- Requires Rust/Node setup outside Docker in GitHub Actions.

**Use when:** the primary pain is failed builds wasting time.

### Option B: Add a faster `staging` build profile for manual test deploys

Add a Rust profile such as:

```toml
[profile.staging]
inherits = "release"
opt-level = 1
lto = false
codegen-units = 256
strip = "symbols"
```

Then add a Docker build arg like `BUILD_PROFILE=release|staging` and a workflow input for manual runs.

**Pros**

- Faster manual test deployments.
- Keeps production `release` builds unchanged.
- Makes the tradeoff explicit at build time.
- Useful for dashboard/API validation where peak runtime performance is not the bottleneck.

**Cons**

- Staging images are not performance-equivalent to production.
- Runtime behavior can differ slightly for timing-sensitive or optimization-sensitive bugs.
- Creates another cache axis; cache scope should include the profile.
- Operators must know which tag/profile they deployed.

**Use when:** the primary pain is iteration speed for testing UI/API/server changes.

### Option C: Optimize release builds directly

Change release build settings, linker, or dependencies.

Possible examples:

- use a faster linker such as `mold`
- loosen release profile settings
- reduce heavy dependencies compiled into the dashboard path
- avoid embedding sourcemaps or unnecessary static assets

**Pros**

- Speeds up production builds, not only staging.
- May reduce image build memory and linker pressure.
- Can produce durable long-term gains.

**Cons**

- Higher risk because production artifacts change.
- Needs measurement before/after to avoid cargo-cult tuning.
- Dependency reductions may require architectural refactors.
- Linker changes add system-package complexity to Docker.

**Use when:** preflight and staging are not enough, or release build time remains a blocker.

### Option D: Split frontend assets from the Rust binary

Serve the Vite SPA as static files in the image instead of embedding it into the Rust binary, or deploy the frontend separately.

**Pros**

- Frontend-only changes may avoid recompiling Rust.
- Smaller Rust binary.
- Better separation of frontend and backend build loops.

**Cons**

- Bigger architectural change.
- More runtime file-serving and cache-control concerns.
- More deployment moving parts.
- Current self-contained dashboard image is operationally simple.

**Use when:** frontend iteration dominates deploys and the embedded-SPA model becomes the bottleneck.

---

## 6. Recommended Path

Implement Options A and B first.

They address the current evidence with low risk:

- Preflight catches compile failures early.
- Staging profile gives faster manual deploy-test loops.
- Production release image remains unchanged.

Keep Options C and D as follow-ups only after measuring the new workflow.

---

## 7. Proposed Workflow Changes

### 7.1 Add a preflight job

Add a `preflight` job that runs before `build-default`.

Suggested checks:

- `corepack enable`
- `cd frontend/web && pnpm install --frozen-lockfile && pnpm build`
- Rust setup for `1.95.0`
- `cargo check --workspace --exclude xvision-identity`

The Docker build should depend on this job:

```yaml
build-default:
  needs: preflight
```

This does not make successful deploys faster by itself, but it makes failed deploys fail much earlier and with clearer logs.

### 7.2 Add a manual build profile input

Extend `workflow_dispatch`:

```yaml
build_profile:
  description: "Rust build profile"
  type: choice
  default: release
  options:
    - release
    - staging
```

Pass it into Docker:

```yaml
build-args: |
  WITH_IDENTITY=0
  BUILD_PROFILE=${{ github.event.inputs.build_profile || 'release' }}
```

Scope Docker cache by profile:

```yaml
cache-from: type=gha,scope=xvision-default-${{ github.event.inputs.build_profile || 'release' }}
cache-to: type=gha,scope=xvision-default-${{ github.event.inputs.build_profile || 'release' }},mode=max
```

### 7.3 Build profile support in Docker

Add to `Dockerfile.deploy` and the CLI-only `Dockerfile` so the workflow's `build_profile` input behaves consistently for both Dockerfile choices:

```dockerfile
ARG BUILD_PROFILE=release
```

Use it for both chef cook and final build:

```sh
cargo chef cook --profile "$BUILD_PROFILE" --recipe-path recipe.json
cargo build --profile "$BUILD_PROFILE" --bin xvn
cp "target/$BUILD_PROFILE/xvn" /usr/local/bin/xvn
```

For `release`, Cargo still writes to `target/release`; for custom profiles, Cargo writes to `target/<profile>`.

### 7.4 Smoke the current image, not always `latest`

The smoke job currently pulls and runs `:latest`. That can test an older image when a manual run or branch run produces only SHA/branch tags.

Prefer passing the primary tag from metadata/build output into the smoke job, or define an explicit smoke tag for the run:

```yaml
type=sha,format=short
```

Then smoke the SHA tag produced by the same workflow run.

This improves correctness more than speed.

### 7.5 Tighten `.dockerignore`

Add common local-only paths:

```gitignore
frontend/web/node_modules
frontend/web/dist
frontend/web/.vite
coverage
*.tsbuildinfo
```

The current context already excludes major Rust/runtime data paths. This is hygiene that protects local builds and avoids accidental context bloat.

---

## 8. Operational Tradeoffs

### Staging image risk

A staging image is for functional testing, not performance validation. Anything involving final backtest throughput, latency, or binary-size assumptions should still be verified with `release`.

### Cache fragmentation

Adding `BUILD_PROFILE` means separate cache scopes. That is intentional: sharing release and staging caches can create confusing cache behavior. The tradeoff is more cache storage and less cross-profile reuse.

### Duplicate work in preflight

Preflight repeats some work that Docker later performs. This is acceptable because its purpose is early failure. If workflow time on successful builds becomes the dominant cost, revisit whether preflight should run only for manual/staging builds.

### Release build remains slow

This design improves failed-build latency and test deploy latency. It does not promise a sub-five-minute production release build. Release build optimization should be measured separately after these lower-risk changes land.

---

## 9. Measurement Plan

Record timings for at least three runs after implementation:

- preflight duration
- Docker dependency cook duration
- final Rust build duration
- export/push duration
- smoke duration
- total workflow duration

Compare:

- `release` manual build
- `staging` manual build
- failed compile before preflight vs failed compile after preflight

Success criteria:

- Compile errors fail before Docker build starts.
- Manual staging image completes materially faster than release.
- Production release path still succeeds and deploys the same default behavior.
- Smoke job validates the image from the current run.

---

## 10. Recommendation

Proceed with:

1. preflight job
2. `staging` Rust profile
3. `BUILD_PROFILE` Docker arg
4. profile-aware workflow cache scopes
5. smoke-current-image fix
6. `.dockerignore` hygiene

Defer:

- release linker changes
- deeper dependency trimming
- splitting frontend asset serving from the Rust binary

Those are valid but should be driven by timing data after the safer workflow changes are in place.
