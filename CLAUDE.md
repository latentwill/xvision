# xvision — project guidance

Project-specific guidance. The workspace-level `/Users/edkennedy/Code/CLAUDE.md`
covers shared conventions across projects; the rules here are xvision-specific
and override anything in conflict with the workspace file.

## Terminology

Naming conventions across the xvision codebase. Locked in 2026-05-10 (terminology
rename Option B — see `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`).
Diverging from these names should require a written rationale.

| Concept | Use this name | Don't use |
|---|---|---|
| Per-decision-cycle id (briefing → decision → outcome) | `cycle_id` | ~~setup_id~~ |
| Pre-mint local id of a marketplace pipeline | `agent_id` (string ULID, becomes the NFT token id post-mint) | ~~strategy_id~~ |
| Immutable pipeline configuration (engine bundle artifact) | `StrategyBundle` | (no rename) |
| Trading-decision producer trait (xvision-eval baselines) | `Algorithm` | ~~Strategy~~ |
| One experimental arm in A/B compare | `arm` / `Box<dyn Algorithm>` | (no change) |
| The trader's call (input to risk) | `TraderDecision` | (no change) |
| The risk gate's verdict (Approved / Modified / Vetoed) | `RiskDecision` | (no change) |
| Wallet plan's per-rule verdict (planned new enum) | `PerStrategyVerdict` | ~~Verdict~~ (collides with RiskDecision) |
| The DB table for cycles (formerly `setups`) | `cycles` | ~~setups~~ |
| Eval-result count of cycles processed | `cycles_evaluated` | ~~setups_evaluated~~ |

**Pipeline-stage names** (intern, trader, risk, executor) are roles in the
processing pipeline and are NOT renamed. The `xvn strategy` CLI verb manages
`StrategyBundle`s and is NOT renamed. The `xvn setup` CLI verb (config init)
is NOT renamed — it remains the verb form.

**Migration notes:**
- DB migration `0002_rename_setup_to_cycle.sql` renamed the `setups` table to
  `cycles` and `setup_id` to `cycle_id` across all six referencing tables
  (briefings, decisions, risk_outcomes, executions, traces).
- The `xvn ab-compare --setups` argument is now `--cycles`. Pre-launch
  breaking change.
- Pre-rename git tag: `pre-rename-baseline`.

## Build & test

```bash
cargo build --workspace
cargo test --workspace
```

The workspace expects `cargo` on PATH from `~/.cargo/bin`.

## Active plans

See `docs/superpowers/plans/` for executable implementation plans:
- `2026-05-10-terminology-rename-option-b.md` (this rename, complete)
- `2026-05-10-blockchain-1-non-custodial-wallets-amendments.md` (wallet plan v1.1)
- `2026-05-10-leverage-items.md` (1-pager, README, MANUAL.md, eod report)
- `2026-05-10-blockchain-1-non-custodial-wallets-plan.md` (original wallet plan)
- AR-1/AR-2/AR-3 (autoresearcher)
- 2c (scheduler), 2d (dashboard), and others
