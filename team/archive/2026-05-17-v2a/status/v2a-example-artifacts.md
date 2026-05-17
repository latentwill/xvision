---
track: v2a-example-artifacts
worktree: (removed post-merge)
branch: task/v2a-example-artifacts (deleted post-merge)
base: origin/main
phase: merged
last_updated: 2026-05-17T00:16:00Z
owner: claude-opus
pr: https://github.com/latentwill/xvision/pull/205
merge_commit: 2e844f75fe734d41bc85d7778c19d1c8befdd2a6
---

# Status: merged

V2A item 3 (`v2a-example-artifacts`) merged to `main` on 2026-05-17.

## Outcome

- `xvision_engine::strategies::templates` exposes three example
  strategies (trend follower, mean reversion, breakout) plus two
  short BTC/USD scenarios (one-week bull, one-week flash crash) and
  the `is_example_strategy` / `is_example_scenario` identity helpers.
- `xvn example seed [--reset] [--json]` CLI subcommand.
  - Default: idempotent, missing rows created, existing example rows
    skipped, operator-owned rows never overwritten.
  - `--reset`: seed-owned strategies deleted and re-created;
    example scenarios deleted and re-inserted when no `eval_runs`
    row references them; FK-protected scenarios preserved as-is
    and listed under `scenarios_preserved_referenced`.
- 5 engine tests + 5 CLI tests passing.

## Review fix-forward

PR #205 carried two review fixes on top of the initial implementation:

1. Implemented the real scenario reset path (via
   `scenario_store::delete_scenario`) so updates to curated scenario
   bodies actually reach users via `--reset`.
2. Replaced an incorrect `xvn ab-compare --scenario … --arms …`
   snippet in the seeded tutorial README with the correct
   `xvn eval run` ⇒ `xvn eval compare` flow.

## Follow-up work (out of scope)

Pre-existing test failures noted on `origin/main` at the time of the
PR but not introduced by this work:

- `xvision-engine`:
  `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
  and three `eval::postprocess::tests::extract_and_record_*` tests.
- `xvision-cli`:
  `commands::eod::tests::populated_state_surfaces_runs_and_audit` —
  its `empty_pool()` helper does not apply migration 014's
  `eval_runs.agent_id` rename.

These should be triaged on their owning tracks.
