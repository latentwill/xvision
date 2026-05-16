# Conductor role

One conductor at a time. Owns process artifacts; does not write feature code.

## Owned artifacts

- `team/board.md` (current-wave board)
- `team/board-v2.md` (V2 roadmap board)
- `team/MANIFEST.md` (top-level pointers)
- `team/OWNERSHIP.md`
- `team/CONFLICT_ZONES.md`
- `team/CONDUCTOR.md` (this file)
- `team/contracts/_template.md`
- The frontmatter (not body) of every `team/contracts/<track>.md`
- `team/intake/<date>-<wave>.md` (raw wave intake docs)
- `team/archive/**` (read-only after creation)

A worker may edit the **body** of their own contract (Notes section,
checkpoints). Frontmatter changes go through a contract-update PR reviewed by
the conductor.

## Out-of-bounds

- Feature code in `crates/**` or `frontend/web/src/**`
- Specs in `docs/superpowers/specs/` or plans in `docs/superpowers/plans/`
- Per-track `team/status/<track>.md` (the worker owns this)

The conductor may write process tooling (`scripts/board-lint.sh` and helpers
under `scripts/board/`).

## Daily checklist (target ≤ 30 minutes)

1. `git fetch --prune origin` and read `team/board.md`.
2. `bash scripts/board-lint.sh` — all green expected.
3. Read each active contract's `team/status/<track>.md`:
   - `claimed` >72h with no `in-progress` update → reassign or escalate.
   - `needs-rebase` → confirm rebase is queued; if blocked, file a queue note.
4. Reconcile `gh pr list --state open` with contract `status:` fields. A PR's
   contract should be `pr-open` while the PR is open; bump to `merged` the
   day it lands.
5. For any contract that became `merged` today:
   - Move its row to `team/archive/<date>/contracts/`.
   - Release any rows it held in `team/CONFLICT_ZONES.md`.
   - Run the branch cleanup step (delete `task/<slug>` on origin).
6. If a new wave's intake is sitting in `team/intake/`, decompose it into
   contracts before opening more leaf tracks.

## Wave lifecycle

A wave moves through four conductor-driven states:

1. **Intake** — raw operator/QA report lands in `team/intake/<date>-<wave>.md`.
2. **Decomposed** — conductor writes one contract per track, registers
   ownership and conflict zones.
3. **In-flight** — contracts are `ready` → `claimed` → `in-progress` →
   `pr-open` → `merged` over days/weeks.
4. **Closed-out** — last Integration track lands. Conductor archives the
   wave: contracts → `team/archive/<date>-<wave>/contracts/`, status files
   → `team/archive/<date>-<wave>/status/`. Wave row leaves `team/board.md`.

## Spinning up a new track

1. Copy `team/contracts/_template.md` to `team/contracts/<slug>.md`.
2. Fill frontmatter. `allowed_paths` and `forbidden_paths` are mandatory.
3. Add an ownership row in `team/OWNERSHIP.md`.
4. If the contract claims a conflict-zone row, update `team/CONFLICT_ZONES.md`.
5. Add the track to `team/board.md` as a single line.
6. Create the worktree:
   ```bash
   git fetch --prune origin
   git worktree add .worktrees/<slug> -b task/<slug> origin/main
   ```
7. The first worker on the track writes `team/status/<slug>.md` and sets
   contract `status: claimed`.

## Stand-down protocol

If the conductor steps down or hands off:

- Commit any in-flight changes to `team/` only.
- Leave a one-paragraph note at the top of `team/CONDUCTOR.md` naming the
  successor and the date.
- Successor runs the daily checklist and confirms `scripts/board-lint.sh`
  passes before merging the next worker PR.

## Tooling

- `scripts/board-lint.sh` — enforces contract ↔ worktree ↔ branch ↔
  ownership consistency. CI-friendly; non-zero exit on violation.

## Current conductor

`@latentwill` (Ed), assisted by Claude sessions in
`.worktrees/conductor/` when present.
