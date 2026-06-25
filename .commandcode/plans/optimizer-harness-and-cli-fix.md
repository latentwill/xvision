# Fix: Optimizer Harness Issues + CLI/API Namespace Confusion

## Problem 1: Optimizer Harness Issues

### 1A. Slot Resolution + Prompt Not Loaded — ✅ ALREADY FIXED

`resolve_slot_for_role()` at `cycle.rs:2090` resolves by canonical role first, falls
back to `"main"`, then `slots.first()`. `process_parent_mutations()` at `cycle.rs:776`
reads `slot.system_prompt`. Both CLI and dashboard paths converge through `run_cycle()`.

**No changes needed.**

### 1B. Honesty Gate Too Lax — Strengthen Semantic Checks

**File: `crates/xvision-engine/src/autooptimizer/canary.rs`**

The gate passed sabotaged strategies because `validate_prompt_semantics()` only
checks prose prompt text, not the full strategy risk params. The 3 sabotage
variants (`KillTrades`, `RemoveLossLimit`, `AbsurdCadence`) operate at the
strategy/param level and should be caught by the numeric gate — but when the parent
has garbage metrics (root issue 1A, now fixed), Δ=0 ± noise gets through.

Strengthening:
1. **Add `validate_strategy_semantics()`** — checks the full strategy (not just
   prose) for broken risk params: zero/negative `risk_pct_per_trade`, max positions,
   absurd `decision_cadence_minutes`, missing required tools.
2. **Call it from `process_parent_mutations()`** in `cycle.rs` — right after the
   `validate_prompt_semantics` check (~line 989), add a strategy-level check.
3. **Add more `validate_prompt_semantics` patterns**: "do not trade", "ignore all
   signals", "never act", "position size: 0" (already partially there but not
   comprehensive enough).
4. **Add `OverfitParam` sabotage variant** to `SabotageVariant` — sets
   `risk_pct_per_trade = 0.5` (over-concentrated) to catch the gate rejecting
   overfit as well as kill.

### 1C. Optimizer Mutates Non-Tunable Parameters — Add Protected Param Mask

**File: `crates/xvision-engine/src/autooptimizer/mutator.rs`**

`max_leverage`, `stop_loss_atr_multiple`, `daily_loss_kill_pct` are in
`RISK_PARAM_FIELDS` → exposed to the mutator. The DSPy flywheel has
`lock_protected_tokens()` that strips them AFTER compilation, but the direct
mutation path still proposes changes to them.

Fix:
1. **Split `RISK_PARAM_FIELDS`**: Remove protected engine params into a separate
   `PROTECTED_ENGINE_PARAMS` const.
2. **Filter `tunable_param_keys()`**: Return only non-protected keys.
3. **Update the mutator prompt** (`mutator-v1.md`): Remove `max_leverage` etc. from
   tunable examples — the prompt already has a "Protected parameters" section but it
   says they can be tuned via param experiments. They should say they are NEVER
   tunable.
4. **Keep the DSPy flywheel's `lock_protected_tokens()`** as defense-in-depth.

Protected params to remove from tunable set:
- `max_leverage` (position leverage multiplier)
- `stop_loss_atr_multiple` (ATR multiple for stop)
- `take_profit_atr_multiple` (ATR multiple for take-profit) — already not in
  `RISK_PARAM_FIELDS` but should be listed as protected
- `daily_loss_kill_pct` (daily loss cap)

Leave tunable in `risk.*`:
- `risk_pct_per_trade` (core decision logic knob)
- `max_concurrent_positions` (decision logic)
- `max_position_pct_nav` (decision logic)

**File: `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md`**

Update the "Protected parameters" section — remove the instruction that says
"these CAN be tuned via param experiments targeting risk.<field>". Replace with
"These values are NEVER tunable — they are engine-level safety limits. Do not
propose changes to them via any experiment kind."

### 1D. No Remote Inspect/Cancel — Add Subcommand + Open Allowlist

**Files:**
- `crates/xvision-cli/src/commands/optimize.rs` — add subcommands
- `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` — open access

`optimize show`, `ls`, `diff`, `export`, `lineage` are already allowed remotely
as read-only (fall through to `Allow` in `check_argv()`).

Need to add:
1. **`optimize cancel <cycle_id>`** CLI subcommand — calls
   `POST /api/autooptimizer/cycles/:cycle_id/cancel` via the dashboard API.
   Add to the allowlist as a permittable action.
