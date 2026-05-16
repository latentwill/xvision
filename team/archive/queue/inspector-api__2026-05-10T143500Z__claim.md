---
from: inspector-api
to: all
topic: claim
created_at: 2026-05-10T14:35:00Z
ack_required: false
---

# `inspector-api` track claimed (Plan 2D.C Task 8 — backend slice)

The Wizard PRs (#36, #39, #41) shipped the chat-driven authoring path on
`/setup`. The Inspector (`/authoring/<draft_id>`) is the direct-edit
surface; this PR lands the audit-emitting `api::strategy::*` wrappers +
the dashboard PUT routes the React Inspector page will call.

The frontend Inspector page (form + slot editors) is a separate follow-up
that depends on this PR's API surface.

Worktree `.worktrees/inspector-api`, branch
`feature/inspector-api-backbone`, based on `origin/main` @ `d23feea`.

Briefing: `team/briefings/inspector-api.md`.

## Scope

1. `crates/xvision-engine/src/api/strategy.rs` — add 4 audit-emitting
   wrappers: `create_strategy`, `update_slot`, `set_risk_config`,
   `validate_draft`. Each wraps the existing `authoring::*` dispatcher
   (PR #36) with audit + ApiError mapping.
2. `crates/xvision-dashboard/src/routes/strategies.rs` — add 4 routes:
   `GET /api/strategy/:id`, `PUT /api/strategy/:id/slot/:role`,
   `PUT /api/strategy/:id/risk`, `POST /api/strategy/:id/validate`.
3. Router wiring + tests.

## Deferred to follow-up PRs

- **Frontend Inspector page** (Plan 2D.C Task 8 templates + JS) — React
  form with the 7 collapsible layer rows + Validation right rail.
- **Task 8a — LLM split editor + live preview** (Move E) — depends on
  FixtureStore from a separate Plan 2D.C subtask.
- `set_mechanical_param` wrapper + route — the dispatcher exists; defer
  until the frontend needs the per-field mechanical editor.

## Zero overlap with active sessions

Board is empty as of `d23feea`. No PRs touching `api/strategy.rs`,
`routes/strategies.rs`, or `authoring.rs` are open.

## v1 progress

After this PR + the frontend Inspector follow-up, the §168
success-criterion 2 (operator can author end-to-end) closes — the
Inspector is the missing piece between the Wizard creating drafts
and operators tuning them directly.
