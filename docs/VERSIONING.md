# Versioning

xvision versions are valid **MAJOR.MINOR.PATCH** triples because Cargo,
npm, Docker tags, and release tooling all expect that shape. Pre-1.0
software; we are free to ship breaking changes inside a minor.

## The scheme

| Component | Meaning | Bump trigger |
|---|---|---|
| **MINOR / release train** (the `21` in `0.21.0`) | One QA wave / one image deploy while pre-1.0 | Each time a Docker image is built and shipped to a deploy host, the release-train number increases by one. |
| **PATCH** (the trailing `0` in `0.21.0`) | Hotfixes inside a shipped image train | Normally stays `0`; use only when a same-train hotfix needs a distinct version. |
| **MAJOR** (the leading `0`) | Stays at `0` pre-launch | Reserved for the first public-launch cut. Not in scope yet. |

The release-train counter is **monotonic across the product's life**. We
are at `0.21` because twenty-one image-shipping QA waves have landed
since the project began (retroactive baseline — see CHANGELOG).

Named capabilities are marked by allocating the next train number; do not
jump backward for decimal-looking labels. For example, if the next major
capability lands after `0.29.0`, it should ship as `0.30.0`, not
`0.3.0`, because semver compares `0.3.0` as older than `0.29.0`.

```
… → 0.21.0 → 0.22.0 → 0.23.0 → … → 0.29.0 → 0.30.0 (autoresearcher) → 0.31.0 → …
```

## Where the version lives

Single source of truth: **`[workspace.package].version` in
`Cargo.toml`**. Every Rust crate inherits via `version.workspace = true`.

The frontend SPA tracks the same number in
`frontend/web/package.json`. The two MUST match — `scripts/bump-version.sh`
updates both atomically and refuses to run when they drift.

The CLI surfaces the version via `xvn --version` (clap reads
`CARGO_PKG_VERSION`). The dashboard exposes it via `GET /api/version`
(planned alongside `/healthz` in the agent-cicd Phase-3 work).

## Bumping

```bash
scripts/bump-version.sh 0.22.0           # standard bump for the next image
scripts/bump-version.sh 0.30.0           # named capability train (e.g. autoresearcher)
scripts/bump-version.sh --dry-run 0.22.0 # show what would change without writing
```

The script:

1. Validates the new version is a strict `MAJOR.MINOR.PATCH` triple.
2. Refuses to bump backward.
3. Edits `Cargo.toml` and `frontend/web/package.json`.
4. Runs `cargo check --workspace` to refresh `Cargo.lock`; failure aborts
   the bump so stale lockfile edits are not committed.
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

We are pre-1.0 and ship breaking changes inside release trains. The
minor component is more useful as an image-deploy odometer than as a
"backward-compatible feature" signal right now. When we hit `1.0` and
start making API stability promises, the rules can tighten.
