# Fix: Worktree disk bloat and Rust fmt churn

**Plan type:** refactor  
**Target repo:** `latentwill/xvision`  
**Status:** active  
**Date:** 2026-06-18

---

## Problem Frame

Two related tooling problems in the xvision Rust workspace:

1. **Worktree disk bloat** — agents spinning up git worktrees each compile the full 16-crate workspace independently. The existing `CARGO_TARGET_DIR` advice in `CLAUDE.md` is unenforced, and there's no shared compile cache, so N worktrees produce N× the dep compilation artifacts. Disk is currently at 91% (38G total).

2. **Rust fmt churn on main** — the working tree is perpetually dirty with formatting changes that stem from inconsistent rustfmt versions or rustfmt config across agent sessions. The `.pre-commit-config.yaml` checks `cargo fmt` but doesn't fix it, and there's no CI guard, so agents can merge (or just leave behind) format drift.

Both problems share a root cause: agents working in the repo lack a standardized, automated setup for Rust build tooling.

---

## Requirements

- R1. A shared artifact cache so N worktrees produce one set of dependency compilations
- R2. Per-worktree target directories with automated cleanup when the worktree is removed
- R3. Disk guardrails: warn at 85%, refuse new builds at 95%
- R4. A single-command worktree setup script that agents can run after `git worktree add`
- R5. CI enforcement that `cargo fmt --check` passes before merge
- R6. The active main checkout must not accumulate fmt-only dirty files
- R7. Backward compatible — existing workflows (vanilla `cargo build`, existing CI workflows) continue to work unchanged
- R8. Block direct commits and pushes to main from the main checkout, redirecting to worktrees
- R9. Documentation in the repo so agents and humans know the expected setup

---

## Key Technical Decisions

### KTD-1: sccache for shared compilation

sccache (`cargo install sccache`) provides a bounded, shared object cache. Independent of target directory location — even if worktrees have separate `target/` dirs, compiled `.o`/`.rlib` files for dependencies hit the shared cache. The only per-worktree cost is the final crate linking artifacts (~10–50MB vs ~500MB–1GB).

Configured via `RUSTC_WRAPPER` using Cargo's `if` conditional syntax in `.cargo/config.toml` so it only activates when sccache is on PATH. A bare `RUSTC_WRAPPER = "sccache"` without the conditional causes every `cargo` invocation to fail on machines where sccache isn't installed — including CI and fresh clones.

### KTD-2: worktree setup script enforces shared target dir + persistent config

A `scripts/setup-worktree.sh` script handles `CARGO_TARGET_DIR` assignment, sccache verification, `.envrc` persistence, and hook installation as part of worktree creation. Agents that create worktrees run this. Crucially, `CARGO_TARGET_DIR` must be written to a persistent config file (`.envrc`), not just exported in the shell, so it survives new terminals and agent restarts.

### KTD-3: auto-fix in pre-commit, check in CI

The pre-commit hook auto-fixes formatting (`cargo fmt`), not just checks it (`cargo fmt -- --check`). CI runs the check-only variant. This keeps the local cycle fast while the remote gate stays strict.

---

## Implementation Units

**Sequencing note:** U6 (block direct work on main) should be implemented first. It's a small, low-risk change to `.pre-commit-config.yaml` and `.githooks/pre-push` that immediately enforces the worktree discipline. Without it, the remaining units are all implemented on main — the exact failure mode U6 targets. Implement U6, create a worktree from main, then implement U1–U5 from the worktree.

### U1. Install and configure sccache

**Goal:** sccache installed and auto-detected by every `cargo` invocation in the workspace.

**Dependencies:** none

**Files:**
- `.cargo/config.toml` (modify)
- `scripts/README.md` (optional, note sccache)
- `CLAUDE.md` (update)

