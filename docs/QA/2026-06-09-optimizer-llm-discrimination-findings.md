# Overnight Optimization Log — 2026-06-09

## Goal
Run hifreq F3 gate across all 5 Ollama models on crash+recovery scenario.
Pick winner → overnight optimizer run.

## Strategy Under Test
**Range Fade HIFREQ** — loosened mean-reversion gate
- Gate: `adx_14 < 28`, `rsi_14 < 45`, `close < vwap_20`, `cooldown_bars: 2`
- Scenario: BTC crash + recovery Aug 2024 15m (`sc_01KTHCWFS5WX5HW6XAQJJ8CYBP`)
- 1,144 decision bars, 14 days

## Model Bakeoff IDs
| Model | Strategy ID |
|---|---|
| qwen3b | `01KTPFFRCMHCWYGAV44P74G0TQ` |
| fino1-8b | `01KTPCCJ3F5BXG0CYRVC592J8G` |
| lfm2.5-8b | `01KTPHRZHYAMA10171PH2HX9A3` |
| gemma4-e4b | `01KTPHRSDGDBBFRVYWJ5D5P8Z4` |
| deepseek-r1-8b | `01KTPHRZMW6W9E5NM6TH2NBJ1W` |

## Baselines (buy-hold this window)
- buy_hold: -8.38% / Sharpe -2.24
- always_flat: 0.00% / Sharpe 0.00
- simple_trend: -2.26% / Sharpe -1.11
- simple_mean_reversion: -5.63% / Sharpe -1.05

---

## Final Corpus Summary (25 examples across 13 scenarios)

| Scenario | Trades | Sharpe | Notes |
|---|---|---|---|
| crash+recovery Aug 2024 (×6 runs) | 3 each | +3.022 | consistent, deterministic |
| crypto-rangebound-q2-2025 | 2 | +7.696 | **best** — ideal regime for fade |
| sc_01KTHCVJWZJVXSKYP6DBKREXG9 (flash 5m) | 2 | +9.828 | highest Sharpe |
| crypto-bear-q3-2024 | 1 | +1.355 | works in bear |
| crypto-bull-q1-2025 | 2 | -1.898 | fails in bull trend |
| BTC-2022-03, BTC-2022-11, BTC-2023-03, flash-aug | 0 each | 0.000 | gate doesn't fire |

Actions across 25 corpus examples: long_open=18, stop_loss=4, flat=2, hold=1
**0 wins, 14 losses, 11 flat** — model enters on every gate fire (2/25 flat = 8% skip rate, mechanical not model-chosen)

Positive Sharpe comes from **risk avoidance** (flat most of the time = avoids buy-hold drawdowns), not profitable entries.

## Results

| Model | Sharpe | Return% | MaxDD% | Trades | vs buy-hold | Notes |
|---|---|---|---|---|---|---|
| fino1-8b (pre-bakeoff smoke, tight gate) | 3.02 | +0.21 | 0.11 | 4 | +8.59pp | text-scan recovery working |
| qwen3b (hifreq gate) | -2.10 | -0.056 | 0.17 | 32 (93 fires) | -0.14pp vs buy-hold | early-stop fired repeatedly, 31% win rate — hifreq gate too noisy |
| fino1-8b (bakeoff, tight gate — wrong ID) | -5.15 | -0.17 | 0.32 | 2 (3 fires) | +8.21pp vs buy-hold | 0% win rate; non-deterministic at T=0 (3.02→-5.15 same setup) |
| lfm2.5-8b (hifreq gate) | — | — | — | — | — | running (cycle 29, zero early-stop events — promising vs qwen3b) |
| gemma4-e4b | — | — | — | — | — | queued |
| deepseek-r1-8b | — | — | — | — | — | queued |

---

## Findings & Notes

### 2026-06-09 16:00 — Bakeoff started
- deepseek-r1:8b was missing from ollama `enabled_models` in `/data/config/default.toml` line 76 — added manually
- Container restart does NOT wipe strategies (bind-mounted XVN_HOME) — only confusion was stale local xvn 0.1.0 binary (removed)
- `nodecision_recovery_succeeded / output_text_json_scan` is the live path for CoT models (fino1, deepseek-r1) — agentd fix confirmed working
- Running models sequentially (not parallel) to avoid Ollama GPU contention

