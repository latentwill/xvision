---
track: eval-runs-ux
worktree: /Users/edkennedy/Code/xvision (main worktree)
branch: feature/eval-runs-ux
phase: phase-a-implementation
last_updated: 2026-05-11T02:02:59Z
owner: claude-opus-4-7 (1M ctx) — v1-gaps Tracks B+C+D bundle
---

# What I'm doing right now

Bundling Tracks B (row drill-in), C (Compare selection), D (error state)
into one PR. All three modify `frontend/web/src/routes/eval-runs.tsx`;
spec recommends the bundle.

## Plan task progress

- [x] Claim posted to `team/queue/`
- [x] Branch `feature/eval-runs-ux` from `origin/main` @ `0fff672`
- [ ] Read full `routes/eval-runs.tsx`
- [ ] B: rows navigate to `/eval-runs/:runId`
- [ ] C: per-row checkboxes + Compare(n) button
- [ ] D: render-order fix (loading / error / empty / table)
- [ ] `tsc -b` + `vite build` green
- [ ] Live smoke: `/api/eval/runs` returns clean shape against booted dashboard
- [ ] Commit + PR + pr-open queue note

# Blocked on

Nothing.

# Followup available

After this PR lands:
- Track E (Inspector "Run eval" CTA) — independent
- Track F (Settings → Danger real impl) — independent
- Track G (audit + health test coverage) — independent
- Track H (Strategies disabled-button affordance) — independent
