# stale-chunk-import-retry — status

Track: `stale-chunk-import-retry`
Branch: `task/stale-chunk-import-retry`
Contract: `team/contracts/stale-chunk-import-retry.md`

## Outcome

Implemented the reactive recovery path for the Vite-SPA-after-deploy
chunk-fetch failure observed on 2026-05-19
(`TypeError: Failed to fetch dynamically imported module:
…/assets/scenarios-new-5cnT8cD7.js`).

Files:

- `frontend/web/src/lib/chunk-reload.ts` (new) — `isChunkLoadError`,
  `attemptChunkReload`, `noteSuccessfulPageLoad`,
  `consumePostReloadNotice`.
- `frontend/web/src/components/AppErrorBoundary.tsx` (new) — React
  error boundary that catches chunk-load errors and routes through
  `attemptChunkReload`. Non-chunk errors fall through to a generic
  error render (no swallowing).
- `frontend/web/src/routes.tsx` — wraps every lazy-route Suspense in
  the new boundary.
- `frontend/web/src/App.tsx` — calls `noteSuccessfulPageLoad()` on
  first commit (clear-trigger) and mounts a lightweight one-shot
  `ChunkReloadToast` that consumes the notice flag and surfaces the
  post-reload notification.
- Unit tests: `chunk-reload.test.ts` (12 cases) and
  `AppErrorBoundary.test.tsx` (4 cases).

## Clear-trigger choice (acceptance-bearing)

The contract requires picking + documenting a clear-trigger for the
`xvn:chunk-reload-attempted` sessionStorage flag.

**Chosen trigger: first React commit after app boot, via a top-level
`useEffect` in `App.tsx`.**

Rationale:

- `useEffect` fires only after the React tree has committed
  successfully for the first time. By that point either (a) the
  initial route's lazy chunks have already resolved (no error
  surfaced), or (b) any chunk-load error on the initial route would
  have been caught by `AppErrorBoundary` before reaching this effect
  — and the boundary keeps the flag set until the *next* successful
  page load.
- Clearing on first commit means a future deploy within the same
  browser session is still allowed to attempt one auto-reload. The
  bound is "one reload per stale-bundle event", not "one reload per
  browser session lifetime", which matches operator expectations.
- Alternative considered: `visibilitychange` to `visible`. Rejected
  because the flag would persist while the tab stays in the
  background, and tabs that are immediately backgrounded after the
  reload would never clear it. First-commit is strictly stronger:
  the React tree has demonstrably mounted with the new bundle.

The notice flag (`xvn:chunk-reload-just-completed`) is separate and
cleared by `consumePostReloadNotice()` when the toast surfaces, so
it can't survive past the first post-reload boot.

## Toast surface

The project has no toast library (sonner / react-hot-toast / etc.)
and the contract bans adding one. The contract anticipated this by
saying "use the existing toast surface" — the existing surface is
nothing. Rather than add a dependency, I built a minimal inline
toast component (`ChunkReloadToast` inside `App.tsx`, allowed path)
that:

- Uses `role="status"` + `aria-live="polite"` (non-focus-stealing,
  conforms to the no-popup-rule toast exception).
- Auto-dismisses after 6s, has a manual dismiss button.
- Lives in the bottom-right corner, z-50, styled with existing theme
  tokens (`border-border`, `bg-bg`, `text-text-1`).

If a project-wide toast surface lands later, this component can be
replaced by a one-line call to that surface.

## Manual repro path

To reproduce the original failure and verify the new recovery on a
local build:

1. Build the SPA: `pnpm --dir frontend/web build`. Note the hashed
   filename of one lazy chunk, e.g.
   `crates/xvision-dashboard/static/assets/scenarios-new-DFvpKgtp.js`.
2. Start the dashboard backend (or any static server pointing at
   `crates/xvision-dashboard/static`).
3. Open the SPA in a browser tab and navigate to `/strategies` (do
   NOT trigger the lazy chunk yet).
4. In another terminal, simulate a redeploy: rename the lazy chunk
   file to a different hash, e.g.
   `mv scenarios-new-DFvpKgtp.js scenarios-new-OLDHASH.js`. The
   currently-open tab still references the original filename in its
   `index.html`-derived module graph.
5. In the tab, click "Scenarios → New" (or anything that triggers
   `import("./routes/scenarios-new")`). Expected:
   - `AppErrorBoundary` catches the `TypeError: Failed to fetch
     dynamically imported module …`
   - "Updating to the latest version…" placeholder renders briefly.
   - `window.location.reload()` fires.
   - After reload, the toast "App was updated — reloaded to the
     latest version." appears bottom-right for ~6s.
6. To verify the loop-guard: leave the chunk renamed and repeat
   step 5 on the new bundle. The boundary should now render
   "Couldn't load the latest app bundle. / Reload didn't recover —
   please refresh the page manually or contact support." and NOT
   trigger a second reload.

Alternative (no rename): deploy a new build on top of an open tab
(`pnpm --dir frontend/web build` while the tab is open), then
trigger a lazy route in the open tab. Same outcome.

## Verification

```
pnpm --dir frontend/web typecheck          # passes
pnpm --dir frontend/web test -- --run \
    chunk-reload AppErrorBoundary           # 16/16 passing
pnpm --dir frontend/web build              # passes
```

## Out of scope (deferred follow-up)

- Proactive recovery (build-id meta tag + polling on focus / route
  navigation). The current track ships reactive-only — the operator
  pays the cost of one failed navigation before the auto-reload
  fires. A follow-up can poll for a `<meta name="x-build-id">`
  mismatch and either prompt or silently reload on next idle
  navigation. Contract explicitly defers this.
- No retry beyond N=1 reload per stale-bundle event. If the reload
  itself fails to recover, the user gets the manual refresh hint.
