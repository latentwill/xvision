# strategy-require-at-least-one-agent-fixture-migration — status

**Track:** `team/contracts/strategy-require-at-least-one-agent-fixture-migration.md`
**Author:** conductor recon (Claude, 2026-05-21)
**Status:** partially landed — `strategy_store.rs` migrated as a worked
example; 16 other fixtures **deferred** because they hit one of two
runtime fallbacks the contract aimed to delete.

## What landed

`crates/xvision-engine/tests/strategy_store.rs` (the canonical save/
load roundtrip test) is now on the target shape:

```rust
agents: vec![AgentRef { agent_id: "01TESTAGENT…", role: "trader" }],
trader_slot: None,
```

No Agent record is needed because the test exercises only the
filesystem-backed `StrategyStore` save/load contract; no
`validate_eval_trader_source` or executor path runs.

## Why the other 16 fixtures didn't migrate

The contract asks for a single mechanical swap — `agents: Vec::new()`
+ `trader_slot: Some(LLMSlot{…})` → `agents: vec![AgentRef{…}]` +
`trader_slot: None`. In practice each of the 16 remaining tests
exercises one of two runtime paths that **require** a real trader
source at runtime, not just at the validation gate:

1. **PaperExecutor / BacktestExecutor direct runs** —
   `tests/eval_executor_paper.rs`, `tests/eval_broker_circuit_breaker.rs`,
   `tests/eval_paper_pnl_realized.rs`, `tests/pipeline_inline.rs`,
   `tests/risk_min_notional.rs`, etc. instantiate the executor directly
   and pass `&[]` for `agent_slots`. The executor then calls
   `trader_model_id(agent_slots, strategy)` which falls back to
   `strategy.trader_slot` (see `crates/xvision-engine/src/eval/executor/paper.rs::trader_model_id`,
   `:280-300`). With `trader_slot: None`, the trader pipeline emits a
   `missing_response` error on every cycle and the run fails.

2. **`validate_eval_trader_source` resolution path** —
   `tests/api_eval_min_notional.rs`, `tests/api_eval_run.rs`,
   `tests/eval_run_scenario.rs`, `tests/broker_rules_integration.rs`,
   etc. call `eval::run_with_deps` / `eval::run`. The
   `resolve_agent_slots(ctx, strategy)` helper (`api/eval.rs:1094`)
   tries to load each `AgentRef.agent_id` from the agent store. If
   the store doesn't contain that id (because the test never seeded
   it), the call returns `NotFound` and the launch fails.

3. **`strategy_roundtrip.rs`** asserts validation passes — it
   exercises `validate_strategy` which today requires either
   `agents.len() >= 1` (with at least one trader-role ref **and** a
   resolvable agent) OR a populated `trader_slot`. With my change
   only the AgentRef is present; the test fixture's
   `validate_strategy` call fails because the agent store is empty.

## What's actually needed to finish this contract

Path A — minimal but invasive (matches the contract verbatim):

1. Add `crates/xvision-engine/tests/helpers/agent_seed.rs` with a
   `save_test_trader_agent(ctx, agent_id) → Agent` helper that
   creates a default trader-role agent in the test's `AgentStore`.
2. For every LAUNCH test:
   - Switch `ctx_with_tables()` → `ctx_with_agents_table()` (the
     latter applies migrations 005, 019, 020, 025).
   - Call `save_test_trader_agent(&ctx, "01TESTAGENT…").await` before
     constructing the `Strategy`.
3. For every direct-executor test (paper/backtest):
   - Build a `ResolvedAgentSlot` inline and pass it as `agent_slots`
     instead of `&[]`. The slot's `agent_slot_to_llm_slot` mapping
     needs to be exposed (or `ResolvedAgentSlot::new_trader_for_test`
     added as a test-only helper on the engine side).
4. Delete the loophole branch in `api/eval.rs::validate_eval_trader_source`
   (lines 996-999):
   ```rust
   if agent_slots.is_empty() {
       if strategy.trader_slot.is_some() { return Ok(()); }   // ← DELETE
       return Err(ApiError::Validation("eval requires a trader output …"));
   }
   ```
   Update the trailing error message at line 1020 to drop the
   "remove attached agents to use the legacy trader slot" wording.
5. Delete the matching test
   `eval_trader_source_accepts_legacy_trader_slot_without_agents`
   in `api/eval.rs::tests` (`:2737`).
6. Delete the legacy resolution fallback at
   `api/eval.rs::agent_slot_for_trader` (`:1999-2002` —
   `if let Some(slot) = strategy.trader_slot.as_ref() { ... }`).

Estimated scope: 17 test-file edits + 2 engine-source edits +
1 new helper file + 1 deleted test = ~600 LOC churn. Worth at least
one dedicated session.

Path B — incremental, slower but lower-risk:

1. Add the helper + start migrating one test family per PR
   (executor-direct first, then launch tests, then roundtrip).
2. Keep both fallbacks until every fixture is off them.
3. Delete fallbacks last, in a single PR with zero test-fixture
   churn — every test should still pass.

This is the safer rollout. It's also slower — five or six small PRs
instead of one big one.

## Recommendation

Park this contract at `status: blocked` (blocked on operator decision
between Path A and Path B). The `strategy_store.rs` worked example is
on `origin/main` so future workers can see the target shape; the
status note above captures the failure modes for the remaining 16
fixtures so the next session doesn't re-do this recon.

If the operator picks **Path A**, file as a single follow-on contract
with the full 600-LOC scope baked in.

If **Path B**, decompose into 5-6 per-family contracts (e.g.
`fixture-migration-paper-direct`, `fixture-migration-launch-tests`,
`fixture-migration-validator-asserts`, plus the final
`drop-trader-slot-fallback`).
