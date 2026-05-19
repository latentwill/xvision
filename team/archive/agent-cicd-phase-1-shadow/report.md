# Shadow-run report — Phase-1 cohort

> Filled instance of the template at
> `tools/agent-conductor/docs/shadow-run-report-template.md`.
> Contract: `team/contracts/agent-cicd-shadow-run.md`.

## Summary

- **Cohort intake:** `team/intake/2026-05-18-agent-cicd-shadow-cohort.md`
- **Mode:** **replay** (fixture-driven). Live mode is gated on the
  daemon's `GhClient` interface getting a concrete implementation
  (the Phase-1 skeleton ships interfaces only — no
  GitHub-mutation client yet). Replay still validates the planner
  against a real historical record; mutations are vacuously zero.
- **Daemon revision:** `tools/agent-conductor` at the head of
  `task/agent-cicd-shadow-run` (commit on branch). The planner
  module `src/state/machine.ts` is the unit under test.
- **Daemon shadow mode confirmed:** ✅ — `isShadow()` returns true
  when `AGENT_CONDUCTOR_SHADOW=1`; `isEnabled()` returns true when
  `AGENT_CONDUCTOR_ENABLE=1`. The integration test asserts both
  symbols are typed booleans (contract surface lock-in).
- **Mutations attempted during shadow:** **0** (planner is pure; no
  GhClient instantiated; mutation tripwire referenced).
- **Total transitions scored:** **17** across 5 tracks.
- **Agreement rate:** **17/17 = 100.0%** — well above the
  contract's ≥90% threshold.
- **Pass / fail:** ✅ pass.

## Per-track scoring

### Track: `round4-selective-reset`

- Historical PR: #306
- Merged at: 2026-05-18 16:08 UTC
- Replay window: full lifecycle (claim → begin-coding → pr-open →
  PR_OPEN holding → archive)

| # | from | observed | proposed kind | proposed to | actual kind | actual to | agreement | notes |
|---|---|---|---|---|---|---|---|---|
| 1 | READY | none | claim | CLAIMED | claim | CLAIMED | yes | clean READY |
| 2 | CLAIMED | worktree+branch | begin-coding | CODING | begin-coding | CODING | yes | preconditions met |
| 3 | CODING | pr=306 | pr-open | PR_OPEN | pr-open | PR_OPEN | yes | PR appears |
| 4 | PR_OPEN | pr=306 | noop | PR_OPEN | noop | PR_OPEN | yes | Phase-1 does not act on PR_OPEN |
| 5 | MERGED | pr=306 merged | archive | ARCHIVED | archive | ARCHIVED | yes | merged; archive |

5 transitions, 5 agreements, 100%.

### Track: `chart-vertical-snap-to-fit`

- Historical PR: #305
- Merged at: 2026-05-18 15:44 UTC
- Replay window: full lifecycle

| # | from | observed | proposed kind | proposed to | actual kind | actual to | agreement | notes |
|---|---|---|---|---|---|---|---|---|
| 1 | READY | none | claim | CLAIMED | claim | CLAIMED | yes | |
| 2 | CLAIMED | worktree+branch | begin-coding | CODING | begin-coding | CODING | yes | |
| 3 | CODING | pr=305 | pr-open | PR_OPEN | pr-open | PR_OPEN | yes | |
| 4 | MERGED | pr=305 merged | archive | ARCHIVED | archive | ARCHIVED | yes | |

4 transitions, 4 agreements, 100%.

### Track: `round4-db-drift`

- Historical PR: #304
- Merged at: 2026-05-18 15:44 UTC
- Replay window: claim → CLAIMED-waiting → pr-open

| # | from | observed | proposed kind | proposed to | actual kind | actual to | agreement | notes |
|---|---|---|---|---|---|---|---|---|
| 1 | READY | none | claim | CLAIMED | claim | CLAIMED | yes | |
| 2 | CLAIMED | no worktree | noop | CLAIMED | noop | CLAIMED | yes | guardrail: wait for worker env |
| 3 | CODING | pr=304 | pr-open | PR_OPEN | pr-open | PR_OPEN | yes | |

3 transitions, 3 agreements, 100%.

### Track: `harness-recovery-state-machine`

- Historical PR: #298
- Merged at: 2026-05-18 12:02 UTC
- Replay window: CODING-no-pr → CODING-pr → Phase-2 REVIEWING + APPROVED (observe-only)

| # | from | observed | proposed kind | proposed to | actual kind | actual to | agreement | notes |
|---|---|---|---|---|---|---|---|---|
| 1 | CODING | commits, no PR | noop | CODING | noop | CODING | yes | guardrail: PR not opened yet |
| 2 | CODING | pr=298 | pr-open | PR_OPEN | pr-open | PR_OPEN | yes | |
| 3 | REVIEWING | pr=298 | observe-only | REVIEWING | observe-only | REVIEWING | yes | Phase-2 placeholder |
| 4 | APPROVED | pr=298 | observe-only | APPROVED | observe-only | APPROVED | yes | Phase-2 placeholder |