**Approach:**
- Add `sccache` to `[env]` in `.cargo/config.toml` using the conditional `if` syntax so it only activates when sccache is on PATH:
  ```toml
  [env]
  RUSTC_WRAPPER = { value = "sccache", if = "sccache: program" }
  ```
  This is read by every `cargo` invocation in the workspace. When sccache isn't installed, cargo silently skips the wrapper.
- Default cache dir is `~/.cache/sccache` — set `SCCACHE_DIR` in both `.cargo/config.toml` and `scripts/setup-worktree.sh` (double-entry) so sccache gets the limit regardless of how it was launched:
  ```toml
  SCCACHE_DIR = "/home/agents/.cache/sccache"
  SCCACHE_CACHE_SIZE = "2G"
  ```
- The workspace's `rust-version = "1.95"` supports the `if` conditional syntax (stabilized in Cargo 1.80).

**Patterns to follow:** The existing `[env]` block in `.cargo/config.toml` sets `XVISION_DISABLE_AGENT_SAVE_GATE` — follow the same convention.

**Test scenarios:**
1. **sccache present + configured** — `sccache --show-stats` returns valid stats; `cargo check -p xvision-core` uses sccache (visible in `sccache --show-stats` as cache hits)
2. **sccache absent** — `cargo check` falls back gracefully (warning but no failure)
3. **cache size enforcement** — `sccache --show-stats` shows `Cache size` ≤ 2G

**Verification:** `sccache --show-stats` shows cache activity after a `cargo check`.

---

### U2. Worktree setup script

**Goal:** A single `scripts/setup-worktree.sh` that agents run after `git worktree add` to establish correct build isolation and disk hygiene.

**Dependencies:** U1 (sccache configured in `.cargo/config.toml`)

**Files:**
- `scripts/setup-worktree.sh` (create)
- `CLAUDE.md` (update — reference the script in the build section)

**Approach:**
The script does:
1. **Verify sccache** is on PATH. If not, suggest `cargo install sccache` and exit with a clear message.
2. **Set `CARGO_TARGET_DIR`** to a path based on the worktree name/branch:
   ```bash
   WORKTREE_NAME=$(basename "$(git rev-parse --git-dir)")
   export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision/$WORKTREE_NAME"
   ```
3. **Persist `CARGO_TARGET_DIR` in `.envrc`** — writes the export to a `.envrc` file in the worktree root so it survives new terminals, agent restarts, and IDE reopens. Supports `direnv` natively and is also source-able manually:
   ```bash
   echo "export CARGO_TARGET_DIR=\"$CARGO_TARGET_DIR\"" > .envrc
   echo "export SCCACHE_CACHE_SIZE=\"2G\"" >> .envrc
   ```
4. **Configure `core.hooksPath`** — points git at a tracked hooks directory so the pre-push hook (U6) is distributed to every checkout:
   ```bash
   git config core.hooksPath .githooks
   ```
5. **Write a `.worktree-meta` file** in the worktree root with `WORKTREE_NAME`, `CARGO_TARGET_DIR`, and creation timestamp — cleanup scripts (U5) use this to find stale artifacts.
6. **Print a summary** showing the worktree name, target dir path, sccache status, and remaining disk space.

**Patterns to follow:** `scripts/deploy-image.sh` for script conventions (bash, error handling, environment variables).

**Test scenarios:**
1. **Fresh worktree setup** — run `git worktree add -b test-fix ../test-fix main` then `./scripts/setup-worktree.sh` from the new worktree → prints target dir path, sccache active, no errors
2. **sccache missing** — script detects absence and exits with install instructions (non-zero exit)
3. **Re-run idempotent** — running setup-worktree.sh again in the same worktree doesn't duplicate or break anything
4. **Target dir env is respected** — `cargo check -p xvision-core` from the worktree writes to the expected `CARGO_TARGET_DIR` path
5. **`.envrc` persistence** — after running setup, opening a new shell and running `cargo check` still uses the correct `CARGO_TARGET_DIR` (because `.envrc` is sourced)
6. **`core.hooksPath` configured** — `git config --get core.hooksPath` returns `.githooks`

