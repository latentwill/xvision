---
from: frontend-2-settings
to: all
topic: claim
created_at: 2026-05-10T17:30:00Z
ack_required: false
---

# Claiming `frontend-2-settings` track

Session 2 (this CLI) claims the **read-only Settings sub-slice** of
Frontend Plan 2: Tasks 6, 7, 8 + frontend-side of Task 16. Scope is
intentionally narrow to avoid conflict with concurrent llm-providers work
(Plan #7) which will own the providers/danger sub-pages.

Worktree: `.worktrees/frontend-2-settings`
Branch: `feature/frontend-2-settings`

## Files this track will touch

- NEW: `crates/xvision-engine/src/api/settings/{mod,brokers,daemon,identity}.rs`
- NEW: `crates/xvision-dashboard/src/routes/settings/{mod,brokers,daemon,identity}.rs`
- MODIFY: `crates/xvision-engine/src/api/mod.rs` (add `pub mod settings;`)
- MODIFY: `crates/xvision-dashboard/src/server.rs` (register three new routes)
- MODIFY: `crates/xvision-dashboard/src/routes/mod.rs`
- NEW: `frontend/web/src/api/settings.ts`
- MODIFY: `frontend/web/src/routes/settings/index.tsx` — replace 3 placeholder tabs

## Independence from concurrent tracks

- ❌ Does NOT modify `xvision-core::config` — llm-providers (#14, Phase 1)
  already landed the `[[providers]]` schema there; Phase 2+ will extend it
  but stays in `xvision-core`.
- ❌ Does NOT touch `xvision-engine::eval::*` — eval-3.B/3.C territory.
- ❌ Does NOT add new migrations.
- ❌ Does NOT touch `xvision-cli` (no new CLI verbs).

So this track runs cleanly in parallel with eval-3.C-metrics and
llm-providers-phase-2.

## What this PR does NOT include

- `/api/settings/providers` CRUD (Plan 2 Task 5 + 15) — owned by
  llm-providers track once their Phase 2 ships the persistence layer.
- `/api/settings/danger` POST wipe (Plan 2 Task 9) — touches multiple
  stores; I'd rather wait until eval/strategy stores stabilize.
