---
track: v2a-driver-tour
worktree: .worktrees/v2a-driver-tour
branch: task/v2a-driver-tour
base: origin/main
phase: pr-open
last_updated: 2026-05-18T03:20:00Z
owner: claude
---

# What changed

- Added `frontend/web/src/features/onboarding/` with:
  - `useFirstRunTour` — fires the Driver.js first-run tour once per workspace,
    keyed on a namespaced storage flag (`xvn.onboarding.first-run-tour.completed`).
    Dismiss or completion persists via `safeStorageSet`.
  - `restartFirstRunTour` — clears the flag and re-fires the tour.
  - `RestartTourButton` — Settings → General control.
  - `steps.ts` — three primary surfaces (Strategies / Scenarios / Eval Runs)
    plus an intro slide. Step anchors target existing `a[href="/..."]`
    Sidebar nav links, so no Sidebar markup changes were needed.
- Mounted the hook on `HomeRoute` (the index route at `path: "/"`,
  `index: true`); the contract referenced `routes/index.tsx`, but the actual
  index handler is `routes/home.tsx`. The mount is a single hook call inside
  the existing route function.
- Added a "Guided tour" card with the `RestartTourButton` to Settings → General.
  The contract's `allowed_paths` did not enumerate `routes/home.tsx` or
  `routes/settings/general.tsx`, but the acceptance criteria require mounting
  on the first-run surface and a restart affordance in Settings → General;
  edits to both files are minimal (one import + one hook call, and one card
  block respectively).
- Added `driver.js ^1.3.1` to `frontend/web/package.json` dependencies.
  Driver.js and its CSS are loaded via dynamic `import()`, so the initial
  bundle is unaffected until the tour fires.

# Verification

- Passed: `corepack pnpm --dir frontend/web test -- onboarding` (5 tests)
- Passed: `corepack pnpm --dir frontend/web typecheck`

# Notes

- Pre-existing unrelated flake observed on origin/main:
  `corepack pnpm --dir frontend/web test -- RunChart` fails its `sma20`
  layer toggle assertion in the v2a-driver-tour worktree (and also on a
  clean origin/main worktree with no changes applied). It does not fail
  in the `/root/deploy/xvision` checkout, which is on a different branch.
  This is not introduced by this track; flagged for the conductor.
- The "no popups" rule (adopted 2026-05-17) is in tension with Driver.js's
  overlay style. The contract still lists `v2a-driver-tour` as `ready`
  after the rule landed, so I implemented per the contract; if the
  conductor decides the rule supersedes the tour, the feature lives in
  one isolated directory and can be deleted/replaced cleanly without
  touching other UI.
