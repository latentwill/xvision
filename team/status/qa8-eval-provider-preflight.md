# qa8-eval-provider-preflight

Status: local checkpoint complete on 2026-05-14.

Summary:
- Added eval launch preflight coverage for missing provider configuration and stale strategy model/provider metadata.
- Added setup actions from eval launch preflight errors to `/settings/providers` and `/settings/brokers`.
- Tightened provider readiness so eval launch requires at least one enabled provider model, not only a default model.
- Tightened strategy readiness so required strategy models must be enabled on the required provider before launch.

Verification:
- `corepack pnpm --dir frontend/web test -- eval-runs`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

Notes:
- Rust `cargo` checks were not run on this deploy host per `CLAUDE.md`.
