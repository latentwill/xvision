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

Paper eval order flow:

1. `xvision-engine::eval::executor::paper::PaperExecutor` runs each decision.
2. It asks the broker for position and balance.
3. It loads the scenario's historical bars and includes the current bar in the
   agent seed inputs.
4. It sizes actionable decisions using the current eval bar close as the
   reference price.
5. It calls `BrokerSurface::submit_order` with that explicit
   `reference_price_usd`.
6. `xvision-execution::broker_surface::AlpacaPaperSurface` converts base size
   into Alpaca notional and submits a market order.

Reference-price invariant:

- The agent must receive the current historical eval bar before deciding.
- Paper eval order sizing and Alpaca notional conversion must use the same
  current eval bar close.
- Do not use live/latest quotes for a historical eval decision.
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
   - Paper execution errors live in
     `crates/xvision-engine/src/eval/executor/paper.rs`.
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
   - Mock flat accounts with `positions: vec![]`.
   - Assert the notional and bracket prices come from the intended eval-bar
     reference price.

5. Validate narrowly first.
   - `cargo test -p xvision-execution --test broker_surface`
   - `cargo test -p xvision-engine eval_executor_paper`
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
