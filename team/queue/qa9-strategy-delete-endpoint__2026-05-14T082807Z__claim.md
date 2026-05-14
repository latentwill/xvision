# Claim: qa9-strategy-delete-endpoint

Worktree: `.worktrees/qa9-strategy-delete-endpoint`

Branch: `qa9-strategy-delete-endpoint`

Owner: codex

## Scope

Add the intended strategy-level deletion contract at `DELETE /api/strategy/:id`.
Deleting attached agent roles is already exposed separately; this track removes
the strategy draft entity itself from the strategy store and search index.

## Verification plan

- `git diff --check`
- `cargo test -p xvision-engine --test api_strategy delete_`
- `cargo test -p xvision-dashboard --test inspector_routes delete_strategy`

Cargo verification is CI/non-deploy only on this host per repository
guardrails.
