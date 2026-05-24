---
name: xvision-alpaca-paper-eval
description: Use when debugging, validating, or modifying xvision Alpaca paper-trading evals, especially failures involving historical bars, reference prices, paper credentials, broker-surface order submission, or BTC/USD eval runs. Keeps paper trading safe, scenario-aware, and diagnosable.
---

# xvision Alpaca Paper Eval

Use this skill for xvision paper-mode eval work that touches Alpaca, broker
credentials, historical-bar/reference-price resolution, order submission, or eval
failure diagnostics.

This skill is informed by the public OpenClaw Alpaca trading skill's broad
operational posture: start in paper mode, verify account/market data before
orders, keep credentials out of source, and treat trading automation as
high-risk. Do not copy OpenClaw commands or prose into xvision; xvision uses
its own CLI, dashboard, `BrokerSurface`, and eval pipeline.

## Safety Rules

- Paper mode first. Do not switch to live Alpaca endpoints unless the user
  explicitly asks and the code path has an explicit live-mode guard.
- Never print API secrets. It is acceptable to show whether keys are present,
  which base URL is configured, and a redacted key suffix.
- Never run commands that place real live orders as validation. Paper orders are
  still stateful; use tiny sizes or mocks unless the user asks for a live paper
  smoke.
- Preserve operator state in `/data` and `$XVN_HOME`. Use a temp `XVN_HOME` for
  local dev smoke tests unless debugging the deployed node's real state.

## Required Mental Model

Backtest and bounded Live Alpaca evals now share the `Executor` loop. The run
shape determines which sources and sinks are attached:

1. Backtests require a scenario id. Bars come from the scenario loader or an
   injected test fixture, fills use `SimulatedFills`, and `live_config` must be
   absent.
2. Live Alpaca v1 requires `RunMode::Live` plus `LiveConfig`. It persists
   `eval_runs.scenario_id = NULL` and `eval_runs.live_config_json`; internally
   the API builds a synthetic scenario envelope from the live config.
3. Live v1 is paper-only. `VenueLabel::Live` and non-paper Alpaca trading base
   URLs must be rejected. The accepted default is
   `https://paper-api.alpaca.markets`.
4. Live bars are supplied by `AlpacaLiveClient` with a polling fallback through
   `LiveStream`. Configured warmup bars seed `market_data.bar_history` only;
   they must not create decisions or fills.
5. Live fills go through `RealBrokerFills` and
   `xvision-execution::broker_surface::AlpacaPaperSurface`. Broker rejections
   should persist the broker error class/message on the fill record and produce
   diagnosable run failure context.
6. Stop policy is mandatory for live runs and uses OR semantics across
   `time_limit_secs`, `bar_limit`, and `decision_limit`.

Reference-price invariant:

- The agent must receive the current historical eval bar before deciding.
- Backtest order sizing and Alpaca notional conversion must use the same
  current eval bar close.
- Do not use live/latest quotes for a historical backtest decision.
- Live orders must use the current live bar close as the reference price.
- Do not default BTC/USD to a hard-coded paper price except inside isolated
  mocks.
- Missing reference price must not be a vague terminal error. Include run id,
  decision index, asset, action, side, size, chosen bar/reference source, and
  the underlying Alpaca call that failed.

## Debug Workflow

1. Locate the failing layer.
   - Broker-surface errors usually mention `alpaca get_position`,
     `alpaca create_order`, `alpaca get_order`, or reference price.
   - Eval orchestration errors live in `crates/xvision-engine/src/api/eval.rs`.
   - Shared backtest/live execution errors live in
     `crates/xvision-engine/src/eval/executor/backtest.rs`.
   - Live config validation lives in
     `crates/xvision-engine/src/eval/live_config.rs`.
   - Alpaca order conversion lives in
     `crates/xvision-execution/src/broker_surface.rs`.

2. Check config without leaking secrets.
   - Stored credentials: Settings -> Brokers or
     `$XVN_HOME/secrets/brokers.toml`.
   - Env credentials: `APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`,
     `APCA_API_BASE_URL`.
   - Paper default base URL should be `https://paper-api.alpaca.markets`.

3. For reference-price failures, inspect whether:
   - Paper mode loaded bars for the scenario before creating the executor.
   - The agent seed includes `market_data.current_bar`.
   - `OrderRequest.reference_price_usd` came from the current eval bar close.
   - The error chain includes the actionable context listed above.

4. Add tests with mocks before touching live credentials.
   - Use `crates/xvision-execution/tests/broker_surface.rs` for
     `AlpacaPaperSurface` behavior.
   - Use `RunMode::Live` requests without credentials only for validation
     failures, not broker submission.
   - Mock flat accounts with `positions: vec![]`.
   - Assert the notional and bracket prices come from the intended eval-bar
     reference price.

5. Validate narrowly first.
   - `cargo test -p xvision-execution --test broker_surface`
   - `cargo test -p xvision-engine live_config`
   - `cargo test -p xvision-engine --test api_eval_run run_rejects_live_mode_without_live_config -- --nocapture`
   - `cargo build --release` before building a deploy image.

## Logging Requirements

When changing paper eval or broker-surface code, make failures explain:

- `run_id`
- `decision_index`
- `asset`
- `action`
- `side`
- `size`
- `reference_price_usd`
- reference source: `eval_bar.close`
- underlying broker method and error

Use `anyhow::Context` for call-chain context and `tracing` for structured fields.
For background eval tasks, persist the full error chain with `{e:#}` so the UI
and `eval get` expose the cause, not just the top-level message.

## Deployment Reminder

After fixing a remote paper-eval issue:

```bash
cargo test -p xvision-execution --test broker_surface
cargo build --release
scripts/deploy-image.sh --push root@100.120.48.1
```

Then verify:

```bash
ssh root@100.120.48.1 'cd /root/deploy/stacks/xvn && docker compose ps && docker logs --tail 80 xvn-app'
curl -I https://xvn.tail2bb69.ts.net
```