**Verification:** `printenv CARGO_TARGET_DIR` from the worktree shows the expected path; `cargo check` writes artifacts there.

---

### U3. CI fmt guard

**Goal:** A GitHub Actions workflow (or step in an existing workflow) that blocks merging when `cargo fmt --check` produces any diff.

**Dependencies:** none

**Files:**
- `.github/workflows/rust-fmt.yml` (create) or add a job to `docker.yml`

**Approach:**
- Add a `fmt-check` job to the existing `docker.yml` workflow (not a separate workflow — a parallel workflow wastes CI runner minutes). The job runs before the build job and short-circuits if formatting fails.
- Runs `cargo fmt --check` on the workspace. Fails on any formatting mismatch.
- Uses `actions-rust-lang/setup-rust-toolchain@v1` with `components: rustfmt` — the standard action does not install rustfmt by default.
- Caches `~/.cargo` to keep it fast (< 10s on cache hit from the first warm run).
- Only triggers on pull requests to `main`.

```yaml
jobs:
  fmt-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          components: rustfmt
      - run: cargo fmt --check
  build:
    needs: fmt-check
    ...

**Patterns to follow:** `docker.yml` for CI structure and cache strategy.

**Test scenarios:**
1. **Clean formatting** — a PR with `cargo fmt`-clean code passes the check
2. **Unformatted code** — a PR with formatting drift fails the check and shows the diff
3. **Non-rust changes only** — a PR touching only `.md`/`.toml`/`.yaml` files still passes (the check produces no diff)

**Verification:** Create a test branch with intentional format drift, push, verify CI fails at the fmt-check step.

---

### U4. Fix pre-commit to auto-fix formatting

**Goal:** The existing `.pre-commit-config.yaml` runs `cargo fmt -- --check` (read-only check) but does not auto-apply fixes. Change it to auto-apply, so the active checkout never accumulates format drift.

**Dependencies:** none

**Files:**
- `.pre-commit-config.yaml` (modify the `cargo-fmt` hook)

**Approach:**
- Change the `cargo-fmt` hook from `cargo fmt --all -- --check` to `cargo fmt --all`. This applies formatting, then the hook exits 0 if it succeeded.
- This means `git commit` auto-fixes formatting before staging. The staged diff is always clean.
- The CI check (`U3`) remains the strict guard for merged code.

**Patterns to follow:** The existing `.pre-commit-config.yaml` structure.

**Fmt version stability note:** If the agent's local `cargo fmt` version differs from CI's, auto-fix and CI check can produce different results. To prevent this, pin the toolchain with a `rust-toolchain.toml` at workspace root (create one if it doesn't exist). This ensures both local and CI run the same rustfmt:
```toml
[toolchain]
channel = "1.85"  # match rust-version in Cargo.toml
components = ["rustfmt", "clippy"]
```

**Test scenarios:**
1. **Unformatted rust file** — modify a `.rs` file to have bad formatting, run `pre-commit run cargo-fmt` → file is auto-fixed, hook exits 0
2. **Clean file** — already-formatted file passes through unchanged
3. **No rust changes** — commit with only non-rust files skips the hook (pre-commit's `types: [rust]` already handles this)

**Verification:** After `pre-commit run cargo-fmt -a`, `git diff --stat` shows no fmt-only changes.

---

### U5. Disk guardrails and cleanup automation

**Goal:** Automatic protection against disk-full scenarios, plus cleanup scripts for stale worktree artifacts.

**Dependencies:** U2 (target dir naming convention)

**Files:**
- `scripts/cleanup-worktrees.sh` (create)
- `scripts/deploy-image.sh` (update — add disk preflight from this script)
- `scripts/setup-worktree.sh` already references cleanup

**Approach:**

**5a. Cleanup script (`scripts/cleanup-worktrees.sh`):**
- Lists all active git worktrees via `git worktree list`
- Runs `git worktree prune` first to clean up metadata for worktrees that were deleted with `rm -rf` instead of `git worktree remove`
- For each worktree name under `$HOME/.cargo-target/xvision/`, checks if the worktree still exists (via `.worktree-meta` files or remaining git worktree entry)
- Removes target dirs for deleted worktrees
- Prunes sccache beyond the 2G limit — either `sccache --clear` or rely on `SCCACHE_CACHE_SIZE` enforcement
- Reports reclaimed space

**Test scenarios:**
1. **Fresh system (nothing to clean)** — cleanup on a clean system prints "Nothing to clean" (exit 0)
2. **Worktree removed with `git worktree remove`** — cleanup deletes the stale target dir
3. **Worktree removed with `rm -rf`** — `git worktree prune` cleans the metadata, then cleanup deletes the stale target dir
4. **Active worktree** — cleanup script skips target dirs for still-active worktrees
5. **Disk near full** — a script consuming this check exits with a message about what to reclaim
**5c. Automation (cron or git hook):**
- Register a `post-worktree-remove` git hook (works at repo level, not per-worktree) that runs the cleanup script

**5b. Disk preflight (reuse from `deploy-image.sh`):**
- The existing `deploy-image.sh` already has disk guardrails (refuse at 95%, warn at 85%)
- Add a `disk_check` function to `scripts/cleanup-worktrees.sh` and document for reuse
- All worktree-creation paths should check disk before allowing new work

**Patterns to follow:** The disk preflight logic in `deploy-image.sh` (check `df -P /`, refuse at ≥95%).

**Verification:** After removing a worktree and running cleanup, `ls $HOME/.cargo-target/xvision/` no longer shows the stale directory.

---

### U6. Block direct work on main

**Goal:** Prevent commits and pushes made directly on the `main` branch checkout, redirecting all work to feature branches or worktrees.

**Dependencies:** none

**Files:**
- `.pre-commit-config.yaml` (modify — add a `no-direct-main` hook; add a `check-worktree-setup` hook)
- `.githooks/pre-push` (create — block direct pushes to main, committed to repo so `core.hooksPath` distributes it)
- `scripts/setup-worktree.sh` (already configures `core.hooksPath` in U2)

**Approach:**

Three layers of increasing severity:

**Layer 1 — Pre-commit hook (cheapest, catches commits on main)**
Add to `.pre-commit-config.yaml`:
```yaml
- id: no-direct-main-commits
  name: No direct commits to main
  entry: bash -c 'case "$(git rev-parse --abbrev-ref HEAD)" in main|master) echo "❌ Direct commits to main are not allowed. Use a worktree or feature branch."; exit 1;; esac'
  language: system
  pass_filenames: false
  always_run: true
