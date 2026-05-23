# XVN filter strategy eval efficiency findings

> QA/process review from 2026-05-23 after an agent attempted to create and compare a filtered XVN strategy against a baseline.
>
> Scope: speed and reliability of the operator workflow for creating strategy/scenario evals, especially filter-functionality tests.
>
> This document records observed workflow friction and recommended product/runbook improvements. It is not a trading-strategy endorsement.

## Summary

The agent eventually created a filtered BTC 4h breakout strategy, attached a Gemini Flash Lite 3.1 trader, ran it against a BTC 4h mixed-regime scenario, created a no-filter baseline, and compared the runs. Both runs completed but produced identical aggregate behavior, so the test did not demonstrate a filter effect.

The work took longer than necessary because the agent treated a known XVN operation as an open-ended codebase investigation. The main functional issue was a fixed strategy cadence mismatch, but several process and UX improvements would make this workflow much faster.

## Confirmed friction

### 1) 4h scenario vs fixed 60-minute strategy cadence

**Severity:** High  
**Area:** Strategy creation / eval preflight  
**Type:** Functional / UX

**Observed behavior:**
- The selected scenario was 4h (`240 min`).
- The created strategy had `decision_cadence_minutes = 60`.
- Validation reported the strategy as not eval-ready because scenario granularity and strategy cadence did not match.
- Attempting to patch `decision_cadence_minutes` through the strategy metadata endpoint failed because that endpoint only accepts metadata fields.
- The backtest still ran despite the preflight mismatch.

**Expected behavior:**
- If users can request or select a 4h strategy/scenario, the creation surface should create a 4h-compatible strategy or clearly block the mismatch.
- Preflight should either hard-block incompatible cadence or explicitly mark the run as cadence-mismatched if execution is still allowed.

**Why it matters:**
- It is confusing for a user to ask for a 4h strategy, get a 1h strategy artifact, and still receive a completed run.
- It weakens trust in eval results and slows operator debugging.

**Recommendations:**
- Expose `decision_cadence_minutes` at strategy creation time.
- Add a first-class `timeframe`/`cadence` field in the strategy authoring UI/API.
- Add a one-click “use scenario cadence” option.
- Make eval launch behavior consistent when preflight says `eval_ready: false`.

---

### 2) Strategy/scenario creation workflow is too discovery-heavy

**Severity:** Medium  
**Area:** Operator workflow / docs / API affordances  
**Type:** Productivity

**Observed behavior:**
- The agent spent significant time reading README, manual, scenario docs, strategy docs, Rust API source, CLI source, dashboard routes, validation code, and templates before running the first useful eval.
- Much of this was only needed because the supported live-node workflow and API constraints were not obvious from a single runbook.

**Expected behavior:**
- A known operation like “create filtered strategy, attach agent, run eval, compare baseline” should have a short operator recipe.

**Recommendations:**
- Maintain a single “fast eval recipe” for strategy/filter tests:
  1. List scenarios.
  2. Filter by asset/timeframe/cadence compatibility.
  3. Pick a regime-diverse scenario.
  4. Create strategy metadata.
  5. Attach trader agent with provider/model/prompt.
  6. Run eval.
  7. Clone/create baseline.
  8. Run baseline on the same scenario.
  9. Compare aggregate metrics and decision divergence.
- Document which operations are API-first versus remote-CLI-first.
- Document known API limitations near the recipe, especially metadata-only strategy patching.

---

### 3) Remote CLI affordances are misleading for creation tasks

**Severity:** Medium  
**Area:** Remote CLI / operator UX  
**Type:** Discoverability

**Observed behavior:**
- Attempts to use remote CLI subcommands such as `strategy create`, `strategy new`, and `scenario create` failed because those subcommands are not allowed on the remote CLI surface.
- The agent then had to switch to direct API calls.

**Expected behavior:**
- Operators should be directed to the supported surface immediately.
- If a command is unavailable remotely, error messages should point to the supported API/UI path.

**Recommendations:**
- Add remote CLI errors like: “This subcommand is not allowed remotely. Use POST /api/strategies plus POST /api/agents and attach via POST /api/strategy/:id/agents.”
- Add a lightweight `xvn remote recipe strategy-eval` or docs page showing the allowed path.
- Consider exposing safe remote creation commands if they are intended operator workflows.

