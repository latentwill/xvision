# Historical MANIFEST tables (pre-overhaul snapshot)

Captured from `team/MANIFEST.md` at the 2026-05-16 migration.

## Phase A — Foundation (complete)

| Track | PR | Merge commit | Notes |
|---|---|---|---|
| `engine-api` | [#4](https://github.com/latentwill/xvision/pull/4) | `adc8d4a` | typed engine API + audit + migration 001 |
| `broker-surface` | [#5](https://github.com/latentwill/xvision/pull/5) | `9cc93cb` | unified BrokerSurface + AlpacaPaper + MockBrokerSurface |
| `frontend-foundation` | [#7](https://github.com/latentwill/xvision/pull/7) | merged | xvision-dashboard + Vite/Tailwind shell |
| `docker-image` | [#6](https://github.com/latentwill/xvision/pull/6) | `76b24b5` | slim runtime image + GHCR workflow |

## Phase B — Build-out (historical snapshot)

| Track | Worktree | Branch | Owner CLI | Plan | Status |
|---|---|---|---|---|---|
| `coordinator` | `xvision/` (main) | `main` | session 1 | — | active — coordinator + integration |
| `eval-engine-3b` | `.worktrees/eval-engine-3b` | `feature/eval-engine-3b-executors` | session 1 | Plan #5 Phase 3.B | active — Phase 3.A merged via PR #10 |
| `frontend-2-home-and-health` | `.worktrees/frontend-foundation` | `feature/frontend-2-home-and-health` | session 2 | Frontend Plan 2 | active — Phase B merged via PR #9 |
| `llm-providers` | (merged) | `feature/llm-providers-phase-1` | session 3 | Plan #7 Phase 1 | merged via PR #14 |
| `llm-providers-2` | (merged) | `feature/llm-providers-phase-2` | session 3 | Plan #7 Phase 2 | merged via PR #16 |
| `llm-providers-3` | `.worktrees/llm-providers-3` | `feature/llm-providers-phase-3-registry` | session 3 | Plan #7 Phase 3 Tasks 9–10 | active — purely additive |

## Migration reservations

| # | Owner | Status |
|---|---|---|
| 001_api_audit.sql | engine-api | claimed by Phase A |
| 002_eval.sql | eval-engine (B.1) | reserved |
| 003_chat_sessions.sql | chat-rail (B.7) | reserved |
| 004_search_index.sql | command-palette (B.8) | reserved |

The post-migration `team/MANIFEST.md` keeps the migration registry but drops the historical phase tables.
