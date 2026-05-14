# Claim: alpaca-3-scenario-registry-runner

Claimed: 2026-05-14T07:40:44Z

Worktree: `.worktrees/alpaca-3-scenario-registry-runner`

Branch: `alpaca-3-scenario-registry-runner`

Scope:

- Execute `docs/superpowers/plans/2026-05-14-alpaca-3-scenario-registry-and-eval-runner.md`.
- Add a domain-level v1 validation contract for DB-backed scenarios.
- Keep API-created scenarios subject to the same validation before insert.
- Preserve existing scenario registry, scenario store, and eval runner behavior.

Verification target:

- `cargo test -p xvision-engine --test scenario_shape`
- `cargo test -p xvision-engine --test scenario_api`
- `cargo test -p xvision-engine --test eval_run_scenario`

Local note:

- This deploy shell is covered by `CLAUDE.md`, which says not to run Rust
  builds/tests here, and `cargo` is not installed on PATH.
