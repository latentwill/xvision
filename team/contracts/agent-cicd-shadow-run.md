---
track: agent-cicd-shadow-run
lane: integration
wave: agent-cicd-phase-1
worktree: .worktrees/agent-cicd-shadow-run
branch: task/agent-cicd-shadow-run
base: origin/main
status: merged
depends_on:
  - agent-cicd-board-schema     # schema must exist
  - agent-cicd-migrate-board    # Project must be populated from markdown
  - agent-cicd-daemon-skeleton  # daemon must support AGENT_CONDUCTOR_SHADOW=1
blocks: []
stacking: none
allowed_paths:
  - team/intake/2026-05-18-agent-cicd-shadow-cohort.md
  - tools/agent-conductor/docs/shadow-run.md
  - tools/agent-conductor/docs/shadow-run-report-template.md
  - tools/agent-conductor/test/shadow-run.integration.test.ts
  - team/archive/agent-cicd-phase-1-shadow/**  # written only at completion
forbidden_paths:
  - team/board.md
  - team/board-v2.md
  - team/contracts/**
  - team/schema/**
  - tools/agent-conductor/src/**
  - tools/agent-conductor/bin/**
  - tools/agent-conductor/scripts/**
  - crates/**
  - frontend/web/**
  - migrations/**
interfaces_used:
  - agent-conductor daemon (shadow mode via AGENT_CONDUCTOR_SHADOW=1)
  - team/schema/board.schema.json (validation)
  - GitHub Project v2 (read-only during shadow run)
  - team/queue/*.md (read-only — daemon writes; this contract observes)
parallel_safe: false  # locks the daemon and the Project for the cohort duration
parallel_conflicts:
  - agent-cicd-daemon-skeleton  # cannot run daemon changes during shadow
  - agent-cicd-migrate-board    # cannot re-migrate during shadow
verification:
  - (cd tools/agent-conductor && AGENT_CONDUCTOR_SHADOW=1 AGENT_CONDUCTOR_ENABLE=1 npm test -- shadow-run.integration)
  - test -f team/archive/agent-cicd-phase-1-shadow/report.md
  - test -f team/archive/agent-cicd-phase-1-shadow/digest.md
  - "node tools/agent-conductor/scripts/validate-schema.mjs team/archive/agent-cicd-phase-1-shadow/final-board.json"
acceptance:
  - "A cohort of 3-5 real (not synthetic) ready tracks is selected from the current `team/board.md` and recorded in `team/intake/2026-05-18-agent-cicd-shadow-cohort.md`. Selection criteria: tracks already `ready`, lane=leaf, no inter-cohort dependencies, expected to land within a normal session."
  - "`tools/agent-conductor/docs/shadow-run.md` documents the ritual: daemon launched with `AGENT_CONDUCTOR_SHADOW=1 AGENT_CONDUCTOR_ENABLE=1`, operator observes proposed transitions in the digest, manually executes the equivalent action (or skips and notes why), the cohort runs to completion under existing manual workflow."
  - "For each task in the cohort: every transition the daemon proposed in shadow mode is logged with timestamp, the action the operator actually took, and an `agreement: yes|no|partial` field. Captured in `tools/agent-conductor/docs/shadow-run-report-template.md` (template) and the filled-in version archived at `team/archive/agent-cicd-phase-1-shadow/report.md` at the end."
  - "Agreement rate ≥ 90% over the cohort. Definition: a proposed transition is in agreement if (a) the operator agrees the transition should fire, and (b) the proposed target state matches what the operator would set. Below 90% blocks live-mode flip — file a daemon bug under `team/intake/` and re-cut a follow-up contract."
  - "Disagreement triage: each non-agreement transition has a one-paragraph root-cause note in the report (daemon misread state, missing data, race condition, intentional operator override, etc.). Not every disagreement is a daemon bug — but it must be explained."
  - "Final digest snapshot (`~/.cache/agent-conductor/digest-<date>.md`) is committed to `team/archive/agent-cicd-phase-1-shadow/digest.md` as evidence."
  - "Final Project board JSON (`gh api graphql … > final-board.json`) is committed to `team/archive/agent-cicd-phase-1-shadow/final-board.json`. Validates against the schema."
  - "On success (≥90% agreement, all cohort tracks landed): the report includes a `flip-to-live` checklist confirming (a) launchd plist installed, (b) `AGENT_CONDUCTOR_SHADOW` unset, (c) `AGENT_CONDUCTOR_ENABLE=1` set, (d) operator on standby for the first live cohort. Operator runs the flip — outside this contract — once the checklist is signed."
  - "Integration test `shadow-run.integration.test.ts` is a smoke harness that runs the daemon in shadow mode against a recorded fixture board (not live GH), asserts the same transitions are proposed each time, and exits 0. This guards the shadow path itself; it is not a substitute for the real cohort run."
  - "No live mutations during shadow: assertion in the daemon (already in `agent-cicd-daemon-skeleton`) plus an explicit check in this contract's verification that no `git worktree add`, `claude` spawn, or GraphQL mutation occurred during the shadow run. Verified by inspecting the digest's `transitions executed` count being zero and `transitions deferred` being non-zero."
---

# Scope

Validates Phase-1 of the agent-conductor against the real (migrated)
board by running it in shadow mode for one cohort. The daemon proposes
transitions; the operator executes them manually using the existing
workflow. Disagreements are root-caused; the cohort completes under
human control. On success, flip to live mode for Phase-1 production use.

This is the Phase-1 acceptance gate from
`docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`:
*"5 consecutive tasks routed end-to-end (READY → MERGED) with zero
manual worktree commands and zero manual board edits."* — adapted to
shadow form: 5 tasks observed in shadow, all proposed transitions
agreed with operator actions ≥90%, no live mutations.

# Out of scope

- Flipping the daemon to live mode. Operator does that after this
  contract's report is signed off — outside the contract.
- Phase-2 review-routing or Phase-3 deploy work.
- Modifying daemon code. If the shadow run reveals bugs, file a
  follow-up under `team/intake/` and a new contract; do not patch
  here.
- Changing the schema. If schema gaps are discovered, file
  intake → new contract → re-migrate → re-shadow.
- Editing the markdown boards or contracts.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-cicd-shadow-run status
git -C .worktrees/agent-cicd-shadow-run log --oneline -3 origin/main..HEAD
# Confirm all three dependencies are merged
git log origin/main --oneline -- team/schema/board.schema.json | head -3
git log origin/main --oneline -- tools/agent-conductor/scripts/migrate-board.mjs | head -3
git log origin/main --oneline -- tools/agent-conductor/src | head -3
# Confirm the migrated Project board exists and is populated
gh project list --owner @me | grep "agent-cicd"
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-cicd-shadow-run -b task/agent-cicd-shadow-run origin/main
```

# Notes

The cohort selection deliberately favors small, leaf-lane tracks. The
goal is to validate the daemon's read of state and its proposed
transitions, not to stress-test the conductor on a Foundation track.
Foundation-track shadowing comes after the first live cohort proves
out leafs.

If the agreement rate is <90% but the disagreements are all
attributable to a single bug, propose a one-line daemon patch contract
and re-shadow with the same cohort definition. Don't expand the cohort
to inflate the agreement rate.
