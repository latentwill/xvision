# Team coordination workflow

Multiple Claude sessions land work in parallel. The `team/` directory is the
single source of truth for who owns what and what's in flight.

Spec: `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`.

## File map

| Artifact | Purpose | Owner |
|---|---|---|
| `team/board.md` | Active execution board (current wave, one line per active track) | Conductor |
| `team/board-v2.md` | V2 roadmap board (V2A active, V2B+ not yet decomposed) | Conductor |
| `team/MANIFEST.md` | Top-level pointers + migration registry | Conductor |
| `team/OWNERSHIP.md` | File-glob → owning track | Conductor |
| `team/CONFLICT_ZONES.md` | Single-writer file registry | Conductor |
| `team/CONDUCTOR.md` | Conductor role + daily checklist | Conductor |
| `team/contracts/<slug>.md` | Per-track contract | Conductor owns frontmatter; worker owns body |
| `team/status/<slug>.md` | Per-track current status | Worker |
| `team/queue/<from>__<utc>__<topic>.md` | Append-only inter-track message | Sender |
| `team/briefings/_template.md` | Sync-before-work briefing template | Read-only |
| `team/intake/<date>-<wave>.md` | Raw wave intake before decomposition | Conductor |
| `team/archive/<date>-<wave>/` | Frozen state of closed-out waves | Read-only after creation |
| `scripts/board-lint.sh` | CI/local consistency check | — |

## Cold-start (worker)

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
cat team/board.md             # current wave
cat team/board-v2.md          # V2 roadmap + V2A active
cat team/contracts/<slug>.md  # read the contract you intend to claim
cat team/briefings/_template.md
```

Then write `team/status/<slug>.md` and set the contract's `status:` to
`claimed`.

## Worktree pattern

```bash
git fetch --prune origin
git worktree add .worktrees/<slug> -b task/<slug> origin/main
```

Set `CARGO_TARGET_DIR=$HOME/.cargo-target/xvision` inside the worktree to
avoid creating a duplicate `target/` tree (see SKILL.md / `/CLAUDE.md`).

## Contract status lifecycle

```
ready ──▶ claimed ──▶ in-progress ──▶ pr-open ──▶ merged
```

The worker can only edit the contract **body** (Notes section, checkpoints).
Frontmatter changes — including `status:` transitions past `claimed` — should
match the actual workflow state and go through the conductor on disputed
edits.

## Daily conductor checklist (target ≤ 30 min)

1. `git fetch --prune origin` and read `team/board.md`.
2. `bash scripts/board-lint.sh` — expect green.
3. Read each active contract's `team/status/<track>.md`:
   - `claimed` > 72h with no `in-progress` update → reassign or escalate.
   - `needs-rebase` → confirm rebase is queued; if blocked, file a queue note.
4. Reconcile `gh pr list --state open` with contract `status:` fields. PRs
   should be `pr-open` while open; flip to `merged` the day they land.
5. For any contract that became `merged` today:
   - Move its row to `team/archive/<date>-<wave>/contracts/`.
   - Move the status file to `team/archive/<date>-<wave>/status/`.
   - Release any rows it held in `team/CONFLICT_ZONES.md`.
   - Run the branch cleanup step (delete `task/<slug>` on origin).
6. If a wave's intake is sitting in `team/intake/`, decompose into contracts
   before opening more leaf tracks.

## Conductor out-of-bounds

When acting as conductor, do **not** edit:

- Feature code in `crates/**` or `frontend/web/src/**`.
- Specs in `docs/superpowers/specs/` or plans in `docs/superpowers/plans/`.
- `team/status/<track>.md` (worker-owned).

Conductor may write process tooling (`scripts/board-lint.sh` and helpers
under `scripts/board/`).

## Wave lifecycle (conductor view)

1. **Intake** — raw operator/QA report lands in `team/intake/<date>-<wave>.md`.
2. **Decomposed** — conductor writes one contract per track, registers
   ownership and conflict zones.
3. **In-flight** — contracts move `ready → claimed → in-progress → pr-open → merged`.
4. **Closed-out** — last Integration track lands. Archive the wave under
   `team/archive/<date>-<wave>/`. Drop the wave row from `team/board.md`.

## Pre-push checklist (worker)

- `bash scripts/board-lint.sh` clean.
- `cargo test --workspace` (or scoped `cargo test -p <crate>` if your contract
  is leaf and the workspace tests have unrelated failures noted on `origin/main`).
- Frontend changes: `pnpm -C frontend/web typecheck && pnpm -C frontend/web test`.
- No edits outside the contract's `allowed_paths` without a queue note
  flagging it.
- PR body restates contract scope + lists any scope-drift exceptions.
