# Shadow-run cohort — 2026-05-18

Cohort selection for the Phase-1 shadow run defined by
`team/contracts/agent-cicd-shadow-run.md`. Five tracks; replay
mode against recent merges so the daemon's proposed transitions can
be scored against what actually happened in the real workflow.

## Why replay instead of live

The shadow-run contract calls for "3-5 ready leaf tracks" pulled
from `team/board.md`. Two practical problems with that today:

1. The current `ready` rows on `team/board.md` are the Phase-1
   foundation tracks themselves — already merged but still listed
   as `ready` in the markdown. Drift between markdown state and
   ground truth is exactly the kind of thing the daemon will
   surface; it's also exactly the kind of thing that makes them
   unusable as a forward-looking cohort.
2. None of the rows are `lane: leaf`. The only `leaf · ready`
   rows on `team/board-v2.md` (`v2a-driver-tour`, `v2a-in-app-docs`)
   are merged too, and were silently dropped by the migration
   script anyway (Gap 3 in the migrate-board queue note).

Solution: replay cohort. Pick five recently-merged tracks whose
lifecycle is fully known (claim → coding → PR open → merged →
archived) and run the daemon in shadow mode against a snapshot of
board state from before each transition. Score each proposed
transition against the historical record.

This is exactly what shadow mode is for: zero mutations, just
proposals + reasoning. The replay angle gives us a ground-truth
oracle for scoring without making the operator wait days for a new
leaf cohort to materialize.

## Cohort

Five recently merged tracks, spanning all four Phase-1 transitions
the daemon acts on:

| # | Track | PR | Merged | Replay covers |
|---|---|---|---|---|
| 1 | `round4-selective-reset` | #306 | 2026-05-18 16:08 | full lifecycle (claim → archive) |
| 2 | `chart-vertical-snap-to-fit` | #305 | 2026-05-18 15:44 | full lifecycle |
| 3 | `round4-db-drift` | #304 | 2026-05-18 15:44 | claim → PR open |
| 4 | `harness-recovery-state-machine` | #298 | 2026-05-18 12:02 | CODING → PR_OPEN (started CODING) |
| 5 | `harness-span-taxonomy-extension` | #297 | 2026-05-18 11:55 | MERGED → ARCHIVED only |

Why these five:

- Each has a known PR number and merge timestamp; the historical
  record is unambiguous.
- Together they exercise all four Phase-1 transitions (`claim`,
  `begin-coding`, `pr-open`, `archive`) at least once each, plus
  a handful of `observe-only` transitions through `REVIEWING` /
  `APPROVED` / `MERGE_READY` we want to confirm the daemon does
  NOT act on.
- All five are scoped tightly — small file footprints, single-PR
  landings — which the cohort criterion's "no inter-cohort
  dependencies" requirement maps to. None of these depend on
  another track in the cohort.
- They are not `lane: leaf` exclusively (round4-db-drift is
  integration, harness-recovery-state-machine is integration),
  but the cohort criterion `lane=leaf` is relaxed here per the
  status file rationale. Documented in this intake so the
  relaxation is explicit, not an oversight.

## Scoring methodology

For each track, walk the historical record one transition at a
time. At each step:

1. Construct a `BoardTask` reflecting the state of the track
   immediately before the transition (using `git log`, the PR's
   history, and the queue markers under
   `team/queue/archive/<date>/<track>__*.md`).
2. Construct an `ObservedReality` reflecting what the daemon would
   have seen at that moment (worktree present? branch pushed? PR
   number known?).
3. Run `planTransition(task, observed)` and capture the
   `PlannedTransition`.
4. Compare to the actual transition the operator drove:
   - `agreement: yes` — daemon proposed the same target state
     and the same kind of transition.
   - `agreement: partial` — daemon proposed the right target
     state but a slightly different transition kind, or vice
     versa.
   - `agreement: no` — daemon proposed a different target state
     or proposed a transition where the operator did nothing
     (or vice versa). Disagreements get a one-paragraph root
     cause in the report.

The contract sets the success bar at **≥90% agreement** over the
cohort. With ~6 scored transitions per track × 5 tracks = ~30
transitions, that's at most 3 disagreements before the live flip
is blocked.

## Out of scope for this cohort

- Phase-2 transitions (`CHANGES_REQUESTED`, `FIXING`, `APPROVED`,
  `MERGE_READY`): the daemon must propose `observe-only` for
  every one of these. Counted in the report but not against the
  ≥90% bar — they're guardrail checks, not active behavior.
- Setup-gap fixes from
  `team/queue/agent-cicd-migrate-board__20260519T000000Z__setup-gaps-found-during-first-run.md`.
  Replay shadow runs do not touch the live Project, so the
  default `Status` field name issue does not block this cohort.
  Live shadow runs would; those wait for the gap fixes to land.

## Artifacts

When this cohort completes, the deliverables in the contract are
archived under:

- `team/archive/agent-cicd-phase-1-shadow/report.md` — the filled
  template, one section per track, per-transition agreement.
- `team/archive/agent-cicd-phase-1-shadow/digest.md` — the
  daemon's append-only digest from the shadow run.
- `team/archive/agent-cicd-phase-1-shadow/final-board.json` —
  the Project board JSON snapshot at the end of the cohort,
  validated against `team/schema/board.schema.json`.

Until those three files exist, the contract `verification` check
`test -f team/archive/agent-cicd-phase-1-shadow/report.md` is the
PR-blocking gate.
