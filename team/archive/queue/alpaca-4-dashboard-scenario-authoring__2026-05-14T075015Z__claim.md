# Claim: alpaca-4-dashboard-scenario-authoring

Claimed: 2026-05-14T07:50:15Z

Worktree: `.worktrees/alpaca-4-dashboard-scenario-authoring`

Branch: `alpaca-4-dashboard-scenario-authoring`

Scope:

- Execute `docs/superpowers/plans/2026-05-14-alpaca-4-dashboard-scenario-authoring.md`.
- Verify the existing dashboard scenario authoring surface against the plan.
- Fill any gaps in scenario API client, shared form, list/detail routes, and eval run launcher wiring.

Verification target:

- `corepack pnpm --dir frontend/web test -- ScenarioForm scenarios-detail eval-runs`
- `corepack pnpm --dir frontend/web build`

Local note:

- Rust checks are CI/non-deploy only per `CLAUDE.md`.