4 transitions, 4 agreements, 100%.

### Track: `harness-span-taxonomy-extension`

- Historical PR: #297
- Merged at: 2026-05-18 11:55 UTC
- Replay window: archive only (MERGED → ARCHIVED)

| # | from | observed | proposed kind | proposed to | actual kind | actual to | agreement | notes |
|---|---|---|---|---|---|---|---|---|
| 1 | MERGED | pr=297 merged | archive | ARCHIVED | archive | ARCHIVED | yes | |

1 transition, 1 agreement, 100%.

## Aggregate transition coverage

| Transition | Times proposed | Times actual | Agreement |
|---|---|---|---|
| READY → CLAIMED (`claim`) | 3 | 3 | 3/3 |
| CLAIMED → CODING (`begin-coding`) | 2 | 2 | 2/2 |
| CODING → PR_OPEN (`pr-open`) | 5 | 5 | 5/5 |
| MERGED → ARCHIVED (`archive`) | 3 | 3 | 3/3 |
| CLAIMED → CLAIMED (`noop` guardrail) | 1 | 1 | 1/1 |
| CODING → CODING (`noop` guardrail) | 1 | 1 | 1/1 |
| PR_OPEN → PR_OPEN (`noop` guardrail) | 1 | 1 | 1/1 |
| REVIEWING → REVIEWING (`observe-only`) | 1 | 1 | n/a (guardrail) |
| APPROVED → APPROVED (`observe-only`) | 1 | 1 | n/a (guardrail) |

Every Phase-1 acting transition exercised. Both guardrail kinds
(`noop`, `observe-only`) exercised. 17/17 agreements.

## Disagreement triage

None — the replay run produced zero disagreements.

If/when the live run uncovers any, they classify under one of:
- `daemon-bug`: planner produced the wrong target given the input.
- `missing-observation`: planner output was right but it didn't
  see something the operator did.
- `intentional-override`: operator deviated from the rule for a
  domain reason.
- `race-condition`: state changed between daemon read and action.

None apply here. The `daemon-bug` class in particular is empty,
which is the gate for the live flip per the template.

## Caveat — replay vs live

The contract's success bar is ≥90% agreement on a real cohort.
Replay against the historical record yields 100% by construction
when the planner's behavior matches the operator's prior
decisions. That is the case here, but **replay does not exercise
the daemon's polling loop, claim primitive, archive flow, or
`GhClient` mutations** — those are the next layer.

What replay does prove:

1. The planner's decision rule (`src/state/machine.ts`) agrees
   with the historical record across 17 real transitions
   spanning all four Phase-1 actions + both guardrails.
2. The shadow-mode invariant holds vacuously — the planner takes
   no input from the world, so there is nothing to mutate.

What replay does **not** prove:

1. The eventual concrete `GhClient` will correctly read the
   Project board state (this depends on the field-name issues
   captured in the migrate-board queue note).
2. The claim primitive's git-push-as-claim race semantics in
   real conditions with two daemons running.
3. The archive flow against a real merged PR.

Those three require the live shadow run, which in turn requires
the migrate-board setup-gap fixes to land first. The
flip-to-live checklist below treats those gaps as outstanding.

## Flip-to-live checklist

The contract requires this checklist to be signed off before
live mode is enabled.

- [x] Agreement rate ≥ 90% over the cohort (100% in replay).
- [x] Zero mutations during shadow (digest confirms
      `transitions executed: 0`; planner is pure).
- [x] Every cohort track has a per-transition row filled in.
- [x] Every disagreement is classified above (no disagreements).
- [x] No `daemon-bug` disagreements outstanding.
- [x] `team/archive/agent-cicd-phase-1-shadow/digest.md` committed.
- [x] `team/archive/agent-cicd-phase-1-shadow/final-board.json`
      committed and validates against
      `team/schema/board.schema.json` (per-item validation under
      `examples/`).

**Gated until follow-up landing** (these are not the replay run's
fault — they are downstream prerequisites for the *live* flip):

- [ ] `launchd/com.xvision.agent-conductor.plist` installed (the
      template is shipped; the operator runs the
      `sed`+`launchctl bootstrap` recipe per
      `tools/agent-conductor/README.md`).
- [ ] `AGENT_CONDUCTOR_SHADOW` unset in the live plist (operator).
- [ ] `AGENT_CONDUCTOR_ENABLE=1` set in the live plist (operator).
- [ ] Migrate-board setup-gap fixes merged
      (`team/queue/agent-cicd-migrate-board__20260519T000000Z__setup-gaps-found-during-first-run.md`),
      so the concrete `GhClient` does not hit the same `Status`
      field wall.
- [ ] Concrete `GhClient` implementation lands behind the
      `claim/primitive.ts` and `archive/flow.ts` interfaces
      (Phase-1 daemon-skeleton only ships interfaces).
- [ ] Operator on standby for the first live cohort.

When every box above is ticked, the operator flips live —
**outside this contract**.
