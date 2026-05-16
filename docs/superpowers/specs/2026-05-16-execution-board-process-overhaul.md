# Execution Board Process Overhaul — Spec

Date: 2026-05-16
Status: Draft for implementation

## Goal

Fix the process conflicts that show up when multiple Claude/Codex sessions work
the xvision execution board in parallel. The root cause is **not** the number of
parallel workers — it's that dependencies, file ownership, and lifecycle state
all live in narrative prose, and the board never gets pruned. Tighten contracts,
make collision zones explicit, archive ruthlessly, and define a single conductor
role.

This spec is xvision-specific. It treats the workspace as it actually is, not as
a generic git-flow problem.

## Current-state evidence

| Artifact | Reality | Friction |
|---|---|---|
| `team/execution-board-2026-05-13.md` | 40 tracks, 643 lines, 4 QA waves (Q4/Q8/Q9/Q10), most rows already checkpointed | Active tracks lost in a sea of completed ones |
| Branches on `origin` | ~160 | No archive policy; stale branches confuse rebases |
| Open PRs | 0 at time of writing | Friction isn't PR queue — it's pre-PR coordination |
| Worktrees | One per track under `.worktrees/<track>` — already enforced | Working; keep |
| `team/queue/` | Append-only `<track>__<utc>__<topic>.md` claim/PR-open files | Working; keep |
| `team/status/<track>.md` | Per-track current status | Working; keep |
| `team/briefings/<track>.md` | 12 files, all from Phase A (engine-api, broker-surface, …) | Stale; not regenerated as new tracks spawn |
| Dependency expression | "Wait for …" / "Do not overlap …" prose lists | Not machine-checkable; collisions slip through |
| File ownership | Implicit only | No way to allocate non-overlapping tracks at planning time |
| Conductor role | Operator (Ed) doing it ad hoc | No artifact ownership; no defined hand-off |

## Evaluation of submitted suggestions

| # | Suggestion | Verdict | Notes |
|---|---|---|---|
| 1 | Trunk-based, short-lived task branches | **Adopt** | Already de facto. Make it policy + enforce branch deletion after merge. |
| 2 | Foundation / Leaf / Integration lanes | **Adopt** | Make the lane an explicit field on each track row. Foundation tracks gate downstream waves. |
| 3 | Task contracts before agents start | **Adopt with template** | Replace the thin board row with a per-track contract file. Existing row columns become the contract header. |
| 4 | Ownership map | **Adopt** | Add `team/OWNERSHIP.md`. Reference paths, not directories of file owners. |
| 5 | Merge queue (bors/Mergify/GH merge queue) | **Defer** | Overkill at current PR cadence (0 open). Revisit when ≥4 mergeable PRs sit simultaneously for >24h. |
| 6 | Avoid stacked PRs unless declared | **Adopt as policy** | Already implicit; add the stacking-declaration rule to the contract template. |
| 7 | Worktrees per track | **Already in place** | Document the `git worktree add … origin/main` ritual in the conductor playbook. |
| 8 | Sync-before-work ritual | **Adopt** | Encode as `team/briefings/_template.md` opening section. |
| 9 | Conflict-zone registry | **Adopt** | Add `team/CONFLICT_ZONES.md` with high-collision files (migrations, route registries, package manifests, generated types). Only one active track may touch any listed file. |
| 10 | Board columns including Needs Rebase / Scope Violation | **Adopt simpler** | Don't introduce a Kanban tool. Encode status as a single `phase:` field in `team/status/<track>.md`, with a fixed vocabulary. |
| 11 | Conductor / architect role | **Adopt** | One named role, no code, owns OWNERSHIP/CONFLICT_ZONES/board updates. |
| 12 | Practical recovery sequence | **Adopt with xvision-specific steps** | See "Migration" below. |

## Target operating model

### Three lanes

Every track is exactly one of:

- **Foundation** — touches shared types, DB migrations, API contracts, central
  config, the agent/eval engine core, or the workspace `Cargo.toml`. Foundation
  tracks merge before any track that depends on them. At most one Foundation
  track is active per cohort area at a time.
- **Leaf** — touches an isolated feature folder under
  `frontend/web/src/features/<X>/**`, a single CLI subcommand,
  a single Rust crate's leaf module, or test/docs only. Leaf tracks run in
  parallel up to the Foundation→Leaf gate.