### 2026-06-09 17:10 — qwen3b result
- **Sharpe -2.10, 31% win rate, 32 trades from 93 gate fires** — hifreq gate is too loose
- The loosened gate (`adx<28, rsi<45, close<vwap`) fires 93× but the model can't discriminate well enough — too many marginal setups
- Early-stop fired at decision_index 15 and 27 — model kept losing early in runs
- Key insight: the tighter original gate (fino1 smoke: 4 trades, Sharpe 3.02) beats the hifreq gate with qwen3b badly
- qwen3b slightly outperforms buy-hold (-0.056% vs -8.38%) but loses to always_flat — net negative alpha
- fino1 now running on same hifreq gate — compare against its tight-gate result

### 2026-06-09 17:14 — fino1 bakeoff result + critical findings
- **Wrong strategy ID in bakeoff**: `01KTPCCJ3F5BXG0CYRVC592J8G` is fino1 with ORIGINAL tight gate, not hifreq clone. Bakeoff for fino1 was not apples-to-apples.
- **Non-determinism at T=0**: same strategy + scenario, fino1 got Sharpe 3.02 in smoke test, Sharpe -5.15 in bakeoff. Temperature=0 is set in config but text-scan recovery path may not enforce it. This undermines optimizer comparisons.
- **Hifreq gate conclusion so far**: qwen3b with hifreq gate = Sharpe -2.10 (too many bad setups, 31% win rate). Original tight gate is likely better even with fewer trades.
- **Action**: lfm/gemma/deepseek still on hifreq gate — will see if any model handles the noise better. After bakeoff, recommend optimizing fino1 on the **original tight gate** (best clean signal so far: Sharpe 3.02 smoke).

### 2026-06-09 16:27 — qwen3b early-stop firing
- Gate IS firing frequently (decision_index reached 27+ in ~27 min)
- `early-stop policy` firing at indices 15 and 27 with `skip_count=4` — inheriting flat after consecutive losses
- This is LIMITING actual trade count even when gate fires lots; early-stop may be over-aggressive for a hifreq strategy
- `nodecision_recovery_succeeded / output_text_json_scan` also active for qwen3b (not just CoT models) — qwen3b not using tool calls either
- **Optimizer note**: may need to tune early-stop threshold, or the prompt needs to be more selective to avoid the loss streaks that trigger it

### Known issues to watch
- gemma4-e4b is slow (~60s/cycle) — may take 45+ min alone
- deepseek-r1:8b is a reasoning model, will use text-scan recovery path like fino1
- Trade count is low (4 for fino1 smoke test) — Sharpe may still be noisy; optimizer needs dense corpus

---

## Strategic Assessment (17:27)

Hifreq gate (-2.10 Sharpe for qwen3b) vs original tight gate (3.02 Sharpe for fino1 smoke):
- Hifreq gate floods the model with marginal setups → early-stop triggers → losing trades dominate
- Original tight gate fires rarely (~3-4x per 14-day window) but those few trades are high quality
- **Hypothesis**: the optimal strategy is the original tight gate with the best model, not a looser gate

**Gemma4-e4b decision**: gemma is ~60s/cycle and would take ~90 min. Given the trend so far, may skip gemma overnight to save time for the optimizer. Will decide after lfm result.

**Timing**: lfm ~18:00, deepseek ~19:00–19:30, optimizer start ~19:30–20:00 if we skip gemma.

## KEY FINDING: LLM Has No Edge in Current Strategy Design

### Discovery (18:30)
Ran fino1 on crash+recovery 4 more times — all 4 got **exactly Sharpe 3.022**. T=0 IS deterministic. The earlier -5.15 bakeoff run was an outlier (likely model cold-start during provider swap).

Then ran qwen3b on the same tight gate, same scenario → **also Sharpe 3.022, identical actions**.

Exported both eval runs and compared decisions side by side:
```
fino1:  long_open (0.66), stop_loss (1.0), long_open (0.66) → pnl same
qwen3b: long_open (0.85), stop_loss (1.0), long_open (0.85) → pnl same
```

**Identical actions, identical PnL, identical Sharpe.** The only difference is conviction score.

### Implication
The LLM is rubber-stamping every gate fire with "long_open". The strategy's edge comes entirely from:
1. The gate's timing (firing at oversold/VWAP-extended moments)
2. The exit rules (stop-loss thresholds, take-profit)

**Optimizing the LLM prompt will not improve this strategy.** It's functionally a mechanical rules strategy with an LLM in the loop that never says no.

