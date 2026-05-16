# Claim: alpaca-1-asset-whitelist-bars

Claimed: 2026-05-14T07:26:02Z

Worktree: `.worktrees/alpaca-1-asset-whitelist-bars`

Branch: `alpaca-1-asset-whitelist-bars`

Scope:

- Execute `docs/superpowers/plans/2026-05-14-alpaca-1-asset-whitelist-and-bars-fetcher.md`.
- Tighten the Alpaca crypto whitelist contract in `xvision-data`.
- Validate Alpaca historical fetches against the whitelist and history floor before HTTP.
- Keep Alpaca paper/execution symbol parsing aligned with the same whitelist.

Verification target:

- `cargo test -p xvision-data --test whitelist --test alpaca_fetcher`
- `cargo test -p xvision-execution --test broker_surface_alpaca_live alpaca_paper_surface`

Local note:

- This deploy shell currently has no `cargo` or `rustfmt` binary, so Rust
  verification must run in CI or a Rust-equipped workspace.