- **Integration** — wires Foundation to Leaves. Lands last. Owns the smoke /
  e2e verification.

A track that "feels like" both Foundation and Leaf is mis-scoped; the
conductor splits it.

### Waves (cohorts)

QA waves (Q4, Q8, Q9, Q10, future Qn) are **cohorts**, not labels. A wave:

1. Opens with an intake doc under `team/intake/<date>-q<N>.md` (raw operator
   report, untouched).
2. The conductor decomposes intake into Foundation/Leaf/Integration tracks
   and writes a per-track contract.
3. The wave **closes out** as a unit: when the last Integration track lands,
   the wave moves to `team/archive/<date>-q<N>/` with the final board snapshot
   and per-track status files. Wave-N rows leave the active board the day they
   close.

### Track contract

Each active track owns a single file at `team/contracts/<track>.md`. The
existing board row collapses into a one-line index entry pointing at this file.

```markdown
---
track: qa10-stop-eval-run-control
lane: integration              # foundation | leaf | integration
wave: q10
worktree: .worktrees/qa10-stop-eval-run-control
branch: task/qa10-stop-eval-run-control
base: origin/main
status: in-progress            # ready | claimed | in-progress | pr-open | needs-rebase | merged | archived | blocked
depends_on:
  - qa10-eval-trader-empty-output-resilience
blocks: []
stacking: none                 # none | declared:<parent-track>
allowed_paths:
  - crates/xvision-engine/src/eval/executor/
  - crates/xvision-dashboard/src/routes/eval/
  - frontend/web/src/features/eval-runs/
forbidden_paths:
  - crates/xvision-engine/migrations/
  - frontend/web/src/features/chat/
interfaces_used:
  - EvalRunStore::cancel
  - SSEStream::terminal_event
parallel_safe: false
parallel_conflicts:
  - qa10-eval-trader-empty-output-resilience  # both touch executor.rs
verification:
  - cargo test -p xvision-engine eval::executor::cancel
  - pnpm --dir frontend/web test -- eval-runs-detail
acceptance:
  - Stop button visible for queued/running runs only
  - Cancel is idempotent across repeated clicks
  - Executor checks terminal status before model call / fill / metric update
  - SSE closes with a terminal status event
---

# Scope

(one paragraph — what is this track doing and why)

# Out of scope

(explicit list of things this track will NOT touch, even if tempting)

# Sync-before-work ritual

```bash
git fetch origin
git status                              # must be clean
git worktree list                       # confirm we own this worktree
git -C .worktrees/<track> log --oneline -3 origin/main..HEAD
```

# Notes

(free-form — checkpoints, surprises, links to PRs)
```

The frontmatter is the contract. The body is for the human (or agent) doing
the work. A worker not editing files outside `allowed_paths` is the contract;
everything else is commentary.

### Single-vocabulary status

`team/status/<track>.md` `phase:` field uses one of these values only:

`ready | claimed | in-progress | pr-open | needs-rebase | merged | archived | blocked | scope-violation`

Any other value is an error. The conductor's daily pass treats anything
older than 72h still in `claimed` as a candidate for reassignment.

### File ownership map

`team/OWNERSHIP.md` maps **specific file globs** to **specific tracks that
own them this wave**. Example:

```markdown
| Path                                                       | Owning track (current)             | Wave |
|------------------------------------------------------------|------------------------------------|------|
| crates/xvision-engine/migrations/**                        | foundation:next-numbered-only      | —    |
| crates/xvision-engine/src/eval/executor/backtest.rs        | qa10-backtest-short-window-replay  | q10  |
| crates/xvision-engine/src/eval/executor/paper.rs           | qa10-stop-eval-run-control         | q10  |
| frontend/web/src/features/eval-runs/**                     | qa10-eval-chat-scrollbars-controls | q10  |
| frontend/web/src/features/chat-rail/**                     | qa10-chat-scenario-dsml-recovery   | q10  |
| frontend/web/src/themes/**                                 | color-themes-light-dark            | —    |
| crates/xvision-cli/src/**                                  | qa8-cli-* (multi-owner, see below) | q8   |
```

