# AutoOptimizer Run-4 Findings — the loop now optimizes (2 issues remain)

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 13:09Z, includes PR #805 `359140c4` "make the optimizer actually optimize — F11,F14,F18,F20,F21,F22" + PR #806 test repairs)
**Headline:** The mutate → backtest → gate → keep/drop loop now works end-to-end on a **real** strategy. Two issues remain open: **F11** (cost/budget) and **F22** (agentless strategies).

---

## The unlock — verified ✅

A real cycle on `gemini_long_gate_v3` (empty `mechanical_params`, openrouter agent) finally produced and evaluated a genuine candidate:

```
cycle 01KT9CB0BAHQ2Q7HQFEJKFGDQG
parent e3f9f8f378  →  candidate b5505dd671   (distinct! a real risk.* mutation)
mutation_proposed → mutation_gated(passed:false) → honesty_check(passed:true)
inspect: b5505dd671  dropped  parent e3f9f8f378  Day Shrp -0.985  Hold Shrp 1.632  openrouter/google/gemini-3.1-flash-lite
```

| # | Was | Now |
|---|-----|-----|
| F14/F21 | mutator emitted invalid `mechanical_params` edits / identity / no candidate | **emits a distinct, valid `risk.*` param candidate** (`b5505dd671 ≠ e3f9f8f378`), gated on real Day/Hold Sharpe |
| F20 | core loop never produced/kept a candidate on any real strategy | **loop runs end-to-end**: real mutation → backtest both windows → gate → drop (Day Sharpe -0.985, correctly rejected). Keep path proven by new integration test `run_cycle_keeps_improving_risk_param_candidate` |
| F13 | candidate cycles invisible | `inspect`/`optimizer ls`/`GET /api/autooptimizer/cycles` all show this candidate cycle with Day/Hold Sharpe, status, parent, mutator |
| F18 | demo crashed on stale `cycle_sealed` | **demo completes**, skipping the unknown variant with a note |

This is the optimizer actually optimizing for the first time. The candidate was *dropped* (Day Sharpe worsened) — a correct gate decision, not a bug.

---

## Still open

### F11 — [HIGH] cost still prints `$0.00`; budget still can't trip
This cycle printed `cycle cost: $0.00 (metered paper-test + experiment-writer/judge inference)` — **with no unknown-price note** — while `model_calls` recorded the real spend: `google/gemini-3.1-flash-lite` **22 calls, 510,946 input tokens, cost_usd = $0.1306, null_cost = 0** (i.e. those calls *are* priced in the ledger). So the meter is reporting `$0.00` even though realized cost (~$0.13) is correctly recorded in `model_calls`, and the promised "N call(s) with UNKNOWN price" fallback note did not appear either.

The metering path is not reading realized cost. The robust fix remains: **sum `model_calls.cost_usd` over the cycle's run_ids** (it already holds the provider-reported truth) rather than a parallel catalog/quote path. Until then `--budget` cannot trip (a $0.00 reading never reaches any ceiling).

### F22 — [HIGH] agentless/mechanical strategies still crash on a default `anthropic.claude-sonnet-4.6`
`xvn optimizer run-cycle --strategy example-trend-follower --provider openrouter --model …` still dies mid-cycle:
`OpenAI-compat API error 400 … "anthropic.claude-sonnet-4.6 is not a valid model ID"` (sent to openrouter). The new F22 preflight did **not** catch it: `example-trend-follower` is fully agentless (`agents: None`), so it resolves a *default* trader model at runtime, which the preflight (inspecting declared agent refs) doesn't see. The fast-fail/guidance only covers strategies that *declare* an agent on a mismatched provider, not the agentless-default-model case. So mechanical example strategies still can't be cycled on this node.

---

## Observation / improvement (not a regression)
- **No live KEPT improvement yet.** The loop now keeps mechanically (integration test), but no live cycle has produced a mutation that beat `min_improvement = 0.05` on both day + baseline. With `mutations_per_parent = 1`, each cycle tries exactly one random-ish risk tweak — unlikely to find an improvement often. Worth considering: more mutations per cycle / a smarter mutator objective, so the optimizer converges rather than mostly dropping. This becomes cost-sensitive — which is blocked on F11 (an unmetered budget can't bound a wider search).

---

## Status of all prior findings
- **Fixed + verified:** F1–F10, F12, F13, F15, F16, F17, F19, **F14, F18, F20, F21**.
- **Open:** **F11** (cost/budget), **F22** (agentless default-model crash).

## Artifacts
- Run log: `/root/xvn-work/night-watch/optrun-v3-f20-131437.log` (cycle `01KT9CB0BAHQ2Q7HQFEJKFGDQG`, candidate `b5505dd671`)
- F22 repro: `xvn optimizer run-cycle --strategy example-trend-follower …` → anthropic 400 (cycle `01KT9CBK70720K2E8QCJ5VF8WS`)
- Cost evidence: `model_calls` gemini-3.1-flash-lite 22 calls / $0.1306 / null_cost 0, vs printed `$0.00`