```
This fires on every `git commit` while checked out to `main`. Slightly annoying during intentional hotfixes — bypass with `git commit --no-verify`. That friction is intentional.

**Layer 2 — Pre-push hook (blocks accidental push to main from any checkout)**
Create `.githooks/pre-push` (tracked in git, distributed via `core.hooksPath`):
```bash
#!/bin/bash
# Block accidental pushes to main. Hotfix pushes still need to go through a PR.
while read local_ref local_sha remote_ref remote_sha; do
    case "$remote_ref" in
        refs/heads/main|refs/heads/master)
            echo "❌ Direct pushes to main/master are blocked. Open a PR instead."
            exit 1
            ;;
    esac
done
exit 0
```
Make it executable. This blocks any client-side push to `main` regardless of which branch the push originates from — prevents the scenario where an agent accidentally merges a feature branch directly to main via push.

**Layer 3 — GitHub branch protection (failsafe, may already exist)**
Enable "Require pull request before merging" on `main` in GitHub repository settings. Double-check this is active — if it's already on, layers 1 and 2 are hygiene/speed bumps that catch the mistake before it reaches GitHub.

**Design note:** Layer 1 (pre-commit) and Layer 2 (pre-push) are complementary. Pre-commit catches the agent that tries to commit while still on main. Pre-push catches the agent that creates a feature branch but then pushes it directly to main's remote ref. Both are needed — they fire at different lifecycle events.

**Why not solve this in the agent instructions?**
CLAUDE.md already tells agents to use worktrees, and it's still happening. A mechanical guard (hook) is the only reliable fix for this class of problem — instructions are aspirational, hooks are enforced.

**Layer 4 — Setup guard (catches agents that work on main without running setup-worktree.sh)**
In addition to blocking main commits, the pre-commit hook should also verify that the environment is set up correctly when working in a worktree. Add a second pre-commit hook that checks `CARGO_TARGET_DIR` is set when in a worktree:

```yaml
- id: check-worktree-setup
  name: Worktree setup check
  entry: bash -c 'dir=$(git rev-parse --git-dir); case "$dir" in .git) exit 0;; esac; if [ -z "${CARGO_TARGET_DIR-}" ]; then echo "⚠️  CARGO_TARGET_DIR not set in this worktree. Run: scripts/setup-worktree.sh"; exit 1; fi'
  language: system
  pass_filenames: false
  always_run: true
