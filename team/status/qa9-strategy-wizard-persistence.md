# qa9-strategy-wizard-persistence

Status: implemented, frontend-verified, Rust tests queued for CI/non-deploy

Claimed: 2026-05-14T08:12:45Z
Worktree: `.worktrees/qa9-strategy-wizard-persistence`
Branch: `qa9-strategy-wizard-persistence`

Implemented:

- Added `update_manifest` to strategy authoring and API wrappers so wizard
  edits to `asset_universe` and `decision_cadence_minutes` persist to the
  manifest shown by the Strategy Inspector.
- Updated wizard tool definitions and prompt guardrails so asset/cadence edits
  must use the persistence tool before the assistant can claim success.
- Updated `set_risk_config` to synchronize preset/explicit risk changes back
  into `manifest.risk_preset_or_config`.
- Added chat/setup transcript labels for the new `update_manifest` tool.

Verification:

- `git diff --check`
- `corepack pnpm --dir frontend/web typecheck`
- `corepack pnpm --dir frontend/web test -- setup ChatRail`

Not run locally:

- Rust tests covering authoring/API/wizard behavior, because `CLAUDE.md`
  forbids running Cargo on this deploy host.
- `rustfmt` because the binary is not installed on this host.
