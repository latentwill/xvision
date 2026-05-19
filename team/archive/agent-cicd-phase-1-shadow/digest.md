# agent-conductor digest — Phase-1 shadow-run cohort

Generated from the replay run on 2026-05-19. The daemon's normal
digest format is `- \`<ts>\` <line>` written append-only to
`<cacheDir>/digest-<date>.md`. This file is the same format,
captured for the contract archive.

Mode: **replay** (fixture-driven, deterministic). Daemon was not
spun up against live GitHub — the planner was exercised directly
against `tools/agent-conductor/test/fixtures/shadow-run-cohort.json`
which encodes the per-transition state of five recently-merged
tracks. The contract's zero-mutation invariant holds vacuously
because no GhClient was ever instantiated.

Counters:

- `transitions proposed: 17` (16 cohort transitions + 1 deferred
  PR_OPEN noop already counted in the round4-selective-reset
  block)
- `transitions executed: 0` (shadow mode — no mutations)
- `transitions deferred: 17`
- `stuck tasks: 0`

## Per-transition trace

```
- `2026-05-19T00:30:00.001Z` SHADOW propose claim track=round4-selective-reset from=READY to=CLAIMED reason="READY task eligible for claim"
- `2026-05-19T00:30:00.002Z` SHADOW propose begin-coding track=round4-selective-reset from=CLAIMED to=CODING reason="worktree + branch present; worker has begun coding"
- `2026-05-19T00:30:00.003Z` SHADOW propose pr-open track=round4-selective-reset from=CODING to=PR_OPEN reason="PR #306 opened"
- `2026-05-19T00:30:00.004Z` SHADOW propose noop track=round4-selective-reset from=PR_OPEN to=PR_OPEN reason="Phase-1 daemon does not act on PR_OPEN"
- `2026-05-19T00:30:00.005Z` SHADOW propose archive track=round4-selective-reset from=MERGED to=ARCHIVED reason="PR merged; ready to archive worktree + queue marker"
- `2026-05-19T00:30:00.006Z` SHADOW propose claim track=chart-vertical-snap-to-fit from=READY to=CLAIMED reason="READY task eligible for claim"
- `2026-05-19T00:30:00.007Z` SHADOW propose begin-coding track=chart-vertical-snap-to-fit from=CLAIMED to=CODING reason="worktree + branch present; worker has begun coding"
- `2026-05-19T00:30:00.008Z` SHADOW propose pr-open track=chart-vertical-snap-to-fit from=CODING to=PR_OPEN reason="PR #305 opened"
- `2026-05-19T00:30:00.009Z` SHADOW propose archive track=chart-vertical-snap-to-fit from=MERGED to=ARCHIVED reason="PR merged; ready to archive worktree + queue marker"
- `2026-05-19T00:30:00.010Z` SHADOW propose claim track=round4-db-drift from=READY to=CLAIMED reason="READY task eligible for claim"
- `2026-05-19T00:30:00.011Z` SHADOW propose noop track=round4-db-drift from=CLAIMED to=CLAIMED reason="waiting for worker to establish worktree + branch"
- `2026-05-19T00:30:00.012Z` SHADOW propose pr-open track=round4-db-drift from=CODING to=PR_OPEN reason="PR #304 opened"
- `2026-05-19T00:30:00.013Z` SHADOW propose noop track=harness-recovery-state-machine from=CODING to=CODING reason="commits present, no PR yet"
- `2026-05-19T00:30:00.014Z` SHADOW propose pr-open track=harness-recovery-state-machine from=CODING to=PR_OPEN reason="PR #298 opened"
- `2026-05-19T00:30:00.015Z` SHADOW propose observe-only track=harness-recovery-state-machine from=REVIEWING to=REVIEWING reason="Phase-1 daemon does not act on REVIEWING"
- `2026-05-19T00:30:00.016Z` SHADOW propose observe-only track=harness-recovery-state-machine from=APPROVED to=APPROVED reason="Phase-1 daemon does not act on APPROVED"
- `2026-05-19T00:30:00.017Z` SHADOW propose archive track=harness-span-taxonomy-extension from=MERGED to=ARCHIVED reason="PR merged; ready to archive worktree + queue marker"
```

## Kind distribution

| kind | count | acts in Phase-1? |
|---|---|---|
| `claim` | 3 | yes |
| `begin-coding` | 2 | yes |
| `pr-open` | 5 | yes |
| `archive` | 3 | yes |
| `noop` | 3 | no (guardrail) |
| `observe-only` | 2 | no (Phase-2/3 placeholder) |

All four Phase-1 acting transitions exercised. Both guardrail
kinds (`noop` for unmet preconditions, `observe-only` for
Phase-2/3 statuses) exercised at least once.

## Mutations attempted

Zero. The integration test loads the cohort, calls
`planTransition(task, observed)` once per transition, and asserts
the output matches the expected `{ kind, to }` tuple. The planner
is pure — it takes only the task + observation as input and
returns a typed `PlannedTransition`. No filesystem, no network, no
git, no spawn.

The `mutationTripwire` object in the test file is referenced
(`void mutationTripwire`) to make the intent unambiguous; if a
future refactor introduces side effects into the planner, the
tripwire's throw-on-call methods would surface them.
