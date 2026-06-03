# xvision backtest harness / position-communication findings
_Found while building an intraday eval strategy on `xvn 0.21.0` in the `xvn-app` container, 2026-06-02. For a code agent. Line refs are against the `fix/cli-agent-experience` worktree (`/tmp/xvision-cli-agent-experience`); confirm against current `main`._

## 1. Deterministic filter fires only on the false→true EDGE of a level condition (and the skip is logged opaquely) — ROOT CAUSE of "barely any trades"
- Filter `{"all":[{"lhs":"adx_14","op":">","rhs":18}]}` on the 2022-11 FTX-crash week (169 1h bars): ADX (from the events' `indicator_snapshot`) had **median 29.4, max 55.1, 117/142 bars > 18** — yet the filter **triggered only 3×** (`wakeups:3`, `suppressed_cooldown:6`).
- On **160/169** bars the event row was `triggered:false, suppressed_reason:null, conditions_passed:[], conditions_failed:[]` — i.e. the predicate wasn't recorded as evaluated at all. There is **no "steady-state / already-satisfied" suppression reason or summary counter**, so an operator cannot tell why a true condition didn't fire.
- Effect: persistent level conditions (`adx>x`, `close>sma_50`, …) wake the model **once** at the crossing, then go silent. This is almost certainly the cause of the prior goal's "PnL works but barely makes trades." Crossing conditions (`crosses_above` etc.) do fire repeatedly.
- **Ask:** either gate every bar the condition holds, or (if edge-trigger is intended) emit an explicit `suppressed_reason:"steady_state"` + a `filter_summary.suppressed_steady_state` counter and **document it**. The empty `conditions_passed/failed` on skipped bars is misleading.

## 2. Eval-path trader cannot see computed indicators → trades blind or refuses
- The 0.21 **engine eval** trader prompt is built from a briefing whose `indicators` map is populated by the **`indicator_panel` tool**, but `xvn strategy new --prompt` (atomic mode) creates the agent with **no tools** (`crates/xvision-cli/src/commands/strategy.rs:940` → `required_tools: Vec::new()`; agent `allowed_tools:None`). Templates attach `["ohlcv","indicator_panel"]` (`strategies/templates.rs`).
- Symptom: a trader prompted to use SMA50/+DI/−DI emitted **`flat` on all 14 wakes**, each justification literally saying *"The SMA50 is not directly provided in the inputs"* / *"+DI/−DI not explicitly provided"* — and then **estimated** them from raw closes.
- Compounding factor: `agent/briefing.rs` **delta-encodes** indicators ("indicators that didn't move are omitted"), so even when present, an unchanged SMA50 is dropped from later-bar prompts.
- The actively-trading reference strategy (`01KT401590…`, 654 decisions) has the **same** `inputs_policy:raw` + no indicator tool and traded anyway — and **lost** (return −0.39%, Sharpe −3.5): consistent with trading on guessed/absent indicators.
- **Ask:** atomic `strategy new` should attach `indicator_panel` (or the eval should always inject the strategy/filter-computed indicator panel — the filter already computes ADX/DI/SMA/EMA/RSI correctly, see #1 — into the trader briefing). Also expose a CLI to add tools to an existing agent (there's `add-filter`/`remove-filter` but no `agent set-tools`).

## 3. Eval trader decision has no stop-loss / take-profit → no auto-closing; positions only close when the model is re-woken
- The engine eval `TraderDecision` exported fields are `{action, conviction, justification, reasoning, order_size, fill_price, fill_size, fee, pnl_realized}` — **no `stop_loss_pct` / `take_profit_pct`** (the legacy `xvision-trader` path *does* have them: `crates/xvision-trader/src/prompt.rs` SCHEMA + `parse.rs` `LlmTraderDecision`).
- Consequence: the backtest executor has **no bracket/protective exits**; a position stays open until a later wake emits `flat`/flip. With #1 throttling wakes, a short opened 2022-11-09 wasn't closed until 2022-11-13 (**4-day hold** — not "intraday"). This is the core reason intraday round-trips are hard to produce.
- **Ask:** add `stop_loss_pct`/`take_profit_pct` to the eval trader schema and have the backtest executor honor them as auto-fills (the ab-compare/`xvision-eval` executor already does TP/SL auto-fills).

## 4. Misleading trade metrics (`win_rate`, `n_trades`)
- A clearly winning realized round-trip (short $18,317 → cover $16,586 = **+$922 realized**) reported `win_rate: 0.00` and `n_trades: 2`. `n_trades` appears to count **legs** (open+close) not round-trips, and `win_rate` did not reflect the realized win.
- **Ask:** define `n_trades` = closed round-trips; compute `win_rate` over realized round-trip PnL. Otherwise operators can't trust the headline trade stats.

## 5. `xvn strategy set-filter` silently no-ops when a filter already exists
- Calling `set-filter` on a strategy that already has a filter returned success (`filter set`) but left the **old** filter active (verified by reading `/data/strategies/<id>.json`); the subsequent eval used the stale filter (byte-identical result). Had to embed the new filter via `strategy new --from-file` to actually replace it.
- **Ask:** `set-filter` should replace (or error telling the user to `remove-filter` first). Today it's a silent no-op.

## 6. Minor
- `scenario inspect --card` shows `decision_bars: 0` for scenarios that *do* have cached bars and run fine (cosmetic, but misleading).
- `crosses_above`/`crosses_below` require **both operands to be indicators** — `rsi_14 crosses_above 50` fails with `E_FILTER_OPERAND_TYPE`. Reasonable, but not obvious from the catalog; worth a catalog note (use a numeric `>`/`<` for level, crossings only indicator-vs-indicator).
- Provider-key UX: `eval run` needs `XVN_PROVIDER_OPENROUTER_KEY` in the live process env even though `doctor`/`provider check` report the key "present" (they read the secrets file). Mismatch is confusing.
