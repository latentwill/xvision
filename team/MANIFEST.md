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

The table below is the source of truth for what's actually on disk in
`crates/xvision-engine/migrations/`. Historical gaps (006, 008, 009)
were reserved but never landed; do not recycle those numbers — picking
the next sequential keeps the registry monotonic and the apply-order
unambiguous.

| #   | Owner                                | Status        |
|-----|--------------------------------------|---------------|
| 001 | engine-api                           | merged        |
| 002 | eval-engine                          | merged        |
| 003 | chat-rail                            | merged        |
| 004 | command-palette                      | merged        |
| 005 | eval-review-data-model               | merged (#176) |
| 006 | (gap — reserved, never landed)       | unused        |
| 007 | skills                               | merged        |
| 008 | (gap — reserved, never landed)       | unused        |
| 009 | (gap — reserved, never landed)       | unused        |
| 010 | bars-cache                           | merged        |
| 011 | scenarios                            | merged        |
| 012 | runs-scenario-fk                     | merged        |
| 013 | cli-jobs                             | merged        |
| 014 | eval-agent-id                        | merged        |
| 015 | eval-decisions-reasoning             | merged        |
| 016 | eval-reviews                         | merged        |
| 017 | eval-findings-review-columns         | merged        |
| 018 | agent-run-observability              | merged        |
| 019 | agent-slot-prompt-version            | merged        |
| 020 | eval-causal-input-sanitization (F-6) | merged        |
| 021 | eval-batch-persistence               | merged        |
| 022 | eval-bundle-agent-id-map (F-11)      | merged        |
| 023 | hypothesis-and-experiments           | merged        |
| 024 | scenario-regime-labels               | merged        |
| 025 | agent-slot-cache-and-window          | merged        |
| 026 | eval-trace-surface-foundation (V2E)  | merged 2026-05-21 |
| 027 | eval-candle-integrity-and-manifest (V2E) | in flight 2026-05-21 |
| 029 | agent-slot-memory-mode (V2D)         | in flight 2026-05-21 |

The next available number is **030**. The conductor must approve and
reserve in this table before a track touches
`crates/xvision-engine/migrations/`.

Note 2026-05-21: eval-trace-surface-foundation originally reserved 023
but 023–025 were already on disk; it landed as 026.
eval-candle-integrity-and-manifest originally reserved 024 but the
same collision shifted it to 027. The contracts' original reserved
numbers are superseded.

Note 2026-05-19: numbers 006, 008, 009 were never landed (collapsed during
the QA waves); the on-disk sequence skips them. New claims continue from
the highest filed-and-merged value.

## Historical context

Phase A/B and the QA waves Q4/Q8/Q9/Q10 are archived under
`team/archive/2026-05-16-migration/`. For one-time historical lookups, read
those files; do not revive them as live work.

## Stand-down

If the conductor changes, update `team/CONDUCTOR.md` "Current conductor"
line first, then this paragraph: previous conductor `@latentwill` 2026-05-16
→ TBD.
