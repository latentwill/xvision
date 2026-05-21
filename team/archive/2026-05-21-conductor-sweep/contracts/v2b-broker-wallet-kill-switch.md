---
track: v2b-broker-wallet-kill-switch
lane: leaf
wave: v2b
worktree: .worktrees/v2b-broker-wallet-kill-switch
branch: task/v2b-broker-wallet-kill-switch
base: origin/main
status: merged
depends_on:
  - v2b-dashboard-auth-boundary
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/safety/**             # NEW — pause/resume endpoints + safety_audit reader
  - crates/xvision-engine/src/safety/**                 # NEW — pause-state singleton + persistence + per-run limits
  - crates/xvision-execution/**                         # pause-gate before submit; per-run limit check
  - crates/xvision-engine/src/eval/run.rs               # venue_label threading + per-run limit hookup
  - crates/xvision-engine/src/eval/scenario.rs          # venue_label field
  - crates/xvision-engine/src/wallet/**                 # pause-gate (if wallet writes live in engine)
  - crates/xvision-identity/**                          # ONLY if non-custodial wallet pause-gate lives here
  - crates/xvision-engine/migrations/**                 # safety_audit table + venue_label column
  - crates/xvision-core/migrations/**                   # cycles.venue_label if needed (foundation already indexed cycles)
  - crates/xvision-engine/tests/safety_*.rs             # NEW
  - frontend/web/src/api/types.gen/**                   # ts-rs regen
  - frontend/web/src/features/safety/**                 # NEW — pause indicator + venue label badge
forbidden_paths:
  - crates/xvision-dashboard/src/auth/**                # v2b-dashboard-auth-boundary owns
  - crates/xvision-dashboard/src/cli_jobs/**            # v2b-remote-cli-job-safety owns
  - crates/xvision-engine/src/eval/executor/**          # don't touch the simulator; the safety gate sits at submit-to-broker boundary, not inside simulate_fill
interfaces_used:
  - xvision_dashboard::auth::AuthContext                # landed by v2b-dashboard-auth-boundary
  - xvision_engine::eval::scenario::Scenario
  - xvision_execution::broker::BrokerSubmit
parallel_safe: true
parallel_conflicts:
  - v2b-remote-cli-job-safety (no shared files; both depend on auth-boundary)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo clippy -p xvision-execution -- -D warnings
  - cargo test -p xvision-engine safety_
  - cargo test -p xvision-execution
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test --run safety
acceptance:
  - **Global pause/kill switch.** `POST /api/safety/pause` and `POST /api/safety/resume` (both auth-required, leveraging the auth-boundary track). State persisted in a new `safety_state` row (single-row table). `GET /api/safety/state` returns `{paused: bool, paused_at, paused_by, reason}`.
  - **Pause default for non-paper/testnet.** Fresh-install pause default: `paused = false` for paper-mode runs, `paused = true` for any Live broker config or any non-testnet wallet config until explicitly resumed. The transition from paper → live config triggers an automatic pause requiring resume.
  - **Pause gate at submit.** Every broker submit path checks the singleton; if `paused`, returns `SafetyPaused { reason }` error and refuses the submit. Same for every wallet write (`crates/xvision-engine/src/wallet/**`) and contract write (`crates/xvision-identity/**`, if applicable).
  - **Per-run limits.** `Scenario` (and/or `Strategy`) gains `safety_limits: Option<SafetyLimits>` with fields:
    * `notional_cap_usd: Option<f64>`
    * `max_order_count: Option<u32>`
    * `max_leverage: Option<f64>` (Crypto perps only)
    * `max_loss_pct: Option<f64>` (drawdown circuit-breaker)
    Checked at submit; breach aborts the run with `RunAbort::SafetyLimit { kind, value, limit }`. Run row records the abort reason.
  - **Venue label.** `Scenario.venue_label: VenueLabel` enum (`Paper`, `Testnet`, `Live`). Live broker submits require `VenueLabel::Live`; submitting a `Paper` scenario to a live-configured broker returns `VenueLabelMismatch` error. UI shows the label as an inline badge (no popup, per `CLAUDE.md`): green for Paper, amber for Testnet, red for Live.
  - **Confused-deputy tests.** Test asserts that a Paper-labelled scenario cannot accidentally hit a live-configured broker.
  - **Audit log.** New `safety_audit` table records one row per: pause toggle, broker submit, wallet write, marketplace action, contract write. Fields: `timestamp`, `user` (from AuthContext), `action_kind`, `params` (JSON), `result` (Allowed | DeniedSafetyPaused | DeniedLimit | DeniedVenueMismatch | Errored), `pause_state_at_time`. New migration adds the table.
  - **Dashboard surfaces.** Pause indicator in the chrome (top bar) — single inline component, no popup. Venue badge on every run row, capsule, and detail surface. Safety-audit log view at `/safety` (route, no popup).
  - **Tests:**
    * Pause endpoint toggles state; broker submit returns `SafetyPaused` while paused.
    * Per-run notional cap breach aborts run with correct enum variant.
    * Per-run max_order_count breach aborts run.
    * Venue-mismatch detection on `Paper` → live-broker.
    * Audit row written for every gated action.
    * Frontend test asserts pause indicator + venue badge render inline (no popup).

---

# Scope

V2B operational hardening for broker, wallet, and contract write paths.
Implements action plan §V2B work packages 4 (broker and wallet
guardrails) and 5 (audit and observability — for the broker/wallet/
marketplace/contract subset; the auth-side audit log lives in the
auth-boundary contract).

# Out of scope

- Marketplace transaction signing flow — V2C. This track only lands the **gate** that V2C signing must honour.
- Mainnet deployment — V4.
- Auth at the dashboard API surface (`v2b-dashboard-auth-boundary` owns).
- CLI job orphan recovery (`v2b-remote-cli-job-safety` owns).
- Changes to the eval simulator (`backtest.rs`). The safety gate sits at the live-submit boundary, not inside the simulator.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/v2b-broker-wallet-kill-switch status
git -C .worktrees/v2b-broker-wallet-kill-switch log --oneline -3 origin/main..HEAD

# Confirm:
#   - rebased on top of v2b-dashboard-auth-boundary's merged commit
#   - AuthContext is available from xvision_dashboard::auth
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/v2b-broker-wallet-kill-switch -b task/v2b-broker-wallet-kill-switch origin/main
```

# Migration coordination

This track adds one or two migrations under
`crates/xvision-engine/migrations/`:

- **028** `028_safety_state_and_audit.sql` — adds `safety_state` (single-row), `safety_audit` (event log).
- Possibly **029** `029_eval_runs_venue_label.sql` if `venue_label` needs a dedicated column on `eval_runs` (vs. being read out of `Scenario.venue_label` at query time).

The agent should check `team/MANIFEST.md` before claiming numbers. As of
2026-05-21 the highest engine migration is 027 (run_bars_manifest from
the V2E candle-integrity track).

# Notes

- The pause-state singleton can be a `tokio::sync::RwLock<SafetyState>` populated at startup from the DB and written on toggle. Avoid hitting the DB on every submit.
- For the "non-paper auto-pause on first install" rule, the bootstrap path should detect: any broker config with a non-paper venue → pause. This avoids accidentally going live on first run.
- `RunAbort` is likely an existing type in `xvision-engine`; extend it rather than adding a parallel error type.
- The `safety_audit` table will grow indefinitely; ship with a TTL/janitor pass either in this track or as a follow-up (the existing observability janitor at `crates/xvision-engine/src/eval/*janitor*.rs` is a model).
