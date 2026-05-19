---
track: stale-chunk-import-retry
lane: leaf
wave: qa-operator-2026-05-19
worktree: .worktrees/stale-chunk-import-retry
branch: task/stale-chunk-import-retry
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes.tsx
  - frontend/web/src/lib/chunk-reload.ts
  - frontend/web/src/lib/chunk-reload.test.ts
  - frontend/web/src/App.tsx
  - frontend/web/src/components/AppErrorBoundary.tsx
  - frontend/web/src/components/AppErrorBoundary.test.tsx
forbidden_paths:
  - frontend/web/src/routes/**
  - frontend/web/src/features/**
  - frontend/web/vite.config.ts
  - frontend/web/src/api/**
  - crates/**
interfaces_used:
  - lazy() route registration in routes.tsx
  - sessionStorage (guard against reload loops)
  - toast notification surface (existing; per the no-popup-rule
    exception)
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run chunk-reload AppErrorBoundary
  - pnpm --dir frontend/web build
acceptance:
  - New helper `frontend/web/src/lib/chunk-reload.ts` exposes:
    - `isChunkLoadError(error: unknown): boolean` — detects the Vite
      lazy-import failure shape. Matches at least:
      - `TypeError: Failed to fetch dynamically imported module`
      - `Failed to fetch dynamically imported module` substring in
        `error.message`
      - `error.name === "ChunkLoadError"` (covers other bundlers)
    - `attemptChunkReload(error): boolean` — when the error is a
      chunk-load error AND `sessionStorage["xvn:chunk-reload-attempted"]`
      is not set, sets the flag and triggers `window.location.reload()`.
      Returns `true` if a reload was triggered, `false` otherwise.
      The flag prevents reload loops; cleared on a successful page
      lifecycle event (`visibilitychange` transitioning to visible AND
      no chunk error in the prior session — pick a clear-trigger and
      document it).
    - `noteSuccessfulPageLoad(): void` — called on app boot AFTER any
      lazy chunk imports successfully resolve, clears the
      reload-attempted flag so a future deploy reload is allowed.
  - `frontend/web/src/components/AppErrorBoundary.tsx` (new or
    extension of an existing top-level boundary): catches errors
    bubbling from the `Suspense` children in `routes.tsx`. When the
    error is a chunk-load error, calls `attemptChunkReload`. When
    the reload succeeds (i.e., the function returned `true`), renders
    a minimal "Updating..." placeholder while the reload happens.
    When the reload was already attempted this session (returns
    `false`), falls through to the existing error-render path with
    a hint: "Reload didn't recover — please refresh the page
    manually or contact support".
  - Post-reload, a one-shot toast surfaces: "App was updated —
    reloaded to the latest version". The toast must use the existing
    toast surface (sonner / react-hot-toast / whatever the project
    already uses); do NOT add a new dependency. The toast is the
    documented exception to the no-popup rule.
  - `routes.tsx` wraps the lazy route boundary in the new error
    boundary so chunk-load errors are caught and routed through
    `attemptChunkReload` rather than crashing to the global error
    UI.
  - Unit tests:
    - `isChunkLoadError` returns true for the three documented error
      shapes and false for unrelated errors.
    - `attemptChunkReload` triggers reload once per session and
      no-ops on subsequent calls within the same session
      (sessionStorage flag is checked).
    - `AppErrorBoundary` component test: a thrown
      `ChunkLoadError`-shaped error inside a child triggers the
      reload path (mock `window.location.reload`); an unrelated
      error falls through to the existing error render.
  - Manual repro path documented in the status note: deploy a new
    bundle (or rename a chunk hash by hand on a local build), open
    a tab against the old `index.html`, navigate to a lazy route,
    observe the auto-reload + toast.
  - No new dependencies. No changes to vite config (build-id polling
    is a deferred follow-up; this track is reload-on-failure only).
  - No `try/catch` silencing of unrelated errors
    (`feedback_alpha_root_cause`). The error-boundary only
    redirects errors it identifies as chunk-load failures; all
    other errors fall through unchanged.
parallel_safe: true
parallel_conflicts:
  - "frontend/web/src/App.tsx and frontend/web/src/routes.tsx: file-level conflicts possible with any concurrent frontend track. Coordinate via team/queue/ if another track is editing these files."
---

# Scope

Operator hit `TypeError: Failed to fetch dynamically imported module:
https://xvn.tail2bb69.ts.net/assets/scenarios-new-5cnT8cD7.js` on
2026-05-19. Classic Vite-SPA-after-deploy: the running tab holds the
old `index.html` referencing the old hashed chunk filename, which the
new build replaced. Hard refresh recovered (new `index.html` ships
new hashes), but the SPA does nothing automatic — the operator has
to know to refresh.

This track adds an error boundary that catches the
chunk-fetch failure and auto-reloads once per session, with a toast
on the post-reload to explain what happened.

Anchor reading:

- `team/intake/2026-05-19-qa-operator-round-4.md` "Round-4 addendum"
  section, item 4 (Finding A).
- `frontend/web/src/routes.tsx` — current lazy route registration.

# Out of scope

- Vite config changes (build-id meta tag, polling on focus/route-change).
  Better UX in the long run; defer as a follow-up. This track ships
  the reactive recovery; the proactive recovery is a separate concern.
- Service worker / PWA install — out of scope.
- Backend changes — the build pipeline already emits hashed chunks
  correctly; the issue is purely client-side recovery.
- Adding a new toast library. Use the existing one.
- Reload loops past N=1 attempts per session. v1 deliberately bounds
  to one reload; if the reload itself fails, the user gets the manual
  refresh prompt.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/stale-chunk-import-retry status
git -C .worktrees/stale-chunk-import-retry log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/stale-chunk-import-retry \
  -b task/stale-chunk-import-retry origin/main
```

# Notes

Append checkpoints / PR links below. The clear-trigger for the
session reload-attempted flag is acceptance-bearing — document the
choice in the status note before opening the PR.
