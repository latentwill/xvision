# v2a-driver-tour-infrastructure

Date: 2026-05-14
Branch: v2a-driver-tour-infrastructure
Claim: team/queue/v2a-driver-tour-infrastructure__2026-05-14T140312Z__claim.md

## Summary

- Added `driver.js` and a dashboard tour controller mounted from the app shell.
- Added first-visit tour completion persistence with safe `localStorage`
  helpers.
- Added a command-palette `Restart tour` action that replays the current page
  tour.
- Added the first concrete route tour for the dashboard home page with stable
  `data-tour` anchors.

## Verification

- PASS: `corepack pnpm --dir frontend/web test -- tour-state CommandPalette`
  (2 files, 4 tests)
- PASS: `corepack pnpm --dir frontend/web typecheck`
- PASS: `corepack pnpm --dir frontend/web test` (23 files, 84 tests)
- PASS: `git diff --check`
- NOT RUN: Rust tests / `cargo test` because this session is on the deploy host
  and `CLAUDE.md` forbids Cargo on deploy hosts.