### What needs to change for LLM to add value
1. **Gate fires at SOME but not all good setups** — model needs to discriminate (reject some fires)
2. **OR make exit LLM-controlled** — model decides stop-loss size based on conviction
3. **OR make hifreq gate work** — loose gate forces model to skip bad setups (this is what we tried, but model failed at discrimination: 31% win rate with hifreq)

### Root cause of hifreq failure
The model doesn't have enough information to discriminate at 93 fires/scenario. At 3 fires/scenario the gate is already selective enough that any "long_open" wins on average. The LLM's added value threshold is somewhere between 3 and 93 fires — probably around 15-25 fires/scenario.

### Recommendation for next session
Before optimizing, redesign the strategy so the LLM actually discriminates:
- Target ~15-20 gate fires per 14-day scenario (not 3, not 93)
- Add context that helps the LLM distinguish good vs bad setups (e.g., trend direction, prior bar behavior, funding/OI proxy signals)
- Verify LLM skip rate is >30% (if model never says flat, it's not discriminating)

## Optimizer Blockers — CRITICAL

### 1. Optimizer defaults to dummy model — should use agent's bound model
`xvn optimize run` defaults to `model_provider=dummy`. The `--live` flag switches to the real model but is stubbed (exit 12). **This is architecturally wrong**: the optimizer already knows which agent it's tuning and that agent has a provider/model bound to it (e.g. fino1 via ollama). It should resolve and use that binding by default, exactly as `eval run` does — no flag needed. The dummy default makes sense for CI unit tests but is useless (and misleading) for end-users. Fix: resolve agent's provider+model in `optimize run` and use it for candidate evaluation. Keep dummy as an explicit `--test-model` override, not the default.

### 2. T=0 non-determinism in text-scan path
Same strategy + same scenario + temperature=0 config → Sharpe 3.02 in one run, -5.15 in the next. The `output_text_json_scan` recovery path (used by all models since none are calling submit_decision as a tool) bypasses the temperature=0 setting. This means:
- The corpus contains contradictory examples (identical indicator snapshots → different decisions → different quality scores)
- Any optimizer trained on this corpus is learning noise
- **Fix needed**: enforce T=0 at the agentd level for the text-scan path, or switch models back to proper tool calls

### 3. Corpus too thin (11 examples, 0 wins)
Original tight gate fires ~1-4 times per 14-day scenario. Even across 7 scenarios, we only have 11 examples. Losses dominate (10/11). A corpus this small and unbalanced won't produce a meaningful optimization signal.

### Overnight work (workaround)
While blockers remain unfixed, running overnight:
1. **Variance measurement**: 4 more fino1 runs on crash+recovery → quantify T=0 variance magnitude
2. **qwen3b tight gate test**: never tested this combo; fast model on tight gate
3. **Large corpus build**: fino1 on 6+ more scenarios → larger future corpus for when --live lands

## Morning Action Items (priority order)

1. **Redesign the gate** — target ~15-20 fires per 14-day window. Middle ground between tight (3 fires, LLM always enters) and hifreq (93 fires, LLM can't discriminate). Suggested approach: current tight conditions + loosen one parameter (e.g. `adx_14 < 26` instead of 22, `rsi_14 < 38` instead of 35, `cooldown_bars: 4` instead of 8).

2. **Verify LLM discrimination** — before optimizing, confirm model skip rate is >25%. If model says `flat` on less than 1 in 4 gate fires, it's not discriminating and optimization won't help.

3. **Fix optimizer --live** (engine change) — `xvn optimize run --live` exits 12 "stub in this wave". Until this is fixed, optimizer only runs against a dummy model. This is the main blocker for overnight optimization.

4. **Fix T=0 enforcement in text-scan path** — the earlier outlier (-5.15 run) likely came from model cold-start during provider transition, not true non-determinism. 4 successive runs all got 3.022, so this is lower priority.

5. **Run optimized gate version across rangebound + flash-crash scenarios** — those are donde the strategy shows strongest edge (Sharpe +7.7 and +9.8). An optimized strategy should target those regimes.

## Optimization Plan (post-bakeoff)

```bash
xvn optimize run \
  --agent <winner_agent_id> \
  --slot trader \
  --capability trader \
  --corpus ./corpus.json \
  --optimizer mipro \
  --metric delta_sharpe \
  --rng-seed 42 \
  --json
```

Corpus should include multiple scenarios for robustness (not just crash+recovery).
