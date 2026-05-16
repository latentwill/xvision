# Claim: qa9-strategy-validation-drift

Worktree: `.worktrees/qa9-strategy-validation-drift`

Branch: `qa9-strategy-validation-drift`

Owner: codex

## Scope

Tighten strategy draft validation so prompt/manifest drift is reported instead
of returning `ok: true`. The QA case was a draft whose prompts described
BTC/USD 6h behavior while the manifest still exposed ETH/USD and 15-minute
cadence.

## Verification plan

- `git diff --check`
- `cargo test -p xvision-engine authoring::tests::validate_draft_reports_prompt_manifest_asset_and_cadence_drift`
- `cargo test -p xvision-engine api::strategy::tests::validate_draft_reports_manifest_slot_prompt_drift`

Cargo verification is CI/non-deploy only on this host per repository
guardrails.
