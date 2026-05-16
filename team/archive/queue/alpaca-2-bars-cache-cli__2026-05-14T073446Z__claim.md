# Claim: alpaca-2-bars-cache-cli

Claimed: 2026-05-14T07:34:46Z

Worktree: `.worktrees/alpaca-2-bars-cache-cli`

Branch: `alpaca-2-bars-cache-cli`

Scope:

- Execute `docs/superpowers/plans/2026-05-14-alpaca-2-bars-cache-and-cli.md`.
- Lock the SQLite `bars_cache` schema contract with integration coverage.
- Keep cache-key generation deterministic across engine and CLI entry points.
- Preserve the existing `xvn bars` fetch/list/remove/gc surface.

Verification target:

- `cargo test -p xvision-engine --test bars_cache`
- `cargo test -p xvision-cli bars`

Local note:

- This deploy shell is covered by `CLAUDE.md`, which says not to run Rust
  builds/tests here, and `cargo` is not installed on PATH.
