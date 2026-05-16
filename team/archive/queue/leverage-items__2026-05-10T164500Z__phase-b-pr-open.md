---
from: leverage-items
to: all
topic: phase-b-pr-open
created_at: 2026-05-10T16:45:00Z
ack_required: false
---

# Leverage items A–D — PR #8 open

PR: https://github.com/latentwill/xvision/pull/8
Branch: `feature/leverage-items` (rebased onto current `main` which already
has the eval-engine claim commit `4971434`)
Worktree: `.worktrees/leverage-items`

## What landed

Items A–D of `docs/superpowers/plans/2026-05-10-leverage-items.md`, the
plan's "v1 test cut":

1. `docs/HACKATHON-1-PAGER.md` — narrative pitch (judges / sponsors / first-100)
2. `README.md` — first-user conversion + alpha-warning + Docker quickstart
3. `MANUAL.md` "Scale tiers" — N=1/10/100/1000 breakpoints with migration paths
4. `MANUAL.md` "Incident response" — contain → diagnose → communicate → post-mortem checklist

## Followup available for next session

**Item E.1 — `xvn eod` CLI command** is unclaimed. Plan section provides:

- Failing test scaffold at `crates/xvision-cli/tests/eod_cli.rs`
- Full implementation body at `crates/xvision-cli/src/commands/eod.rs`
- Three small edits to `commands/mod.rs` and `lib.rs` to wire the subcommand

**Why it's NOT in this PR:** the CLI mod.rs / lib.rs edits collide with active
frontend-foundation Phase B work (which is also editing those files for the
`xvn dashboard serve` evolution). Pick this up after frontend Phase B settles —
or coordinate via queue if you want to take it now and merge order is clear.

## No file overlap

- `frontend-foundation` Phase B: `crates/xvision-cli/`, `crates/xvision-dashboard/`, `frontend/web/` — untouched here
- `eval-engine` (newly claimed): `crates/xvision-eval/`, `crates/xvision-engine/migrations/002_eval.sql`, `crates/xvision-engine/src/api/eval.rs` — untouched here
- Migration reservations table — untouched here
