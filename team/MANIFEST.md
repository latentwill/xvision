# xvision v1 — Team Manifest

> Single source of truth for current phase and per-track ownership. Updated
> whenever a track lands a phase boundary or a new track spawns.
>
> Last updated: 2026-05-10 by `docker-image` (added third parallel track)

## Current phase

**Phase A — Foundation** (running now)

Goal: land the engine API foundation, broker surface trait, and frontend
scaffolding so subsequent tracks have stable surfaces to build on.

| Track | Worktree | Branch | Owner CLI | Plan | Status |
|---|---|---|---|---|---|
| `coordinator` | `xvision/` (main) | `main` | session 1 (this one) | — | active — coordinator + integration |
| `engine-api` | `.worktrees/engine-api` | `feature/engine-api-foundation` | session 1 (this one) | [#3](../docs/superpowers/plans/2026-05-10-engine-api-foundation.md) | **PR #4 open** — awaiting merge |
| `broker-surface` | `.worktrees/broker-surface` | `feature/broker-surface-trait` | session 1 (this one) | [Plan 2c §Task 7](../docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md#task-7-brokersurface-trait--dispatch) (extracted) | **PR #5 open** — awaiting merge |
| `frontend-foundation` | `.worktrees/frontend-foundation` | `feature/frontend-foundation` | session 2 (external CLI) | [Plan 1](../docs/superpowers/plans/2026-05-10-frontend-1-foundation-and-strategies.md) Phases 0–1 (scaffolding only) | active — Tasks 1+2 + Vite/Tailwind/Shell committed; Task 5 (ts-rs) waits on PR #4 |
| `docker-image` | `.worktrees/docker-image` | `feature/docker-image` | session 3 | [Plan #14](../docs/superpowers/plans/2026-05-10-docker-image.md) | claimed — independent of Phase A; v1 QA distribution channel |

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
