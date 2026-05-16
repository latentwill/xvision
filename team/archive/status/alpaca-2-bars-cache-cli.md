---
track: alpaca-2-bars-cache-cli
worktree: /root/deploy/xvision/.worktrees/alpaca-2-bars-cache-cli
branch: alpaca-2-bars-cache-cli
base: alpaca-1-asset-whitelist-bars
phase: implemented
last_updated: 2026-05-14T07:34:46Z
owner: codex
---

# What changed

- Added integration coverage that asserts the `bars_cache` table exposes the
  required columns and `bars_cache_by_asset_window` index.
- Added engine-level coverage for deterministic 64-character bars cache keys
  shared by the cache wrapper and `xvn bars` CLI.
- Confirmed the existing `xvn bars` command is wired through the CLI and uses
  the same `compute_cache_key` helper as `eval::bars::load_bars`.

# Checkpoints

- `test(engine): lock bars cache contract`

# Verification

- `git diff --check`

# Blocked on

- Rust tests were not run on this deploy host because `CLAUDE.md` forbids
  `cargo`, `cargo build`, `cargo check`, and `cargo test` here.
- `cargo` is also not installed on PATH in this shell.

# CI/non-deploy verification target

```bash
cargo test -p xvision-engine --test bars_cache
cargo test -p xvision-cli bars
```
