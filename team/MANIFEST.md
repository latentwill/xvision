# xvision v1 — Team Manifest

> Single source of truth for current phase and per-track ownership. Updated
> whenever a track lands a phase boundary or a new track spawns.
>
> Last updated: 2026-05-10 by `coordinator` (Phase B in flight — 3 PRs open)

## Current phase

**Phase A — Foundation** ✅ **complete**

All four foundation tracks merged to `main`:

| Track | PR | Merge commit | Notes |
|---|---|---|---|
| `engine-api` | [#4](https://github.com/latentwill/xvision/pull/4) | `adc8d4a` | typed engine API + audit + migration 001 |
| `broker-surface` | [#5](https://github.com/latentwill/xvision/pull/5) | `9cc93cb` | unified BrokerSurface + AlpacaPaper + MockBrokerSurface |
| `frontend-foundation` | [#7](https://github.com/latentwill/xvision/pull/7) | merged | xvision-dashboard + Vite/Tailwind shell |
| `docker-image` | [#6](https://github.com/latentwill/xvision/pull/6) | `76b24b5` | slim runtime image + GHCR workflow |

**Phase B — Build-out** (running now)

Phase A unblocked all of Phase B. Pick a row from the build order, claim it via
`team/queue/<track>__<utc>__claim.md`, edit the row below, and start.

| Track | Worktree | Branch | Owner CLI | Plan | Status |
|---|---|---|---|---|---|
| `coordinator` | `xvision/` (main) | `main` | session 1 (this one) | — | active — coordinator + integration |
| `eval-engine` | `.worktrees/eval-engine` | `feature/eval-engine-foundation` | session 1 (this one) | [#5 (Eval Engine)](../docs/superpowers/plans/2026-05-08-eval-engine-plan.md) Phase 3.A only (Tasks 1–3) | **PR #10 open** — awaiting merge |
| `frontend-foundation-phase-b` | `.worktrees/frontend-foundation` | `feature/frontend-foundation-phase-b` | session 2 (external CLI) | [Frontend Plan 1](../docs/superpowers/plans/2026-05-10-frontend-1-foundation-and-strategies.md) Phase B (ts-rs codegen + `/strategies` wired) | **PR #9 open** — awaiting merge |
| `leverage-items` | `.worktrees/leverage-items` | `feature/leverage-items` | session 3 (external CLI) | [#13 (Leverage items)](../docs/superpowers/plans/2026-05-10-leverage-items.md) Items A–D (docs) | **PR #8 open** — awaiting merge |

## Build order (post-Phase-A)

Phase A unlocks Phase B. See `v1-shipping-plan.md` for the full sequence.

| # | Phase | Plan | Depends on (Phase A item) |
|---|---|---|---|
| B.1 | Eval Engine | [#5](../docs/superpowers/plans/2026-05-08-eval-engine-plan.md) | engine-api, broker-surface |
| B.2 | Strategy 2a — MCP + tools + templates | [#6](../docs/superpowers/plans/2026-05-08-strategy-engine-2a-mcp-tools-templates.md) | engine-api |
| B.3 | LLM Providers + per-arm models | [#7](../docs/superpowers/plans/2026-05-10-llm-providers-and-per-arm-models-plan.md) | engine-api |
| B.4 | Strategy 2b — Skills | [#8](../docs/superpowers/plans/2026-05-08-strategy-engine-2b-skills.md) | engine-api |
| B.5 | Strategy 2d — Dashboard + Wizard | [#9](../docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md) | engine-api, frontend-foundation |
| B.6 | Settings & Onboarding | [#10](../docs/superpowers/plans/2026-05-10-settings-and-onboarding-plan.md) | engine-api |
| B.7 | Chat Rail Persistence | [#11](../docs/superpowers/plans/2026-05-10-chat-rail-persistence-plan.md) | engine-api |
| B.8 | Command Palette | [#12](../docs/superpowers/plans/2026-05-10-command-palette-plan.md) | engine-api |
| B.9 | Leverage items | [#13](../docs/superpowers/plans/2026-05-10-leverage-items.md) | none (docs+CLI) |
| B.10 | Frontend Plan 2–5 | [front-2](../docs/superpowers/plans/2026-05-10-frontend-2-read-only-screens.md) … [front-5](../docs/superpowers/plans/2026-05-10-frontend-5-findings-compare-polish.md) | per-plan deps in their files |
| B.11 | Docker image | [#14](../docs/superpowers/plans/2026-05-10-docker-image.md) | none (independent — packages whatever is on `main`) |

## How to spawn a CLI on a track

```bash
# from anywhere
cd /Users/edkennedy/Code/xvision/.worktrees/<track>
claude
# inside Claude:
#   1. Read team/MANIFEST.md
#   2. Read team/briefings/<track>.md
#   3. Begin work
```

## Migration reservations

See `v1-shipping-plan.md` §"Migration reservations". Live registry:

| # | Owner | Status |
|---|---|---|
| 001_api_audit.sql | engine-api | claimed by Phase A |
| 002_eval.sql | eval-engine (B.1) | reserved |
| 003_chat_sessions.sql | chat-rail (B.7) | reserved |
| 004_search_index.sql | command-palette (B.8) | reserved |

Never claim a new number without editing this table AND `v1-shipping-plan.md`
in the same commit.
