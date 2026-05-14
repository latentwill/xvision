---
track: qa9-scenario-dedupe-api
worktree: /root/deploy/xvision/.worktrees/qa9-scenario-dedupe-api
branch: qa9-scenario-dedupe-api
phase: local-verified
last_updated: 2026-05-14T08:25:30Z
owner: codex
---

# Status

Picked up the QA9 scenario dedupe board item from
`team/execution-board-2026-05-13.md`.

## Implemented

- Added a scenario API regression test for active duplicate display names.
- Added create-time validation that rejects an active scenario whose trimmed
  display name matches an existing active scenario case-insensitively.
- Archived scenarios can be recreated with the same display name.

## Verification

- `git diff --check` passed.
- Not run locally: `cargo test -p xvision-engine --test scenario_api create_rejects_active_duplicate_display_name`

Cargo tests are not run on this deploy host per repository guardrails.
`rustfmt` is not installed on this host.
