# Claim: qa9-scenario-dedupe-api

Worktree: `.worktrees/qa9-scenario-dedupe-api`

Branch: `qa9-scenario-dedupe-api`

Owner: codex

## Scope

Prevent active scenarios with indistinguishable display names from surfacing in
`/api/scenarios`. The API now rejects create/clone requests that would reuse an
active scenario display name. Archived scenarios do not permanently reserve the
name, so operators can intentionally replace a scenario after archiving the old
one.

## Verification plan

- `git diff --check`
- `cargo test -p xvision-engine --test scenario_api create_rejects_active_duplicate_display_name`

Cargo verification is CI/non-deploy only on this host per repository
guardrails.
