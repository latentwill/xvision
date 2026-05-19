# Handoff — Agent CI/CD Phase 1 (parked 2026-05-19)

Phase-1 of the Agent CI/CD control plane spec
(`docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`) is
parked off the active execution board on 2026-05-19. The five contracts
that decomposed it have been moved out of `team/contracts/` to keep the
board focused on operator-blocking QA work; nothing about Phase-1 has
been cancelled.

## Why parked

- The board's 2026-05-18 sweep-2 left this wave as the only non-QA
  active item. Nothing currently active blocks on it, and the conductor
  wanted operator-visible QA tracks (QA Round 5, harness audit tail) to
  finish before another foundational tools wave starts.
- Phase-1 already cleared its shadow-run gate
  (`team/archive/agent-cicd-phase-1-shadow/report.md`, 17/17 transitions
  agreed = 100%). Reviving it does not need a fresh shadow run; it
  needs an owner who can take the four ready contracts through to merge
  + a live flip.
- Phase-2 work (`agent-cicd-extract-package`) is intentionally deferred
  until Phase-1 is live; no change there.

## What lives where now

- **Spec** — `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md` (unchanged).
- **Shadow-run report** — `team/archive/agent-cicd-phase-1-shadow/report.md` (passed).
- **Shadow-run cohort intake** — `team/intake/2026-05-18-agent-cicd-shadow-cohort.md` (unchanged).
- **Phase-1 contracts** — moved to `team/archive/2026-05-19-sweep/contracts/`:
  - `agent-cicd-board-schema.md` — JSON Schema 2020-12 for the task object + GitHub Project v2 setup doc. Blocks the others.
  - `agent-cicd-migrate-board.md` — one-time idempotent script: parse `team/board.md` + `team/board-v2.md`, enrich from contracts, create Issues + Project items. Depends on board-schema.
  - `agent-cicd-daemon-skeleton.md` — Node/TS daemon at `tools/agent-conductor/` with `start|stop|pause|resume|status|watch|cancel` CLI, three-layer status surface (CLI + state.json + digest), instance identity for multi-repo Hermes, zero-host-repo-references boundary. Phase-1 transitions only. Depends on board-schema.
  - `agent-cicd-shadow-run.md` — run daemon in shadow against a real 3–5 leaf cohort; ≥90% agreement gate; archived report unblocks live flip. Depends on the other three. (Shadow itself is already passed — see archive.)
  - `agent-cicd-extract-package.md` — Phase-2 work: extract `tools/agent-conductor/` to standalone npm package + `npx agent-conductor init` scaffolder. Deferred until Phase-1 is live and Phase-2 review-routing has merged.

`team/OWNERSHIP.md` rows 75–103 still reserve the relevant
`tools/agent-conductor/**`, `team/schema/**`, `.github/projects/`, and
`agent-conductor.config.ts` paths under the `agent-cicd-phase-1` wave.
Leave them in place — they document the wave's intended surface so the
next owner does not have to rederive it. They are dead pointers (no
matching active contract) but `scripts/board-lint.sh` only enforces
ownership against `team/contracts/` and will not flag them.

## How to resume

1. Move the five contract files from `team/archive/2026-05-19-sweep/contracts/`
   back to `team/contracts/`:
   ```bash
   git mv team/archive/2026-05-19-sweep/contracts/agent-cicd-*.md team/contracts/
   ```
2. Re-add an **Agent CI/CD Phase 1** section to `team/board.md` under
   `## Active`. The exact section content as of 2026-05-19 sweep is in
   the next section of this doc — paste it verbatim, or update the
   intro line if Phase-2 routing has since landed.
3. Run `bash scripts/board-lint.sh`. Expected: clean.
4. Claim `agent-cicd-board-schema` first (it blocks the other three);
   from there the wave runs as previously planned.
5. The shadow-run pass at `team/archive/agent-cicd-phase-1-shadow/`
   stands. A re-run is only required if the daemon's planner module
   (`tools/agent-conductor/src/state/machine.ts`) has been modified
   since 2026-05-18.

## Verbatim board section (as of 2026-05-19 sweep)

```
### Agent CI/CD Phase 1 (2026-05-18)

Implements `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`.
Phase-1 closes the worktree + PR-open gap; review routing and deploy are
Phase 2/3 (not contracted yet).

- [agent-cicd-board-schema](contracts/agent-cicd-board-schema.md) - foundation - ready - JSON Schema 2020-12 for the task object + GitHub Project v2 setup doc. Blocks the other three.
- [agent-cicd-migrate-board](contracts/agent-cicd-migrate-board.md) - integration - ready - one-time idempotent script: parse `team/board.md` + `team/board-v2.md`, enrich from contracts, create Issues + Project items. Depends on board-schema.
- [agent-cicd-daemon-skeleton](contracts/agent-cicd-daemon-skeleton.md) - foundation - ready - Node/TS daemon at `tools/agent-conductor/` with `start|stop|pause|resume|status|watch|cancel` CLI, three-layer status surface (CLI + state.json + digest), instance identity for multi-repo Hermes, zero-host-repo-references boundary. Phase-1 transitions only. Depends on board-schema.
- [agent-cicd-shadow-run](contracts/agent-cicd-shadow-run.md) - integration - ready - run daemon in shadow against a real 3-5 leaf cohort; ≥90% agreement gate; archived report unblocks live flip. Depends on the other three.
- [agent-cicd-extract-package](contracts/agent-cicd-extract-package.md) - integration - deferred - Phase-2 work: extract `tools/agent-conductor/` to standalone npm package + `npx agent-conductor init` scaffolder. Deferred until Phase-1 is live and Phase-2 review-routing has merged.
```
