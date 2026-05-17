---
track: eval-running-animation
lane: leaf
wave: ux-polish
worktree: .worktrees/eval-running-animation
branch: task/eval-running-animation
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/components/primitives/Pill.tsx
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-compare.tsx
  - frontend/web/src/routes/home.tsx
  - frontend/web/src/routes/eval-runs.test.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/tailwind.config.ts
  - frontend/web/src/styles/globals.css
forbidden_paths:
  - crates/**
  - frontend/web/src/api/types.gen/**
  - frontend/web/src/features/onboarding/**
  - frontend/web/src/routes/docs/**
  - frontend/web/src/features/docs/**
  - frontend/web/src/routes/authoring.tsx
  - frontend/web/src/routes/settings/providers.tsx
interfaces_used:
  - components/primitives/Pill (tone, children)
  - RunStatus = "queued" | "running" | "completed" | "failed" | "cancelled"
parallel_safe: true
parallel_conflicts: []
verification:
  - cd frontend/web && npm run typecheck
  - cd frontend/web && npm run lint
  - cd frontend/web && npm test -- eval-runs eval-runs-detail
  - bash scripts/board-lint.sh
acceptance:
  - When an eval run's `status === "running"`, the status pill on the eval-runs list, eval-runs-detail header, eval-compare matrix, and the home recent-runs list renders with a visible animation (pulsing dot glow and/or a small spinner) that draws the eye to in-flight rows.
  - The `queued` state may share the animation or use a milder variant, but `completed`, `failed`, and `cancelled` remain static — no animation, no spinner.
  - Animation respects `prefers-reduced-motion`: when the user has reduced motion enabled, the running pill stays visually distinct (e.g. brighter/glow ring) but the motion stops (no pulse, no spin).
  - No layout shift when the pill transitions between static and animated states — pill width/height stays constant within ±1px.
  - Existing Pill `tone` API is unchanged. Either an `animated` prop is added (opt-in) or the running animation is driven by a co-located `RunningStatusPill` wrapper component — Pill itself must stay a stateless presentational primitive usable by non-eval callers.
  - The four routes (`eval-runs`, `eval-runs-detail`, `eval-compare`, `home`) share a single source for the running indicator (no copy-paste of keyframes / class strings across files).
  - Tests: at minimum, one render test asserts that a row with `status: "running"` carries the animation marker (class, data attribute, or aria-busy) and a row with `status: "completed"` does not. Existing eval-runs / eval-runs-detail tests continue to pass.
  - Dark mode borders stay theme-token-driven (no `border-white` / hex whites). See `/Users/edkennedy/Code/CLAUDE.md` dark mode rule.
---

# Scope

Today the four surfaces that render an eval run status (`eval-runs` list, `eval-runs-detail` header, `eval-compare` matrix, `home` recent-runs) all show the same static `<StatusPill status="running" />` — a static info-toned pill with a colored dot. The pill that says "running" doesn't read as live work; on a list of 30 historical runs there's no visual difference between an actively running row and one that finished hours ago, so users miss the cue and have nothing to anchor their attention while they wait for results.

This track adds a small motion / glow affordance to the "running" state so an in-flight eval is immediately recognizable at a glance — pulsing the indicator dot, optionally adding a spinner glyph, and giving the pill a soft glow ring. The change is confined to the presentational layer; eval data, polling cadence, and store/query plumbing are not touched.

Implementation sketch (worker is free to choose either option):

1. **`animated` prop on `Pill`** — add an optional `animated?: boolean` prop that toggles a `running-pulse` class (and the spinner inside the dot). Each existing `StatusPill` helper sets `animated={status === "running"}`.
2. **`RunningStatusPill` wrapper** — keep `Pill` untouched; introduce a small co-located component that renders the dot + spinner + glow ring when `status === "running"` and falls back to plain `Pill` otherwise. Each route's local `StatusPill` defers to it.

Either way: a single `@keyframes` lives in `globals.css` (or in `tailwind.config.ts` under `theme.extend.animation`), guarded by `@media (prefers-reduced-motion: reduce)`.

# Out of scope

- Backend status semantics. `RunStatus` enum, the daemon, eval executor, and the API surface are untouched.
- Polling cadence. The 2000 ms refetch in `eval-runs-detail.tsx:37` and the conditional polling in `eval-runs.tsx:74-87` stay as-is.
- Cancellation UX, retry buttons, progress bars, ETA displays — separate UX tracks, not bundled.
- New colors / theme tokens. Reuse `--info`, `--gold`, etc. that already drive `STATUS_TONE`.
- Daemon-status / scenario-bars `useBarsFetchJob.ts` running state. Different surface; if it needs the same treatment, it lands as a follow-up so this contract stays small.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/eval-running-animation -b task/eval-running-animation origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
cd .worktrees/eval-running-animation
git log --oneline -3 origin/main..HEAD   # must be empty before any edits
```

# Notes

- Claimed 2026-05-16 by @latentwill (Ed) per request to add a Running animation to the Eval surface.
- Status pill callsites to update (one per file):
    - `frontend/web/src/routes/eval-runs.tsx:888` (StatusPill)
    - `frontend/web/src/routes/eval-runs-detail.tsx` around `:246` (tone + inflight already computed)
    - `frontend/web/src/routes/eval-compare.tsx:146`
    - `frontend/web/src/routes/home.tsx:290`
- Pill primitive lives at `frontend/web/src/components/primitives/Pill.tsx` — single source for any new `animated` prop.
- Reuse the existing `--info` token; do not introduce a new color.
- See workspace CLAUDE.md "Dark mode borders" rule when adding any glow ring — use theme variables, not `border-white`.
