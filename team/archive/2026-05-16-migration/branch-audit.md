# Branch audit — 2026-05-16

Generated as step 1 of the execution-board process overhaul
(`docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`).

## Summary

| Class | Count | Action |
|---|---:|---|
| Merged via PR (head still on origin) | 116 | Delete remote |
| Closed unmerged via PR (head still on origin) | 16 | Delete remote (work superseded or rolled into other PRs) |
| Merged into main with no PR record (older direct merges) | ~10 | Delete remote |
| No PR, work absorbed by `-clean` / split variants | ~10 | Keep for one pass, revisit |
| Live (`main`, `HEAD`) | 2 | Keep |

Open PRs at audit time: **0**.

Last 5 commits on `main`:

```
c5a3cf1 fix(eval): typed trader-output failures with raw provider diagnostics (#180)
722e77d fix(eval): drop 200-bar backtest warmup gate so short-window evals can replay (#177)
55e9f98 [codex] add browser console logging (#174)
4b02021 docs(board): add qa10-backtest-short-window-replay track
4ce3213 docs: document local image deploy ssh auth
```

## Method

- `gh pr list --state all --limit 300` to fetch all PR head refs by state.
- Intersected merged head refs with current `origin/*` branch list.
- Cross-checked against `git branch -r --merged origin/main` for non-squash merges (e.g. older `qa4-*` direct merges).
- Inspected `team/status/*.md` for tracks still claiming `in_progress` against merged PR numbers — all reconciled as merged or superseded.

## Delete lists

- `delete-merged.txt` — 116 branches whose PRs are merged.
- `delete-closed.txt` — 16 branches whose PRs were closed without merge (work rolled into successor PRs).

## Retained for follow-up

The following branches have no PR record but appear absorbed into `main` through later renames or `-clean` rewrites. Conductor decision: keep one wave, revisit next migration pass.

- `bars-fetch-ui` — cherry-pick source for `qa4-scenarios-4h-bars-ui`. Now redundant.
- `chat-rail-route-persistence` — superseded by chat-rail merges.
- `docs/qa-findings`, `docs/strategies-freqtrade-playlist` — docs-only, low risk.
- `eval-failure-diagnostics`, `eval-picker-readable-title` — exploratory; not referenced by any spec.
- `explore/eval-dispatch-honors-model-requirement` — exploratory.
- `hackathon/sample-strategies` — hackathon source; retain until V2A "example artifacts" track lands.
- `j/eval-xvnej-default-provider` — Hermes Agent branch, retain pending confirmation.
- `pr-94`, `pr94-chart-stabilization` — work in main via `pr94-chart-stabilization-clean` (#merged).
- `pwa-mobile-responsive` — mobile work in main via `mobile-safari-load-main` (#147).
- `spec/cli-config-and-dispatch-unification` — spec branch, content already in `docs/`.

## Out-of-scope: local feature branches

Local-only branches (`audit-post-merge`, `feature/eval-3b-progress`, several `qa-pass-2-*`) are not addressed by this pass. They are not on `origin`, do not block coordination, and can be pruned by the user with `git branch -D`.
