# Live Alpaca Wave 1

Status: implementation complete; PR validation evidence captured below.
Owner: Codex, 2026-05-24.

## Source Material

- `team/intake/archive/2026-05-21-alpaca-live-eval-and-executor-refactor.md`
- `docs/superpowers/notes/handoffs/2026-05-21-alpaca-live-filter-multi-asset.md`
- recovered stashes: `live-engine-internals`, `live-storage`, `live-frontend`

## Delivered

- Added LiveConfig persistence for Live eval runs:
  - `eval_runs.live_config_json`
  - nullable `eval_runs.scenario_id` when `mode = Live`
  - store invariant checks that require LiveConfig for Live and reject it for Backtest
- Wired `RunMode::Live` through API launch:
  - validates `LiveConfig`
  - rejects non-paper Alpaca trading base URLs
  - builds Alpaca paper `BrokerSurface`, live bar stream, polling fallback, and live executor
- Completed the shared executor live path:
  - live bars are dynamic rather than scenario-injected
  - configured warmup bars seed history only and do not trade
  - current live bar close is the reference price
  - stop policies bound live runs by time, bars, or decisions
  - broker rejection class/message are retained on fill records
- Added CLI launch flags for bounded Live Alpaca runs:
  - `--live-asset`
  - `--live-capital`
  - `--live-broker-creds-ref`
  - `--live-bar-limit`
  - `--live-decision-limit`
  - `--live-time-limit-secs`
  - `--live-warmup-bars`
- Enabled the dashboard start dialog for Live Alpaca paper runs with bounded stop-policy inputs and broker preflight.
- Updated the Alpaca paper-eval skill to reflect the current LiveConfig/Executor path.

## Evidence

- `cargo check -p xvision-engine -p xvision-cli -p xvision-dashboard`
- `cargo test -p xvision-engine --test api_eval_run run_rejects_live_mode_without_live_config -- --nocapture`
- `cargo test -p xvision-engine live_config -- --nocapture`
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web exec vitest run src/routes/eval-runs.test.tsx`
- `cargo run -q -p xvision-cli -- eval run --help | rg -n "live-asset|live-capital|live-bar-limit|live-time-limit|live-warmup|scenario"`

## Safety Notes

- Live v1 is still Alpaca paper-only.
- `VenueLabel::Live` remains rejected.
- No validation step places real live orders.
