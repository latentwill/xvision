# Versioning

xvision uses **MAJOR.MINOR.PATCH** numbers but maps them to product
cadence rather than strict semver. Pre-1.0 software; we are free to
ship breaking changes inside a minor.

## The scheme

| Component | Meaning | Bump trigger |
|---|---|---|
| **PATCH** (the `21` in `0.21.0`) | One QA wave / one image deploy | Each time a Docker image is built and shipped to a deploy host, the patch number increases by one. |
| **MINOR** (the `0` in `0.21.0`) | A major product capability | Bumped when a named feature lands as a unit. Autoresearcher = `0.3`. Wallet plan + on-chain identity = a future minor. The next minor resets the patch counter to `0`. |
| **MAJOR** (the leading `0`) | Stays at `0` pre-launch | Reserved for the first public-launch cut. Not in scope yet. |

The patch counter is **monotonic across the product's life**, not per
minor. We are at `0.21` because twenty-one image-shipping QA waves have
landed since the project began (retroactive baseline — see CHANGELOG).

When a minor bump occurs, patch resets:

```
… → 0.21.0 → 0.22.0 → 0.23.0 → … → 0.29.0 → 0.3.0 (autoresearcher) → 0.3.1 → 0.3.2 → …
```

The minor-bump version itself ships as an image, so the first patch
under the new minor is `.0`, not `.1`.

## Where the version lives

Single source of truth: **`[workspace.package].version` in
`Cargo.toml`**. Every Rust crate inherits via `version.workspace = true`.

The frontend SPA tracks the same number in
`frontend/web/package.json`. The two MUST match — `scripts/bump-version.sh`
updates both atomically and `scripts/verify-version.sh` (or a CI check)
fails if they drift.

The CLI surfaces the version via `xvn --version` (clap reads
`CARGO_PKG_VERSION`). The dashboard exposes it via `GET /api/version`
(planned alongside `/healthz` in the agent-cicd Phase-3 work).

## Bumping

```bash
scripts/bump-version.sh 0.22.0           # standard patch bump for the next image
scripts/bump-version.sh 0.3.0            # minor bump (e.g. autoresearcher landing)
scripts/bump-version.sh --dry-run 0.22.0 # show what would change without writing
```

The script:

1. Validates the new version is a strict `MAJOR.MINOR.PATCH` triple.
2. Refuses to bump backward.
3. Edits `Cargo.toml` and `frontend/web/package.json`.
4. Runs `cargo check --workspace` to refresh `Cargo.lock`.
5. Prepends an empty `[<new>] - <UTC date>` section to `CHANGELOG.md`
   for the operator to fill in before commit.
6. Prints the commands to commit + tag (does not commit on your behalf
   so you can edit the changelog first).

## CHANGELOG

`CHANGELOG.md` follows the [Keep a Changelog](https://keepachangelog.com)
shape: one section per released version, items grouped under `Added`,
`Changed`, `Fixed`, `Removed`. Each version maps 1:1 to one shipped
image (or one minor feature landing).

The changelog is the operator-facing release note. The deploy script
should reference the matching `[<version>]` heading when announcing a
deploy.

## Image tagging

The Docker image built by `scripts/deploy-image.sh` is tagged
`xvision:deploy-<sha>` today. The bump script's follow-up command
suggests also tagging `xvision:<version>` so the deploy host can pull
by version name. The two tags point to the same digest.

The version that a running container reports MUST match the version in
the image tag. Drift = the container is running an image other than
the one named.

## Why not strict semver

We are pre-1.0 and ship breaking changes inside minors. The patch
counter is more useful as an image-deploy odometer than as a
"backward-compatible bugfix" signal. When we hit `1.0` and start
making API stability promises, the rules can tighten.
