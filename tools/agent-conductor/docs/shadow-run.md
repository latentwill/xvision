# Shadow-run ritual

This document is the operator runbook for executing the Phase-1
shadow-run cohort defined in
`team/contracts/agent-cicd-shadow-run.md` and selected in
`team/intake/2026-05-18-agent-cicd-shadow-cohort.md`.

The goal: prove the daemon's proposed transitions match what the
operator would have done, at ≥90% agreement, **without making a
single mutation**. Shadow mode = print, don't push.

## Pre-flight

1. Daemon is built and `npm test` is green:
   ```bash
   cd tools/agent-conductor && npm install && npm run build && npm test
   ```
2. `agent-conductor.config.json` at repo root points at Project v2
   #1 (`agent-cicd`). Verify:
   ```bash
   jq -r '.project.number, .repo.owner, .repo.name' agent-conductor.config.json
   ```
3. `gh auth status` shows the `project` and `read:project` scopes
   on the active token (`gh auth refresh -s project,read:project`
   if missing).
4. **Setup-gap fixes have landed** for the migrate-board script
   (the case-insensitive `Status` field lookup; the auto-label
   creation; the em-dash row parser). Without them the daemon's
   eventual `GhClient` will hit the same wall the migrate script
   did. See
   `team/queue/agent-cicd-migrate-board__20260519T000000Z__setup-gaps-found-during-first-run.md`.
5. Decide replay vs live:
   - **Replay** (default for this cohort): the daemon is fed a
     fixture board JSON snapshotted from before each historical
     transition. No network calls to GitHub. Run via
     `npm test -- shadow-run.integration`.
   - **Live**: the daemon polls Project v2 #1 and proposes
     transitions against current state. Requires the setup-gap
     fixes. Run via `AGENT_CONDUCTOR_SHADOW=1 ./bin/agent-conductor start`.

## Invariants the shadow run must preserve

The contract is explicit: shadow mode does **zero** mutations.

- No `git worktree add` calls.
- No `claude` spawns.
- No GraphQL mutations against the Project.
- No remote refs created.

The daemon enforces this at the module boundary: `AGENT_CONDUCTOR_SHADOW=1`
flips a single check that gates every side-effecting call. The
integration test asserts on this invariant by running with a fake
client whose mutation methods all throw. Any proposal that tries
to mutate fails loudly.

If you see ANY of the following in the digest, abort and file a bug:

- `transitions executed: <nonzero>` (must be `0` in shadow mode)
- `git worktree add` lines
- `claude` PIDs appended to queue markers
- `gh api graphql --hostname … mutation` invocations

## Replay run

The replay run is fixture-driven and fully deterministic. Operator
runs:

```bash
cd tools/agent-conductor
AGENT_CONDUCTOR_SHADOW=1 AGENT_CONDUCTOR_ENABLE=1 \
  npm test -- shadow-run.integration
```

Internally:

1. Loads the cohort from
   `test/fixtures/shadow-run-cohort.json` (one entry per cohort
   track, with the historical `BoardTask` + `ObservedReality`
   snapshots per transition).
2. Calls `planTransition` on each snapshot.
3. Asserts the proposed transition matches the expected one
   (encoded in the fixture from the historical record).
4. Asserts zero mutations were attempted (the fake clients throw).

Test passing = the daemon's Phase-1 planner agrees with the
historical operator decisions for every transition in the cohort.

## Live run (when ready)

Requires the setup-gap fixes plus operator at the keyboard.

```bash
cd /Users/edkennedy/Code/xvision
AGENT_CONDUCTOR_SHADOW=1 AGENT_CONDUCTOR_ENABLE=1 \
  ./tools/agent-conductor/bin/agent-conductor start
```

The daemon prints proposed transitions to the digest as it polls.
For each one:

1. Operator reads the proposed transition.
2. Operator decides whether to take the equivalent action manually
   (or skip and write a one-paragraph reason).
3. Operator records `agreement: yes|no|partial` in the report.

After the cohort completes, operator collects the artifacts:

```bash
# digest
cp ~/.cache/agent-conductor/digest-<today>.md \
   team/archive/agent-cicd-phase-1-shadow/digest.md

# final board JSON
gh api graphql -f query='query { node(id:"<PROJECT_NODE_ID>") { ... on ProjectV2 { items(first:50) { nodes { ... } } } } }' \
  > team/archive/agent-cicd-phase-1-shadow/final-board.json

# schema validation
node tools/agent-conductor/scripts/validate-schema.mjs \
  team/archive/agent-cicd-phase-1-shadow/final-board.json
```

## Scoring

Definition of agreement (per the contract):

> A proposed transition is in agreement if (a) the operator
> agrees the transition should fire, and (b) the proposed target
> state matches what the operator would set.

`partial` is for cases where (a) holds but the proposed kind
differs (e.g., proposed `begin-coding` when the actual transition
was a direct `READY → CODING` skip). Partials count as 0.5 toward
the agreement rate.

## Failure paths

| Failure | Action |
|---|---|
| `<90% agreement` | File a follow-up track `agent-cicd-shadow-run-fixups`; re-cut the cohort with the daemon corrections; do not flip to live. |
| Non-zero mutations during shadow | Abort. File a P1 bug under `team/intake/`. Shadow mode is the load-bearing safety; a leak invalidates the whole approach. |
| Replay fixture stale (board evolved) | Regenerate `test/fixtures/shadow-run-cohort.json` from the latest history; re-run the test. |
| `validate-schema.mjs` fails on `final-board.json` | The Project drifted from the schema — usually new fields added or option enum extended. Update the schema in a separate track; do not patch around it. |

## On success

Operator runs the **flip-to-live** checklist (the report's last
section). When that's signed off:

1. Install launchd plist per `tools/agent-conductor/README.md`.
2. Unset `AGENT_CONDUCTOR_SHADOW`.
3. Keep `AGENT_CONDUCTOR_ENABLE=1`.
4. Stand by for the first live cohort.
