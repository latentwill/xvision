# xvision v1 — Team Manifest

> Single source of truth for top-level coordination pointers. The conductor
> owns this file (see `team/CONDUCTOR.md`).
>
> Last updated: 2026-05-16.

## Live coordination

| Artifact | Purpose |
|---|---|
| `team/board.md` | Active execution board — current wave (one line per active track) |
| `team/board-v2.md` | V2 roadmap board — V2A active, V2B/V2C/V3/V4 not yet decomposed |
| `team/CONDUCTOR.md` | Conductor role + daily checklist |
| `team/OWNERSHIP.md` | File-glob → owning track map |
| `team/CONFLICT_ZONES.md` | Single-writer file registry |
| `team/contracts/<track>.md` | Per-track contract (one file per active track) |
| `team/contracts/_template.md` | Contract template |
| `team/status/<track>.md` | Per-track current status (worker-owned) |
| `team/queue/<from>__<utc>__<topic>.md` | Append-only inter-track messages |
| `team/briefings/_template.md` | Sync-before-work briefing template |
| `team/intake/<date>-<wave>.md` | Raw wave intake before decomposition |
| `team/archive/<date>-<wave>/` | Frozen state of closed-out waves |
| `scripts/board-lint.sh` | CI/local consistency check |

Spec that defined this layout:
`docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`.

## Worker onboarding (cold start)

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
cat team/board.md                 # current wave
cat team/board-v2.md              # V2 roadmap + V2A active
cat team/contracts/<track>.md     # read the contract
cat team/briefings/_template.md   # do the sync ritual
```

Then write `team/status/<track>.md` and begin.

## Migration registry

Reserved DB migration numbers. Never claim a new number without editing this
table AND `v1-shipping-plan.md` in the same commit.

| # | Owner | Status |
|---|---|---|
| 001 | engine-api | merged |
| 002 | eval-engine | merged |
| 003 | chat-rail | merged |
| 004 | command-palette | merged |
| 005 | eval-review-data-model | merged (#176) |

The next available number is 006. The conductor must approve and reserve in
this table before a track touches `crates/xvision-engine/migrations/`.

## Historical context

Phase A/B and the QA waves Q4/Q8/Q9/Q10 are archived under
`team/archive/2026-05-16-migration/`. For one-time historical lookups, read
those files; do not revive them as live work.

## Stand-down

If the conductor changes, update `team/CONDUCTOR.md` "Current conductor"
line first, then this paragraph: previous conductor `@latentwill` 2026-05-16
→ TBD.
