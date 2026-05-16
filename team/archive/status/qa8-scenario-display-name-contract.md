---
track: qa8-scenario-display-name-contract
worktree: /root/deploy/xvision/.worktrees/predeploy-eval-scenario-guardrails
branch: codex/predeploy-eval-scenario-guardrails
phase: implemented
last_updated: 2026-05-14T07:22:01Z
owner: codex
---

# What changed

- `CreateScenarioRequest.display_name` now defaults to an empty string during serde decode, so missing names in JSON/TOML reach the API validator instead of failing at extraction/parse time.
- Scenario creation trims the accepted display name before persisting.
- Added regressions for:
  - engine validation of omitted `display_name`,
  - persisted display-name trimming,
  - CLI `scenario validate --from-file` actionable missing-name errors,
  - dashboard `POST /api/scenarios` actionable missing-name 400 responses.

# Verification

- `git diff --check`

# Blocked on

- Rust tests were not run on this deploy host because `CLAUDE.md` forbids `cargo`, `cargo build`, `cargo check`, and `cargo test`.
