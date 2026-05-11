---
track: inspector-run-cta
worktree: /Users/edkennedy/Code/xvision (main worktree)
branch: feature/inspector-run-cta
phase: phase-a-implementation
last_updated: 2026-05-11T03:14:12Z
owner: claude-opus-4-7 (1M ctx) — v1-gaps Track E
---

# What I'm doing right now

Track E of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md` —
Inspector right-rail "Run eval" CTA. Surfaces the CLI command +
clipboard copy + link to `/eval-runs`.

## Plan task progress

- [x] Claim posted to `team/queue/`
- [x] Branch `feature/inspector-run-cta` from `origin/main` @ `b74b657`
- [x] Added `RunEvalCard` between ValidationCard and BackLinkCard
- [x] `tsc -b` + `vite build` green
- [ ] Commit + PR + pr-open queue note

# Blocked on

Nothing.

# Followup available

- Track H (Strategies disabled-button affordance) — `routes/strategies.tsx`
- Track F (Settings Danger) — being worked elsewhere per operator
- Per-strategy filtering of `/eval-runs` — defer until after Tracks B+C
  (PR #65) and this PR both merge (avoids same-file churn)