---

### 4) Filter functionality test used a scenario that was too clean/sparse

**Severity:** Medium  
**Area:** Scenario selection / eval design  
**Type:** Test quality

**Observed behavior:**
- Filtered and unfiltered runs produced identical aggregate metrics and action counts.
- Both effectively opened one long and then mostly held/flat.
- This does not prove the filter is useless; it indicates the scenario did not exercise the gate.

**Expected behavior:**
- A filter-functionality test should include enough marginal conditions for the filtered and baseline arms to diverge.

**Recommendations:**
- Tag scenarios with regime features such as `trend`, `chop`, `false-breakout`, `volatility-expansion`, `volatility-compression`, and `high-trade-density`.
- Provide a default “filter stress test” scenario that includes obvious chop, false breakouts, at least one clean breakout, and enough post-warmup decision points.
- Prefer cadence-compatible 1h scenarios until 4h strategy cadence creation is fixed.

---

### 5) Baseline/variant comparison should be first-class

**Severity:** Medium  
**Area:** Eval UX / strategy workflow  
**Type:** Productivity / analysis

**Observed behavior:**
- The agent manually created a filtered strategy and separate baseline, then compared aggregate outputs.
- There was no obvious first-class “clone as baseline/variant” workflow.

**Expected behavior:**
- Testing a filter should make it easy to create A/B arms where only the filter instruction differs.

**Recommendations:**
- Add a “Clone as baseline” or “Create variant” action.
- Preserve shared fields: model, provider, asset universe, risk preset, scenario, and base trading prompt.
- Make the filter instruction the only intentional delta.
- Store an explicit comparison group/run set for the two evals.

---

### 6) Comparison output needs filter-specific diagnostics

**Severity:** Medium  
**Area:** Eval results / diagnostics  
**Type:** Observability

**Observed behavior:**
- Aggregate metrics were enough to detect identical behavior, but not enough to quickly explain filter effectiveness.

**Expected behavior:**
- Filter tests should expose whether the filter changed decisions and why.

**Recommended diagnostics:**
- Action divergence count between baseline and filtered arms.
- First divergence timestamp.
- Count of trades blocked by the filter.
- Hypothetical PnL of blocked baseline trades, if available.
- Regime classification histogram.
- Examples of blocked trades with rationale.
- Baseline trades taken during chop.
- Allowed-trade win rate versus baseline win rate.

---

## Faster workflow for the next run

Use this when the goal is to test filter functionality, not to research the best possible trading edge:

1. Start API-first on the live node; do not begin with source spelunking.
2. Pick a cadence-compatible scenario before creating any strategy.
3. Prefer a 1h mixed-regime scenario while strategy creation defaults to 60-minute cadence.
4. Create two nearly identical prompts:
   - Baseline: breakout/trend-following trader.
   - Filtered: same trader plus a regime gate that blocks chop/ambiguous breakouts.
5. Use the same provider/model for both arms.
6. Attach trader agents before eval launch.
7. Run both evals on the same scenario.
8. Compare aggregate metrics plus decision divergence, not return alone.
9. Only read repo source if the API response is unexplained or blocked.

## Product fixes that would reduce future operator time

- Strategy creation accepts cadence/timeframe and validates it against selected scenario.
- Eval launch blocks or clearly annotates cadence mismatches.
- Remote CLI errors point to the supported API/UI path.
- Scenario browser exposes regime-diversity and trade-density tags.
- One-click baseline/variant creation for A/B evals.
- Eval comparison page shows filter-effectiveness diagnostics.
- A documented “filter stress test” scenario exists and is kept cadence-compatible with the default strategy authoring path.

## Result from the reviewed run

- Filter strategy was created and attached to a Gemini Flash Lite 3.1 trader.
- Baseline strategy was created with the same model.
- Both backtests completed on the same BTC 4h scenario.
- Both runs produced identical headline metrics and action counts.
- The result is best interpreted as an insufficiently discriminating test setup, not evidence that filtering has no value.
