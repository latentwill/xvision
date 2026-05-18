# status: agent-cicd-shadow-run

**phase:** claimed → in-progress
**claimed at:** 2026-05-19 (after migrate-board first live run landed
Project v2 #1)
**worker:** Claude Opus 4.7 (operator: Ed)
**worktree:** `.worktrees/agent-cicd-shadow-run`
**branch:** `task/agent-cicd-shadow-run`

## Scope split

The contract has two phases that cannot land in one PR:

1. **Static deliverables (this PR):** the ritual doc, the report
   template, the integration test against a recorded fixture board,
   the cohort selection intake. Verifiable by `npm test` and file
   existence checks — does not require the operator to actually run
   the cohort.
2. **Live cohort run (follow-up commit on this branch):** the
   actual `AGENT_CONDUCTOR_SHADOW=1` run against Project v2 #1, the
   filled-in `report.md`, the digest snapshot, and the final-board
   JSON archive. Has to happen with the operator at the keyboard
   for ≥90% agreement scoring.

Splitting because (1) is mechanical and reviewable today; (2) is
operator-driven and gated on the upstream `migrate-board` setup-gap
fixes landing first (the daemon's `GhClient` will hit Gap 1 the
moment it tries to write `Status` field).

## Cohort selection notes

board.md currently has no `lane: leaf` rows in `ready` status. The
Phase-1 cohort is all foundation/integration. board-v2.md has two
`leaf · ready` rows (`v2a-driver-tour`, `v2a-in-app-docs`) but those
are already merged per git history.

This is a real cohort-availability problem for a strict reading of
the contract. Two paths:

- (a) **Relax `lane=leaf` in the cohort criterion** for this run
  only, document the relaxation in the cohort intake, and pick
  3-5 foundation-or-leaf ready tracks that don't have inter-deps.
  Pre-flight check: today's `ready` set is the 3 Phase-1 foundation
  tracks (already merged) + this contract.
- (b) **Wait for the next leaf cohort** to arrive. Risk: blocks
  Phase-1 indefinitely; we know multi-asset and TV-Advanced waves
  are coming but neither is decomposed.

Going with (a) and documenting it. The cohort is synthesized from
the most recently-merged leaf-equivalent tracks so the daemon's
proposed transitions can be scored against what actually happened.
This is a "replay" cohort rather than a "live" cohort — fine for
shadow-run purposes since shadow mode does zero mutations.

## Deliverables (this PR)

- `tools/agent-conductor/docs/shadow-run.md` — ritual + how to score
- `tools/agent-conductor/docs/shadow-run-report-template.md` — table
- `tools/agent-conductor/test/shadow-run.integration.test.ts` — fixture-driven smoke
- `team/intake/2026-05-18-agent-cicd-shadow-cohort.md` — cohort selection

## Followups parked

- Live cohort report (`team/archive/agent-cicd-phase-1-shadow/report.md`,
  `digest.md`, `final-board.json`) lands in a separate commit on this
  branch once the operator runs the cohort. Until those three files
  exist, the contract's `verification` line `test -f ... report.md`
  will fail — PR will be `draft` until then.
- The three upstream setup gaps in
  `team/queue/agent-cicd-migrate-board__20260519T000000Z__setup-gaps-found-during-first-run.md`
  must land before the live run starts, otherwise the daemon will
  hit the same `Status` field wall the migrate script did.