2. **`optimize inspect <cycle_id>`** — alias for `optimize show` (read-only,
   already allowed).

For the `optimize cancel` add to allowlist:
- Add `DeniedNested` removal for `optimize cancel` (it should be allowed)
- Or add `optimize cancel` to a new specific template

### 1E. Corpus Diagnostics — Add `optimize explain-missing-data`

**File: `crates/xvision-cli/src/commands/optimize.rs`**

Add `optimize explain-missing-data` (or `optimize corpus`) subcommand that:
1. Queries the memory store for observations in the `autooptimizer:mutations`
   namespace
2. Reports count, oldest/newest observation timestamps
3. Warns when < 5 observations (too few for meaningful training)

---

## Problem 2: CLI/API Namespace Confusion

### 2A. Add `/api/optimize/run-cycle` as a Route Alias

**Files:**
- `crates/xvision-dashboard/src/routes/flywheel.rs` (or new route)
- `crates/xvision-dashboard/src/server.rs` (register route)

The gist identifies 3 namespaces for the same pipeline:
| Path | Name |
|------|------|
| `xvn optimize run` | "optimizer" (CLI) |
| `POST /api/autooptimizer/run` | "autooptimizer" (API, memory-distillation) |
| `POST /api/autooptimizer/run-cycle` | "autooptimizer run-cycle" (API, mutation cycle) |

The CLI (`xvn optimize run`) maps to the mutation cycle, not memory distillation.
`POST /api/optimize/run` already exists but delegates to `flywheel::optimize_run_simple`
(memory distillation — WRONG pipeline).

Fix:
1. **Add `POST /api/optimize/run-cycle`** that delegates to the same handler as
   `POST /api/autooptimizer/run-cycle` (`autooptimizer_cycle::start_cycle`).
   This creates a discoverable one-to-one mapping: CLI namespace `optimize` →
   API namespace `/api/optimize/run-cycle`.
2. **Keep `POST /api/autooptimizer/run-cycle`** as the canonical internal path.
3. Document at both endpoints that they are aliases.

---

## Files Changed

| # | File | Change |
|---|------|--------|
| 1 | `crates/xvision-engine/src/autooptimizer/canary.rs` | Strengthen `validate_prompt_semantics`, add `validate_strategy_semantics`, add `OverfitParam` sabotage variant |
| 2 | `crates/xvision-engine/src/autooptimizer/cycle.rs` | Call `validate_strategy_semantics()` in `process_parent_mutations()` after existing prompt check |
| 3 | `crates/xvision-engine/src/autooptimizer/mutator.rs` | Split `PROTECTED_ENGINE_PARAMS` from `RISK_PARAM_FIELDS`, filter from `tunable_param_keys()` |
| 4 | `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md` | Update "Protected parameters" section — never tunable, not even via param experiments |
| 5 | `crates/xvision-cli/src/commands/optimize.rs` | Add `Cancel` subcommand, add `Inspect` alias for `Show`, add `ExplainMissingData` subcommand |
| 6 | `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` | Allow `optimize cancel` remotely, allow `optimize inspect` |
| 7 | `crates/xvision-dashboard/src/routes/flywheel.rs` | Add `optimize_run_cycle` handler that delegates to `autooptimizer_cycle::start_cycle` |
| 8 | `crates/xvision-dashboard/src/server.rs` | Register `POST /api/optimize/run-cycle` route |

---

## Verification

1. **Slot resolution** — `cargo test -p xvision-engine resolve_slot_for_role`
2. **Honesty gate** — `cargo test -p xvision-engine autooptimizer_canary`
   (tests should verify new sabotage variants + semantic checks)
3. **Protected params** — `cargo test -p xvision-engine autooptimizer_validator`
   (tests should verify protected params are rejected)
4. **CLI subcommands** — `cargo build -p xvision-cli && xvn optimize --help`
   should show `cancel`, `inspect`, `explain-missing-data`
5. **Allowlist** — `cargo test -p xvision-dashboard cli_jobs_allowlist`
   (tests should verify `optimize cancel` allowed, `optimize inspect` reads allowed)
6. **API alias** — `curl -X POST /api/optimize/run-cycle -H 'Content-Type: application/json'`
   should start a cycle (same as `/api/autooptimizer/run-cycle`)
