---
track: alpaca-paper-crypto-submit
lane: integration
wave: post-q15
worktree: .worktrees/alpaca-paper-crypto-submit
branch: task/alpaca-paper-crypto-submit
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-execution/src/broker_surface.rs
  - crates/xvision-execution/tests/broker_surface.rs
  - crates/xvision-execution/tests/broker_surface_alpaca_live.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/trader_output.rs   # only if a new failure-class enum needs sibling tagging
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/api/**
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/agent/**
  - crates/xvision-engine/src/eval/scenario.rs
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/src/eval/run.rs
  - crates/xvision-cli/**
  - crates/xvision-dashboard/**
  - frontend/**
interfaces_used:
  - xvision_execution::broker_surface::BrokerSurface
  - xvision_execution::broker_surface::OrderRequest / OrderConfirmation / Side
  - xvision_execution::alpaca::{AlpacaApi, OrderRequest as ApacOrderRequest, OrderSide}
  - xvision_engine::eval::executor::{classify_run_failure, format_failure_reason}
parallel_safe: false
parallel_conflicts:
  - any future track that edits paper.rs concurrently
verification:
  - cargo build --workspace
  - cargo test -p xvision-execution
  - cargo test -p xvision-engine eval::executor
  - cargo test -p xvision-engine -- classify_run_failure
  - bash scripts/board-lint.sh
  - Optional operator smoke (not in CI):
      APCA_API_KEY_ID=… APCA_API_SECRET_KEY=… \
      cargo test -p xvision-execution --test broker_surface_alpaca_live -- --ignored alpaca_paper_crypto_submit_simple_market
acceptance:
  - Submitting a paper eval order for an Alpaca **crypto** asset never sends `Class::Bracket` to Alpaca. Bracket take-profit / stop-loss legs are silently dropped at the surface for crypto symbols. Equities (when added later) preserve bracket behaviour.
  - `short_open` on an Alpaca crypto asset is converted to a non-fatal no-op at the paper executor (or surface) — the decision is still recorded with `order_size = None`, no order is sent, the run continues to the next decision instead of failing as `[unclassified]`.
  - `format_failure_reason` preserves the full source chain of an `anyhow::Error` (e.g. `paper eval submit_order failed: … : alpaca create_order: <inner>`) so the stored `eval_runs.error` carries the Alpaca rejection wording, not just the outermost context.
  - `classify_run_failure` adds the failure classes `broker_rejected`, `broker_auth`, `broker_unsupported`, `broker_insufficient_funds`, `broker_timeout`, and routes the new patterns (`"not permitted"`, `"forbidden"`, `"not shortable"`, `"insufficient buying power"`, `"bracket"` + `"not supported"`, broker‐side timeout phrases) to the correct tag.
  - Unit tests cover, at minimum:
      1. Crypto bracket-omission (mock api captures `OrderRequest` with `take_profit_price = None`, `stop_loss_price = None`).
      2. `short_open` on `BTC/USD` returns a recorded-but-no-order decision and does not error the run.
      3. `format_failure_reason` shows both the `with_context` wrapper and the inner broker error in the saved string.
      4. `classify_run_failure` dispatches each of the new `broker_*` classes against representative error strings.
  - The pre-existing `submit_buy_with_bracket` test for the legacy `AlpacaExecutor` path keeps passing (the legacy `Executor` path is not on the crypto-paper route and should not regress).
  - Acceptance demo: rerun the failing eval (`xvn eval run --strategy 01KRQGPDHFN5C8CWB4ED757ER0 --scenario sc_01KRQGQX6Z40MFGAD2B5P64SAZ --mode paper`) end-to-end. Long-open decisions submit successfully, short-open decisions are recorded as no-ops, and the run completes with `status = "completed"` and a populated `metrics` summary. The stored `eval_runs.error` for any genuine failure carries a non-`[unclassified]` class tag.
---

# Scope

Production paper-eval runs against Alpaca paper are failing with
`[unclassified] paper eval submit_order failed: …` (both Buy/`long_open` and
Sell/`short_open` BTC/USD orders, observed in run ids
`01KRRA4CB1073KRRPPD06W6EEB` and `01KRRA1PJCTDR9NBEP8J2309DW`). Root cause is
twofold and product-shaped, not a transport bug:

1. **`AlpacaPaperSurface::submit_order` always sends bracket legs whenever
   `take_profit_pct` and `stop_loss_pct` are present** (see
   `crates/xvision-execution/src/broker_surface.rs:159-171` and the resulting
   `Class::Bracket` branch at `crates/xvision-execution/src/alpaca.rs:209-223`).
   Alpaca's crypto API does not support bracket / OCO / OTOCO — only simple
   market and limit orders. Every paper eval order on a crypto asset
   therefore round-trips through `Class::Bracket` and is rejected server-side.
2. **`short_open` on crypto is submitted as `Side::Sell` from a flat
   account** (see `PaperExecutor::run_inner` lines `437-441` in
   `crates/xvision-engine/src/eval/executor/paper.rs`). Alpaca crypto is
   long-only — selling from flat is rejected.

Compounding both, the error chain is collapsed at
`crates/xvision-engine/src/eval/executor/mod.rs:77-85`: `format_failure_reason`
calls `err.to_string()`, which on an `anyhow::Error` returns only the
outermost `with_context` message. The actual Alpaca rejection wording
(`"not permitted"`, `"bracket orders not supported for this asset class"`,
`"asset is not shortable"`, …) never reaches the stored `eval_runs.error`
field, so the classifier at `classify_run_failure` cannot route the error
and every failure lands in the `unclassified` bucket. There is no broker-side
class in the classifier today either; it only knows about trader-output and
provider-transport classes.

Fixing all three layers — surface, executor decision policy, and error
classification — is the smallest set of changes that turns these failures
from a run-killer into either a successful submission or a clean recorded
no-op with an accurate class tag for any residual failure mode.

# Likely fix surfaces

In rough order:

- **`AlpacaPaperSurface::submit_order`**
    - Detect crypto symbols (`asset.contains('/')` or, preferred, parse via
      `AssetSymbol::from_str` and branch on the Alpaca crypto whitelist) and
      drop bracket legs for crypto. Either keep `(None, None)` and submit a
      simple market order, or compute the legs but submit them on a
      follow-up surface call as separate stop/limit orders — v1 should
      take the simpler path (drop legs, log a debug trace).
    - When a crypto symbol receives `Side::Sell` and the current
      `position(asset)` is `0.0`, return a typed
      `BrokerSurface::SubmitError::ShortNotSupported(asset)` (or an
      `anyhow::Error` with a `broker_unsupported` tag string in the
      message) instead of round-tripping to Alpaca. Closing an existing
      long via `Side::Sell` must still work.
- **`PaperExecutor::run_inner`** (paper.rs)
    - When `is_actionable(&parsed.action)` is true and the action would
      submit a short-open on a crypto asset, record the decision row with
      `order_size = None`, emit a `DecisionEmitted` event with `size = 0.0`
      and the parsed action label, **do not** call `submit_order`, and
      continue the loop. The run stays Running, the operator sees the
      LLM's intent in the decisions table, and the broker isn't asked to do
      something it can't.
    - Optional: same treatment for any Alpaca rejection class the surface
      escalates as "non-fatal" — but err on the side of preserving the
      current "run fails on broker reject" behaviour for everything other
      than the documented crypto-short case, so we don't silently swallow
      real account problems (auth, insufficient funds, …).
- **`format_failure_reason`**
    - Replace `err.to_string()` with a chained representation: either
      `format!("{:#}", err)` (anyhow's alternate Display, which joins with
      ": ") or an explicit `err.chain().map(ToString::to_string).join(": ")`.
      Keep the `[<class>] ` prefix shape; only the right-hand body grows.
- **`classify_run_failure`**
    - Walk the same chained string when matching patterns.
    - Add the new `broker_*` classes listed in `acceptance` and pattern-match
      against the lowercase chain. Document them next to the existing class
      list comment in `executor/mod.rs:36-41`.

# Out of scope

- DB migrations or schema changes (no new tables, no `eval_runs.error_class`
  column — the `[<class>] ` prefix shape stays the wire contract).
- Strategy / agent / risk model changes. The trader can keep proposing
  `short_open` for crypto; the surface refuses to send it. A future track
  may pre-validate the strategy authoring UX (e.g. warn the user that
  `short_open` is unreachable for the chosen broker), but that's a separate
  UX wave.
- Dashboard UI work. The chain-aware error string flows into existing
  fields; styling, copy, and review-panel surfacing of the new classes
  belong on a follow-up track if needed.
- `Class::Bracket` support for equities. Equities aren't on Alpaca paper in
  this wave — keep the bracket branch live for the non-crypto path so the
  legacy `Executor` tests (`alpaca::tests::submit_buy_with_bracket`) keep
  passing.
- Orderly. Orderly already has its own bracket / TP-SL handling
  (`crates/xvision-execution/src/orderly.rs`); this track does not edit it.
- Changing the eval pipeline (no new agent slots, no new fields on
  `TraderDecision`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/alpaca-paper-crypto-submit -b task/alpaca-paper-crypto-submit origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
cd .worktrees/alpaca-paper-crypto-submit
git log --oneline -3 origin/main..HEAD   # must be empty before any edits
```

# Notes

- Failing runs that motivated this contract (kept for repro / regression):
    - `run_id = 01KRRA1PJCTDR9NBEP8J2309DW`, decision_index 0,
      action `short_open`, side `Sell`, asset `BTC/USD`,
      `size = 0.033243219673556645`, `reference_price_usd = 75165.26`.
    - `run_id = 01KRRA4CB1073KRRPPD06W6EEB`, decision_index 2,
      action `long_open`, side `Buy`, asset `BTC/USD`,
      `size = 0.03298935067768613`, `reference_price_usd = 75743.693`.
    - Both fired through strategy `01KRQGPDHFN5C8CWB4ED757ER0`
      ("Aggressive Scalper") on scenario
      `sc_01KRQGQX6Z40MFGAD2B5P64SAZ` ("30daytest", BTC/USD,
      2026-04-16 → 2026-05-16, 1m cadence).
- The surface-level "skip bracket for crypto" change is the *only* one that
  fixes the Buy case. The executor-level "short-open on crypto is a no-op"
  is the *only* one that fixes the Sell case. The classifier work is
  cosmetic relative to the runs that motivated the track, but is what makes
  any future broker rejection legible.
- Live integration test (`broker_surface_alpaca_live.rs`) currently skips
  `submit_order` to avoid generating real paper orders. The contract adds
  one operator-run `--ignored` test that submits a tiny BTC/USD market
  order without bracket legs and asserts a filled receipt. Not run in CI.
- Class names follow the existing `provider_*` convention so the wire
  contract for downstream consumers (review panel filters, `xvn eval json`
  output) stays uniform.
- A polished follow-up — surfacing the new `broker_*` classes in the
  dashboard review panel and the `xvn eval show` CLI output — is *not* in
  this track. File it as a follow-up contract once this lands.