For multi-owner areas, the row lists every active owning track. The conductor
ensures non-overlap **within the listed area** (e.g., qa8 CLI tracks split by
subcommand file, not by feature concept).

### Conflict-zone registry

`team/CONFLICT_ZONES.md` lists files where **only one active track may write**
at a time, regardless of ownership:

- `crates/xvision-engine/migrations/**`
- `crates/xvision-engine/src/eval/store.rs` (single struct, single mutex of changes)
- `frontend/web/src/routes.tsx` and `frontend/web/src/App.tsx` (route registry)
- `Cargo.toml` (workspace), `frontend/web/package.json`
- `frontend/web/src/features/eval-runs/eval-runs-detail.tsx` (eval-runs hot
  page; many Q10 tracks land here)
- Any `index.ts` / `mod.rs` that is a re-export registry

Workers reading a contract whose `allowed_paths` includes a conflict-zone path
must check the registry's "current claim" line before editing.

### Conductor role

One conductor at a time. Responsibilities:

1. Owns `team/OWNERSHIP.md`, `team/CONFLICT_ZONES.md`, the active board index,
   and `team/contracts/*.md` headers.
2. Decomposes new intake into Foundation/Leaf/Integration tracks.
3. Assigns lanes; rejects scope-violating contracts.
4. Decides merge order; updates `depends_on` and `blocks` as PRs land.
5. Closes out waves; moves the wave to `team/archive/<date>-q<N>/`.
6. Does **not** write feature code. May write process tooling (board scripts,
   ownership lint).

The conductor is a human role today; a Claude session in
`.worktrees/conductor/` may assist. The conductor never holds a feature
worktree.

### Branch lifecycle

- Task branches: `task/<track-slug>` (replaces the current ad-hoc
  `feature/…`, `qa-pass-…`, `codex/…` prefixes for new tracks).
- Source from `origin/main` only. No stacking unless `stacking: declared:…`
  in the contract.
- On merge: delete the remote branch the same day. Local worktree's branch
  may linger; the worktree itself is removed within 7 days.
- Old branches (any `feature/*`, `codex/*`, `qa-*` not referenced by an
  active contract) are candidates for archive deletion in the migration
  step.
- Long-running checkpoint branches (e.g., `strategy-agent-backend-core`)
  declare themselves as Foundation in their contract; they may live beyond
  one wave but must rebase weekly.

### PR policy

- One PR per track. PR title format: `[<lane>] <track>: <one-line summary>`.
- PR description includes: link to contract, list of acceptance criteria with
  checkboxes, verification output.
- A PR that touches files outside its contract's `allowed_paths` is closed,
  not merged. The conductor either widens the contract (and the worker
  resubmits) or splits the PR.
- Stacked PRs allowed only when both parent and child contracts have matching
  `stacking:` declarations.
- Defer merge queue tooling until ≥4 mergeable PRs sit simultaneously >24h.

## Repo artifacts to create

| Path | Purpose | Initial content |
|---|---|---|
| `team/contracts/_template.md` | Track contract template (frontmatter above) | Filled-in template |
| `team/contracts/<track>.md` | One per active track | Conductor seeds from current board rows |
| `team/OWNERSHIP.md` | File-glob → owning track | Seeded from current Q10 wave |
| `team/CONFLICT_ZONES.md` | High-collision files | Seeded from board's "Do not overlap" list |
| `team/intake/<date>-q<N>.md` | Raw wave intake | Move existing "Q8 QA intake" / "Q10 QA intake" sections out of board |
| `team/archive/<date>-q<N>/` | Closed waves | Q4 and Q8 complete-row state moves here on migration day |
| `team/board.md` | New live board — index only | One line per active track linking to contract |
| `team/briefings/_template.md` | Briefing template w/ sync ritual | Replaces ad-hoc briefing format |
| `team/CONDUCTOR.md` | Role definition + daily checklist | New |
| `scripts/board-lint.sh` | CI script: verify contracts ↔ ownership ↔ branches consistent | New |

Files retired or moved:

- `team/execution-board-2026-05-13.md` → archive snapshot. Replaced by
  `team/board.md` + `team/contracts/*`.
- `team/MANIFEST.md` Phase-A/B historical sections — keep as
  `team/archive/2026-05-13-phase-a-b.md`; remove from MANIFEST.
