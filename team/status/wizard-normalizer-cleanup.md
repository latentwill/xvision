---
track: wizard-normalizer-cleanup
worktree: .worktrees/wizard-normalizer-cleanup
branch: task/wizard-normalizer-cleanup
phase: pr-open
last_updated: 2026-05-19T18:25:00Z
owner: Claude Opus 4.7 (operator: Ed)
pr: https://github.com/latentwill/xvision/pull/326
---

# What I'm doing right now

PR open at #326. Work complete; waiting on review.

## Summary

- Tightened the `create_scenario` tool-call JSON Schema with explicit
  `enum` constraints on `asset_class` / `quote_currency` / `source` /
  `granularity`, `oneOf` on `calendar` (closes the `{"type":
  "Custom"}` shape), typed `time_window` (`format: date-time`), typed
  `capital`, fixed `replay_mode` from string to object.
- Deleted the calendar tag-wrap unwrap (Qwen-specific shape repair).
- Trimmed the `source` alias list from 13 entries to the 4 canonical
  lowercase aliases.
- `coerce_to_one_of` kept as the model-agnostic last-line defense.

## Verification

- `cargo build -p xvision-dashboard` — clean.
- `cargo test -p xvision-dashboard --lib wizard_loop` — 40 / 40.
- 4 pre-existing `tests/http.rs::create_scenario_*` failures
  reproduce on stashed `origin/main` — unrelated.

## Blocked on

Review.

## Next up

- Address review feedback.
- After merge: watch the next Qwen 3.6 wizard session for
  `create_scenario` normalizer warns — if the schema advertisement
  works, there should be zero warns from the deleted-shim paths.
