# 2026-05-16 Migration Archive

Frozen state of the execution board immediately before the process overhaul.

Contents:

- `execution-board-2026-05-13.md` — the previous monolithic dated board (40 tracks, 643 lines). All tracks listed there were complete-with-checkpoint or covered by merged PRs at archive time.
- `branch-audit.md` — full classification of 159 `origin/*` branches with delete-lists.
- `delete-merged.txt` — branches scheduled for batch deletion (merged via PR).
- `delete-closed.txt` — branches scheduled for batch deletion (closed PRs, work superseded).
- `manifest-historical-tables.md` — pre-overhaul `team/MANIFEST.md` row tables (Phase A and Phase B build-out historical state).

Spec that drove the migration: `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`.

## Wave-by-wave outcome

All four QA waves (Q4, Q8, Q9, Q10) were operationally complete by 2026-05-16. Per-wave PR status:

| Wave | Closed-out | Live PRs at archive |
|---|---|---|
| Q4 | All four tracks merged into `main` (`qa4-chat-eval-launcher`, `qa4-scenarios-4h-bars-ui`, `qa4-settings-zero-provider`, `qa4-surface-consistency`) | 0 |
| Q8 | All board tracks landed via #124 / #162 / #164 (combined Codex PRs) or superseded; individual `qa8-*` PRs closed unmerged on purpose | 0 |
| Q9 | All `qa9-*` PRs merged (#131–#161) | 0 |
| Q10 | All `qa10-*` PRs merged (#166–#180); chat/runtime recovery via #169 and #170 | 0 |

Eval-review wave is partially landed: `eval-review-data-model` merged (#176). `eval-review-agent-engine`, `eval-review-api-cli`, `eval-review-run-detail-ui` remain TODO and carry over into the post-migration active board.

## Re-opening an archived track

If a regression points at archived work:

1. Read this archive's `execution-board-2026-05-13.md` for the original scope and verification.
2. Read the corresponding `team/archive/status/<track>.md` for the implementing notes.
3. Open a new contract under `team/contracts/<new-track>.md` with a fresh slug — do not resurrect the old branch.