- Stale per-track briefings under `team/briefings/` that have no matching
  active contract → `team/archive/briefings/`.

## Migration (today → target state)

Numbered steps, each landable as its own small PR:

1. **Branch audit & archive list.** Generate
   `team/archive/2026-05-16-branch-audit.md` listing every branch with: last
   commit date, last commit author, merge status vs `origin/main`, matching
   contract (if any). Output of `git for-each-ref --sort=-committerdate
   refs/remotes/origin --format '%(refname:short)\t%(committerdate:iso)\t%(authorname)'`.
2. **Close Q4 and Q8.** Both waves are functionally complete per the existing
   board. Move row state to `team/archive/2026-05-16-q4/` and `…-q8/`. Delete
   merged remote branches; leave unmerged checkpoint branches with a written
   note in the archive.
3. **Bootstrap contracts for active tracks only.** Convert each currently-active
   Q9, Q10, eval-review, and color-themes row into a `team/contracts/<track>.md`
   file. About 12 contracts, not 40.
4. **Write `team/OWNERSHIP.md` from the active 12.** Conductor walks each
   contract's `allowed_paths` and produces the inverse map.
5. **Write `team/CONFLICT_ZONES.md` from the board's existing "Do not overlap"
   list.** This already names most collision files; convert prose to table.
6. **Write `team/CONDUCTOR.md`.** Define role, daily checklist, hand-off
   protocol.
7. **Add `scripts/board-lint.sh`.** First version checks:
   - Every active contract has a worktree on disk
   - Every active contract's branch exists on `origin`
   - No two active contracts list the same `allowed_path` without both being
     in `CONFLICT_ZONES.md`'s multi-owner exemption list
   - `team/status/<track>.md` `phase:` is in the allowed vocabulary
8. **Cut `team/board.md`.** One line per active track:
   `- [qa10-stop-eval-run-control](contracts/qa10-stop-eval-run-control.md) — integration, q10, in-progress`.
9. **Branch cleanup pass.** Delete remote branches from step 1's audit that
   are: (merged into main) OR (last commit >30 days old AND no active
   contract). Push deletes in one batch with a written audit trail.
10. **Update CLAUDE.md.** Point new workers at `team/board.md` and
    `team/CONDUCTOR.md`; retire references to the dated execution board.

## Acceptance criteria

- `team/board.md` exists; the dated execution board moves to archive.
- Every active track has a `team/contracts/<track>.md` with a valid
  frontmatter contract.
- `team/OWNERSHIP.md` and `team/CONFLICT_ZONES.md` exist and are consistent
  with active contracts.
- `team/CONDUCTOR.md` defines the role; a named conductor is documented.
- `scripts/board-lint.sh` runs clean against the post-migration state.
- Q4 and Q8 waves are archived under `team/archive/`.
- Branch count on `origin` reduced to ≤ (count of active contracts + 10
  long-lived).
- A new worker starting a track can complete the sync-before-work ritual
  using only the contract file, with no need to read the dated board.

## What this spec does **not** do

- Introduce a hosted merge-queue tool. Deferred until PR cadence justifies it.
- Replace the file-based queue with MCP/subagent messaging. The board explicitly
  warned that's flaky; keep filesystem coordination.
- Rename existing CLI verbs or shipped artifacts. This is process only.
- Touch the workspace-level `Code/CLAUDE.md` or any other project's process.

## Risks

- **Conductor becomes a bottleneck.** Mitigation: conductor only writes
  contracts and ownership; does not gate PRs (reviewers do). Daily checklist
  is ≤30 minutes.
- **Workers cargo-cult the template and produce shallow contracts.**
  Mitigation: `scripts/board-lint.sh` will flag empty `allowed_paths` and
  missing `verification`.
- **`allowed_paths` drift from reality during a task.** Mitigation: when a
  worker realizes the contract is wrong, they push a contract update PR
  **before** the code PR. The contract update is a normal review.
- **Archive churn.** Mitigation: archive is append-only, dated, never edited
  in place. Cheap to ignore if you don't need it.

## Open questions for the user

(None — author proceeded with reasonable defaults per "no clarifying questions"
mode. The user redirects in review if a default is wrong.)
