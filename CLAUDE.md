# xvision — project guidance

Project-specific guidance. The workspace-level `/Users/edkennedy/Code/CLAUDE.md`
covers shared conventions across projects; the rules here are xvision-specific
and override anything in conflict with the workspace file.

## Deployment guardrails (hard rules)

These rules are mandatory for agents operating on remote/deploy hosts. They do
not apply to local development workstations.

- On remote/deploy hosts, NEVER run `cargo`, `cargo build`, `cargo check`, or
  `cargo test`.
- On remote/deploy hosts, NEVER build Docker images for production rollout.
- ALWAYS publish runtime images through GitHub Actions workflow `.github/workflows/docker.yml`.
- ALWAYS use `workflow_dispatch` inputs explicitly when triggering GHCR builds:
  - `dockerfile=Dockerfile.deploy`
  - `build_identity=false` (unless identity image is intentionally requested)
  - `build_profile=release` for production; `staging` only for manual test images
- ALWAYS source `.op_env` before using `gh` or `op` so GitHub and 1Password access come from the expected deploy-host environment.
- ALWAYS verify rollout by checking running container image digest matches the GHCR digest you just published.

Preferred command path from repo root:

```bash
scripts/deploy-ghcr.sh
```

## Terminology

Naming conventions across the xvision codebase. Locked in 2026-05-10 (terminology
rename Option B — see `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`).
Diverging from these names should require a written rationale.

| Concept | Use this name | Don't use |
|---|---|---|
| Per-decision-cycle id (briefing → decision → outcome) | `cycle_id` | ~~setup_id~~ |
| Pre-mint local id of a marketplace pipeline | `agent_id` (string ULID, becomes the NFT token id post-mint) | ~~strategy_id~~ |
| Immutable pipeline configuration (the strategy artifact) | `Strategy` (in `crates/xvision-engine/src/strategies/`) | ~~StrategyBundle~~, ~~bundle~~ |
| Reusable agent template (per-prompt+model+skills record) | `Agent` (with `Vec<AgentSlot>`) | ~~agent template~~, ~~saved profile~~ |
| Strategy's reference to a library agent | `AgentRef { agent_id, role }` | (no rename) |
| Trading-decision producer trait (xvision-eval baselines) | `Algorithm` | ~~Strategy~~ |
| One experimental arm in A/B compare | `arm` / `Box<dyn Algorithm>` | (no change) |
| The trader's call (input to risk) | `TraderDecision` | (no change) |
| The risk gate's verdict (Approved / Modified / Vetoed) | `RiskDecision` | (no change) |
| Wallet plan's per-rule verdict (planned new enum) | `PerStrategyVerdict` | ~~Verdict~~ (collides with RiskDecision) |
| The DB table for cycles (formerly `setups`) | `cycles` | ~~setups~~ |
| Eval-result count of cycles processed | `cycles_evaluated` | ~~setups_evaluated~~ |

**Pipeline-stage names** (intern, trader, risk, executor) are **valid
conventions, not enforced.** They live as starter-template labels in
`crates/xvision-engine/src/agents/templates.rs`; the underlying data model
treats slot names as user-defined free text. Users can rename or invent
slot names per strategy. The conventions exist so multi-stage strategies
have a shared vocabulary; nothing in the engine requires them.

Amended 2026-05-12 to reflect the agents page v1 outcome (see
`docs/superpowers/plans/2026-05-11-agents-page-v1.md` and the followup
strategies refactor at `2026-05-12-strategies-refactor-agent-composition.md`).
Before that, slot names were hardcoded in `StrategyBundle` fields
(`intern_slot`, `trader_slot`, `regime_slot`); the strategies refactor
replaces those with `Strategy { agents: Vec<AgentRef> }` where the role
label per AgentRef is free text.

The `xvn strategy` CLI verb manages strategy bundles and is NOT renamed.
The `xvn setup` CLI verb (config init) is NOT renamed — it remains the
verb form.

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

## Docker

Slim runtime image of the `xvn` CLI lives at `ghcr.io/latentwill/xvision`.
Two tags: `:latest` (default-members; no on-chain identity stack) and
`:identity` (workspace build including `xvision-identity`).

Local builds:

```bash
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
docker compose run --rm xvn --version
```

See `docker/README.md` for env vars and mounts.

## Active plans

See `docs/superpowers/plans/` for executable implementation plans:
- `2026-05-10-terminology-rename-option-b.md` (rename, complete)
- `2026-05-10-blockchain-1-non-custodial-wallets-amendments.md` (wallet plan v1.1)
- `2026-05-10-leverage-items.md` (1-pager, README, MANUAL.md, eod report)
- `2026-05-10-blockchain-1-non-custodial-wallets-plan.md` (original wallet plan)
- `2026-05-11-agents-page-v1.md` (agents page v1, complete — merged in PR #83)
- `2026-05-11-perps-eval-simulator.md` (perps backtest sim, follow-up)
- `2026-05-12-strategies-refactor-agent-composition.md` (Strategy → agent composition, big follow-up)
- `2026-05-12-eval-per-agent-metrics.md` (per-agent attribution, depends on strategies refactor)
- AR-1/AR-2/AR-3 (autoresearcher)
- 2c (scheduler), 2d (dashboard), and others
