---
from: inspector-api
to: all
topic: pr-open
created_at: 2026-05-10T14:44:00Z
ack_required: false
---

# `inspector-api` track — PR #47 open

PR: https://github.com/latentwill/xvision/pull/47
Branch: `feature/inspector-api-backbone`
Worktree: `.worktrees/inspector-api`
Base: `origin/main` @ `3bf245c`

## What landed

Plan 2D.C Task 8 backend slice — 4 audit-emitting `api::strategy::*`
wrappers + 4 dashboard PUT/POST routes. Closes the gap between PR #36's
`engine::authoring::*` dispatcher and the Foundation pattern's audit
requirement.

## Files this PR touches

- `crates/xvision-engine/src/api/strategy.rs` (additive — 4 fns + audit
  + error mapping helper)
- `crates/xvision-dashboard/src/routes/strategies.rs` (additive — 4 new
  handlers)
- `crates/xvision-dashboard/src/server.rs` (additive — 4 new routes
  registered + `put` import)
- `crates/xvision-dashboard/tests/inspector_routes.rs` (new — 8
  integration tests)

## Tested

- 9 wrapper unit tests (engine)
- 8 route integration tests via `axum_test::TestServer` (dashboard)
- `cargo test --workspace` — **524 passed, 0 failed**

## Hooks for downstream tracks

After this lands, the React Inspector page can call:

```ts
GET  /api/strategy/:id                   // hydrate the form
PUT  /api/strategy/:id/slot/:role        // save slot edit
PUT  /api/strategy/:id/risk              // save risk preset
POST /api/strategy/:id/validate          // refresh Validation card
```

Natural follow-ups (each its own PR):

- Frontend Inspector page (Plan 2D.C Task 8 — React form with 7
  collapsible layer rows + Validation right rail)
- Task 8a LLM split editor + live preview (Move E) — depends on
  FixtureStore
- `set_mechanical_param` wrapper + route — defer until frontend needs
  per-field mechanical editor

## Zero overlap with active sessions

Board was empty when this track started.

## v1 progress

| §168 criterion | Status |
|---|---|
| Backtest end-to-end | ✅ |
| Authoring (`/setup` Wizard → `/strategies` list) | ✅ |
| Inspector authoring (`/authoring/:id` direct edit) | 🟡 backend (this PR); frontend deferred |
| Alpaca paper end-to-end | 🟡 |
| `/eval/compare` UI | 🟡 |
| `xvn eod` | ✅ |
