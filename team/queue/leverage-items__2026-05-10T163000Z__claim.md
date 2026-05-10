---
from: leverage-items
to: all
topic: claim
created_at: 2026-05-10T16:30:00Z
ack_required: false
---

# `leverage-items` track claimed (Phase B)

A Claude CLI session is taking the `leverage-items` track. Worktree
`.worktrees/leverage-items`, branch `feature/leverage-items`, branched off
`origin/main` (which now has all four Phase A PRs #4/#5/#6/#7 merged). Plan:
[`docs/superpowers/plans/2026-05-10-leverage-items.md`](../../docs/superpowers/plans/2026-05-10-leverage-items.md).

## Scope (v1 test cut: A → B → C → D only)

Per the plan's "v1 test cut" table, items A–D ship for v1; items E.1, E.2,
F (folded into B), and G are deferred:

- **Item A** — `docs/HACKATHON-1-PAGER.md` (NEW): narrative pitch for judges + sponsors + first-100 users
- **Item B** — `README.md` (CREATE; doesn't currently exist on `main`): first-user conversion + alpha-warning
- **Item C** — `MANUAL.md` (APPEND): "Scale tiers" section (N=1/10/100/1000 breakpoints)
- **Item D** — `MANUAL.md` (APPEND): "Incident response" checklist

**Skipping in this PR:**
- **Item E.1** — `xvn eod` CLI command. Adds `crates/xvision-cli/src/commands/eod.rs` + edits `mod.rs` + `lib.rs`. **Skipped** to avoid CLI-file conflicts with frontend-foundation Phase B (still active, also touches `commands/`). E.1 is a clean follow-up PR once frontend-foundation Phase B lands or pauses.
- **Item E.2** — scheduler registration; depends on Plan 2c which is out of v1 test scope.
- **Item G** — runtime agent rename; depends on wallet plan Group B.3 which is out of v1 test scope.

## Files this track touches (no overlap with active tracks)

- `docs/HACKATHON-1-PAGER.md` (new)
- `README.md` (new — does not currently exist on `main`)
- `MANUAL.md` (append only; existing `# Manual operator tasks` content preserved)
- `team/MANIFEST.md` (mark Phase A complete; add B.9 leverage-items as in-flight)
- `team/status/leverage-items.md` (new)

Zero source-code changes. Zero conflict with frontend-foundation Phase B
(currently active in `crates/xvision-cli/src/commands/dashboard.rs`,
`crates/xvision-dashboard/`, `frontend/web/`).

## Note for whoever spawns next

E.1 (`xvn eod` CLI) is a great pickup once frontend-foundation Phase B lands
or its CLI scope freezes. The plan section is `Item E — `xvn eod` end-of-day
report (1 day)` in the leverage-items plan, with a TDD-ready test scaffold
already written.
