---
track: scenario-form-date-range-picker
worktree: .worktrees/scenario-form-date-range-picker
branch: task/scenario-form-date-range-picker
phase: pr-open
last_updated: 2026-05-19T17:40:00Z
owner: Claude Opus 4.7 (operator: Ed)
pr: https://github.com/latentwill/xvision/pull/323
---

# What I'm doing right now

PR open at #323. Work complete; waiting on review.

## Summary

- Component-design package at `docs/design/calendar-picker/` ported to
  `frontend/web/src/components/calendar-picker/` (`calendar-core.tsx` +
  `calendar-desktop.tsx` + `calendar-mobile.tsx` + barrel).
- `ScenarioForm.tsx` From/To `<input type="date">` pair replaced with
  `<InlineRangeBar>` (sm+) / `<MobileInlineCard>` (below sm).
- `ScenarioForm.tsx` `const CALENDAR` constant replaced with state +
  `<select>`; `Custom` reveals an inline string input that produces
  the typed `{ Custom: string }` payload.
- 10 picker tests + 11 reworked ScenarioForm tests + 3 new calendar
  select tests, all green.

## Verification

- `pnpm typecheck` — clean.
- `pnpm vitest run calendar-picker` — 10 / 10.
- `pnpm vitest run scenario` — 50 / 50.
- `pnpm build` — clean.
- Pre-existing `RunChart.test.tsx` failure (`sma20` lookup) reproduces
  on stashed working tree — not introduced by this PR.

## Contract amendment

Mid-implementation `frontend/web/src/components/scenario/*.test.tsx`
moved out of `forbidden_paths` — see contract Notes for rationale.

## Blocked on

Review.

## Next up

- Address review feedback.
- Follow-up (separate contract, NOT this PR): `wizard-normalizer-cleanup`
  to retire the Qwen-specific repair shims in
  `crates/xvision-dashboard/src/wizard_loop.rs::normalize_create_scenario_input`
  once the wizard tool-call schema is tightened.
