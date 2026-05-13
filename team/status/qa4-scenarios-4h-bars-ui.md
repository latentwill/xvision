# qa4-scenarios-4h-bars-ui

Status: claimed and implemented on 2026-05-13.

Scope:
- Added `4h` / `Hour4` support for scenarios, Alpaca bars cache keys, scenario preview payloads, and relevant CLI parse paths.
- Added dashboard bars-fetch controls that submit `xvn bars fetch` through the existing remote CLI job API and refresh scenario chart/cache queries after terminal job status.
- Added focused frontend coverage for 4H scenario creation and bars-fetch job wiring.
- Added Rust scenario API coverage for `Hour4`; not run locally because deploy-host guardrails prohibit cargo/rust tooling.

Verification:
- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test -- ScenarioForm scenarios-detail`
- `corepack pnpm --dir frontend/web test`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

Blocked / follow-up:
- Board verification asks for `cargo test -p xvision-engine scenario -- --nocapture`, but `CLAUDE.md` forbids cargo/rust tooling on this deploy host. Run this in CI or a non-deploy development environment.