```

This fires on every commit from a worktree (not from the main `.git` checkout) and rejects the commit if the agent skipped setup. Combined with U6's main-block, this closes the loop: agents can't work directly on main AND can't work in a worktree without proper setup.

**Design note:** `.githooks/` (committed to repo) vs `.git/hooks/` (not committed). The pre-push hook must live in `.githooks/pre-push` (tracked by git) and the setup script must run `git config core.hooksPath .githooks` to activate it. This is why U2 configures `core.hooksPath` — it makes the hook available to every checkout automatically.

**Patterns to follow:** The existing `.pre-commit-config.yaml` hooks for structure and conventions.

**Test scenarios:**
1. **Commit on main** — check out `main`, modify a file, `git commit` → hook rejects with clear message, exits non-zero, no commit created
2. **Commit on feature branch** — check out a feature branch, `git commit` → hook passes through (doesn't fire or exits 0)
3. **Pre-push to main** — attempt `git push origin main` → hook rejects with clear message, push aborted
4. **Bypass by intent** — `git commit --no-verify` on main works (allows legitimate hotfixes with deliberate friction)
5. **Non-main branch push** — `git push origin feature-branch` passes the pre-push hook

**Verification:** On main, `git commit --allow-empty -m "test"` produces the rejection message and exits non-zero without creating a commit.

---

## Open Questions

- **What is the current source of fmt churn?** — is it different rustfmt versions between agents, or the fact that the pre-commit hook currently only *checks* rather than *fixes*? U4 (auto-fix) and U3 (CI check) together should stop the drift regardless of cause.
- **sccache availability** — sccache needs to be installed on the build host (`cargo install sccache`). Adding to the repo's documented setup is enough, or a `.cargo/config.toml` fallback to avoid warning when sccache is absent.

---

## Risks and Dependencies

- **Risk: sccache startup latency** — first invocation adds ~100ms startup time. Acceptable trade.
- **Risk: pre-commit hook slows down** — `cargo fmt --all` is fast (~1s on a clean workspace). The existing `cargo test` hook is the slow one (minutes), unchanged by this plan.
- **Risk: CI cache miss** — if the sccache/rust-toolchain cache isn't configured in CI, the fmt check adds ~30s per run. Add cache configuration to keep it under 10s.
- **Dependency: sccache on PATH** — agents must have `sccache` installed. The setup script enforces this; CI doesn't need it since CI envs won't use worktrees.
- **Dependency: rustfmt component** — the CI job needs `rustup component add rustfmt`. Handled by the `actions-rust-lang/setup-rust-toolchain` action.
