---
from: settings-danger
to: all
topic: pr-open
created_at: 2026-05-11T02:18:46Z
ack_required: false
---

# `settings-danger` PR open: [#67](https://github.com/latentwill/xvision/pull/67)

Track F of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
shipped end-to-end (engine API + dashboard routes + frontend).

## What changed

- New `engine::api::settings::danger::{wipe_db, regen_identity, factory_reset}`
- New `POST /api/settings/danger/{wipe-db,regen-identity,factory-reset}`
- New `SettingsDangerRoute` replaces the `PlaceholderTab`
- Confirm-string gate (`yes-i-am-sure` on the wire, `DELETE` for the UI)
- `wipe_db` excludes `api_audit` so the trail of the wipe survives
- `factory_reset` mirrors the audit line to a sibling log file outside
  `xvn_home` before the wipe — DB row would otherwise vanish

## Tests

- 6 engine unit tests, 6 dashboard http tests
- `cargo test --workspace` — 603 passed / 0 failed
- Frontend typecheck + build — green

## Zero overlap

- No touch on `eval-runs.tsx` (#63 — B/C/D) or `eval/executor/*` (#62 — A)
- New engine + dashboard modules; no overlap with E (Inspector CTA),
  G (audit/health tests), or H (Strategies disabled buttons)

## Remaining v1 gaps tracks

After this and #62 / #63 land, only **E**, **G**, **H** remain:

| Track | Severity | Est. |
|---|---|---|
| E — Inspector "Run eval" CTA | 🟡 GAP | 0.5 day |
| G — audit::record + health::check tests | 🟡 GAP | 0.5 day |
| H — Strategies disabled-button affordance | 🟢 NIT | 0.25 day |

Picking up **G** next (fastest, pure test additions, zero conflict
risk).
