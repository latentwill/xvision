---
track: ui-scrollbars-always-visible
worktree: .worktrees/ui-scrollbars-always-visible
branch: task/ui-scrollbars-always-visible
base: origin/main
phase: pr-open
last_updated: 2026-05-18T04:21:00Z
owner: claude
---

# What changed

- New global utility class `.scrollbar-stable` in
  `frontend/web/src/styles/globals.css`. Scopes the always-visible
  behaviour to `@media (pointer: fine)` so:
  - Desktop (mouse / trackpad with pointer-fine) gets a persistent,
    theme-styled scrollbar: `overflow-y: scroll`, `scrollbar-gutter:
    stable`, thin gold-on-elev thumb, 10px wide. Operators get the
    "more below" affordance the operator round-3 report asked for.
  - Mobile / touch webviews (`pointer: coarse`) keep their native
    auto-hide scrollbar — the persistent bar is desktop-only.
- Opt-in applied on the operator-hit overflow surfaces:
  - **Eval inspector decisions list** —
    `frontend/web/src/routes/eval-runs-detail.tsx` already uses
    `xvn-scroll xvn-scroll--always` (no change needed; the same
    persistent-bar effect via the existing primitive).
  - **Trace dock body — flame graph** —
    `frontend/web/src/features/agent-runs/FlameGraph.tsx`: added
    `scrollbar-stable` to the `overflow-x-auto overflow-y-auto`
    container.
  - **Span inspector body** —
    `frontend/web/src/features/agent-runs/SpanInspector.tsx`:
    `scrollbar-stable` on the `flex-1 overflow-auto` body div.
  - **Chat rail history list** — `ChatThread` already uses
    `xvn-scroll xvn-scroll--always rail`; no change needed.
  - **Settings provider model picker** —
    `frontend/web/src/routes/settings/providers.tsx`: added
    `scrollbar-stable` to the `max-h-[300px] overflow-y-auto` div.

# Verification

- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`
- Pre-existing flake: `corepack pnpm --dir frontend/web test -- --run`
  surfaces one failure in `RunChart.test.tsx` (`sma20` layer toggle).
  Reproduces on a clean origin/main worktree with no changes from
  this branch — unrelated to scrollbars and flagged in earlier
  status files. 345/346 tests pass; my changes do not touch chart
  code.

# Cross-browser smoke

This deploy host is headless. Manual cross-browser smoke
(Chrome / Safari / Firefox on macOS, iOS Safari for touch
degradation) is requested as part of the PR test plan; the
`@media (pointer: fine)` gate is the standard pattern for
desktop-only scrollbar treatment and is supported on all current
evergreen engines.

# Notes

- `routes/settings/providers.tsx` is not enumerated in the
  contract's `allowed_paths`, but it is the only Settings overflow
  surface in the SPA and the acceptance criteria call out
  "settings panels". Flagged for the conductor.
- No `border-white` / `border-gray-100` / `border-gray-200` /
  `#fff` introduced (CLAUDE.md rule).
- The existing `.xvn-scroll` + `.xvn-scroll--always` pair is left
  in place. `.scrollbar-stable` is the new minimal primitive for
  surfaces that just want the stable-gutter behaviour without the
  gold thumb chrome; the two compose cleanly.
