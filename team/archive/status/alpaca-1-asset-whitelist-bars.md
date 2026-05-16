---
track: alpaca-1-asset-whitelist-bars
worktree: /root/deploy/xvision/.worktrees/alpaca-1-asset-whitelist-bars
branch: alpaca-1-asset-whitelist-bars
phase: implemented
last_updated: 2026-05-14T07:30:47Z
owner: codex
---

# What changed

- Added the Alpaca crypto whitelist source of truth in `xvision-data`.
- Added paginated Alpaca historical-bars fetching with whitelist and history-floor validation.
- Aligned Alpaca paper/execution and CLI asset parsing with the expanded crypto whitelist.
- Normalized bare, pair, lowercase, trimmed, and concatenated USD forms such as `ETH`, `eth/usd`, ` ETHUSD `.

# Checkpoints

- `b09a1ba` — `feat(data): tighten alpaca crypto whitelist`
- `27a5345` — `fix(cli): align asset parser with alpaca whitelist`
- `9214a24` — `fix(data): accept compact alpaca pair symbols`
- `9214a24` — `fix(data): accept compact alpaca pair symbols`

# Verification

- `git diff --check`

# Blocked on

- Rust tests were not run on this deploy host because `CLAUDE.md` forbids `cargo`, `cargo build`, `cargo check`, and `cargo test`.
- `rustfmt` is not installed on this deploy shell.

# CI/non-deploy verification target

```bash
cargo test -p xvision-data --test whitelist --test alpaca_fetcher
cargo test -p xvision-execution --test broker_surface_alpaca_live alpaca_paper_surface
cargo test -p xvision-cli asset
```
