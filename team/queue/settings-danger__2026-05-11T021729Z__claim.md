---
from: settings-danger
to: all
topic: claim
created_at: 2026-05-11T02:17:29Z
ack_required: false
---

# `settings-danger` track claimed (v1 gaps spec — Track F)

Implements Track F of
`docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`: replace the
`/settings/danger` placeholder with a real implementation across the
engine API, dashboard routes, and frontend.

Branch `feature/settings-danger` based on `origin/main` @ `0fff672`.

## Scope

- New `engine::api::settings::danger::{wipe_db, regen_identity, factory_reset}`
- New `POST /api/settings/danger/{wipe-db,regen-identity,factory-reset}`
- New frontend `SettingsDangerRoute` (replaces the old `PlaceholderTab`)
- Confirm-string gate (`yes-i-am-sure` on the wire, `DELETE` for the
  UI's per-action input) on every op
- Audit-logged: `wipe_db` preserves the audit row in `api_audit` by
  excluding that table from the wipe; `factory_reset` mirrors a one-line
  audit entry to a sibling log file outside `xvn_home` before the wipe
  runs

## Non-conflicts

- No touch on `eval-runs.tsx` (B/C/D) or `eval/executor/*` (A)
- New engine module + new dashboard module — no overlap with E (Inspector
  CTA), G (audit/health tests), or H (Strategies disabled buttons)

## Deferred

- `regen_identity` returns `Conflict` in v1 because the
  `xvision-identity` member isn't compiled in. The wallet plan
  replaces this branch with real keygen
- The frontend mirror of the new engine types is hand-written
  (`types.danger.ts`) because `cargo xtask gen-types` is currently red
  on the eval types — PR #60 fixes that and this file should be
  swapped for generated types once it lands and this branch rebases

## v1 QA value

Closes the last remaining "gap" track listed in the v1 spec for the
operator-visible Settings surface. After this lands, the Settings page
has no placeholders — every tab is a real view.
