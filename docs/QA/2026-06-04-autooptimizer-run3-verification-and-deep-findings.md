# AutoOptimizer Run-3 Verification + Deep Findings (coding-agent handoff)

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 11:00Z, includes PR #804 `298438d3`/`bf8d4176` "fix F11-F19")
**Context:** Verifies the F11–F19 fixes, then digs into whether the optimizer can actually *complete a successful optimization end-to-end*. Short answer: **no — not on any strategy currently on this node.** Two independent root causes (F20/F21 for agent strategies, F22 for mechanical strategies), plus F11 still ineffective and an F18 regression.

---

## Verified fixed ✅

| # | Fix | Evidence |
|---|-----|----------|
| F12 | re-run unblock + cycle-safe ancestry | v3 reseeded (`"…marked rejected; reseeding it as an active root…"`) and ran; `lineage show` terminates (`depth=1 → 90c621fd (active) → [root]`) |
| F13/F19 | cycles are first-class | `optimizer ls` has a "Mutation cycles" section; `inspect <cycle>` shows candidates/Day-Hold-Sharpe/gate verdict; `GET /api/autooptimizer/cycles[/:id]` → HTTP 200 with full data |
| F15 | typed no-candidate | cycle emits `no_candidate{reason}` event + a distinct CLI "no candidate produced" summary |
| F16 | mutate-once blob | `mutate-once <hash> --mock --dry-run` → `Gate: passed`; `--blob-dir` defaults to `$XVN_HOME/lineage/blobs` |
| F17 | flywheel global view | `flywheel status` defaults to the `global` namespace with a note |

## Still open / regressed ❌

### F11 — cost still reports `$0.00`; budget still blind (fix ineffective)
A real cycle printed `cycle cost: $0.00 (metered paper-test + experiment-writer/judge inference)`, but `model_calls` recorded the real spend for `openrouter/google/gemini-3.1-flash-lite`: **62 calls, 1,442,668 input tokens, cost_usd = $0.369**.

**Root cause.** The new meter prices via `compute_token_cost_usd_from_catalog`, which needs `pricing_per_million_*_usd` on the catalog entry. The cached openrouter catalog has **no price** for `gemini-3.1-flash-lite` (`xvn provider models --name openrouter` shows its price column as `—`), so `positive_price(...)` returns `None` → the call contributes `$0` → cycle total `$0.00`. The accurate, provider-reported cost is already in `model_calls.cost_usd` ($0.369).

**Fix.** Source realized cost from `model_calls.cost_usd` (sum over the cycle's run_ids) instead of a catalog lookup that lacks the price — or fix `provider refresh-models` to ingest openrouter's pricing fields. Until then the `--budget` ceiling cannot trip for the primary provider.

### F14 — param mutation is dead for ~all real strategies
Param changes are dot-paths into `mechanical_params` (`mutator.rs:78`). But **only 3 of 28 strategies have non-empty `mechanical_params`, and all 3 are seeded examples.** All 25 user strategies (every `gemini_*`, the donchian ones) have `mechanical_params = {}` — their tunables live in `risk` (e.g. `stop_loss_atr_multiple`, `risk_pct_per_trade`) and the filter. The v3 cycle confirmed the failure mode: the mutator proposed `stop_loss_atr_multiple` and was rejected with `[unknown_param] Param 'stop_loss_atr_multiple' not found in strategy mechanical params` (it's a risk field, not a mechanical param). **No real strategy can be param-optimized.** Either teach the mutator/`apply_to` to address `risk.*` (and filter) params, or have the mutator only propose params that exist on the target.

### F18 — REGRESSION: `xvn optimizer demo` is now broken
The fix removed the `cycle_sealed` variant from the taxonomy but left it in the demo fixture, so the replay aborts midway: `malformed fixture event: unknown variant 'cycle_sealed', expected one of …`. The demo (the no-API-key smoke path) no longer completes. Regenerate the fixture to the current event vocabulary (drop `cycle_sealed`; the taxonomy now includes `no_candidate`).

---

## New deep findings (run-3)

### F20 — [CRITICAL] The optimizer has never kept/improved a candidate, and currently can't on any strategy on this node
There are **zero active lineage nodes with a parent** — i.e., no mutation has ever survived the gate as an improvement. The mutate → gate → **keep** loop is unproven end-to-end on a real strategy. Worse, it cannot currently succeed on *any* strategy here, because no strategy is simultaneously **mutatable** and **runnable**:

| Strategy class | Provider (paper-test) | Mutatable? | Outcome |
|---|---|---|---|
| `gemini_*` (real) | openrouter ✅ available | ❌ empty `mechanical_params` (F14) + no prose fallback (F21) | `no_candidate` every cycle |
| `example_*` (mechanical) | anthropic ❌ unavailable here (F22) | ✅ has `mechanical_params` | paper-test 400, cycle dies |

So the headline "does the optimizer work?" is: the *plumbing* runs, but the *core job* (produce + keep an improvement) has never happened and is currently blocked by F21 and F22. The mock path (`mutate-once --mock` → `Gate: passed`) proves the gate/keep machinery itself is sound; the gap is real candidate generation + runnable paper-tests.

**Acceptance.** Demonstrate at least one real cycle that produces a candidate, gates it on real backtests, and **keeps** it (active child node with a parent) — and add a test asserting a kept-candidate path.

### F21 — [HIGH] Mutator picks an invalid mutation kind and never falls back
`MutationDiff` supports `prose` (system-prompt edits), `params`, and `tools`. For `gemini_long_gate_v3` (empty `mechanical_params`, but a real agent `system_prompt`), the mutator proposed a **param** edit, failed 3×, and gave up — it never tried a **prose** mutation, which is the natural lever for an agent strategy. The mutator is not choosing a mutation kind valid for the target's tunable surface (params present? agent prompt present? tools present?). It should inspect the strategy and only propose applicable kinds (prose for prompt-driven agents, params for mechanical strategies), and fall back across kinds before declaring `no_candidate`.

### F22 — [HIGH] Mechanical/agentless strategies crash the cycle on a default `anthropic.claude-sonnet-4.6`; paper-test model isn't overridable
Running a cycle on `example-trend-follower` (`agents: None`, `activation_mode: every_bar`) dies with:
`OpenAI-compat API error 400 … "anthropic.claude-sonnet-4.6 is not a valid model ID"` (sent to `openrouter.ai`).
Even though `--provider openrouter --model google/gemini-3.1-flash-lite` was passed, the paper-test resolved a **default `anthropic.claude-sonnet-4.6` trader** (the seeded example attestation model, `authoring.rs:931`) and routed it through the cycle's openrouter dispatch → an anthropic model ID hit openrouter → 400. Two distinct problems:
1. **`--provider/--model` only override the mutator/judge** (`commands/autooptimizer.rs:1104-1108` set `cfg.mutator.*` only), never the paper-test trader. There is no way to redirect a strategy whose agent points at an unavailable provider, so such a strategy can never be cycled on this node.
2. **Provider/model routing mismatch:** an agentless strategy resolves to a default model whose provider (`anthropic`) is not registered, yet the dispatch sends that anthropic model ID to openrouter instead of failing fast with "provider anthropic not registered." (Compare the clean `require_launchable_provider` gate used for the mutator/judge.)

**Acceptance.** (a) A mechanical/agentless strategy either runs without an LLM trader or fails fast with a clear "provider not registered" error — never a confusing cross-provider 400. (b) Provide a way to run/optimize a strategy whose agent provider is unavailable (e.g. a paper-test provider/model override, or a clear preflight that blocks the cycle with guidance).

---

## Severity recap & suggested fix order
1. **F20 (critical, umbrella)** — make at least one real keep succeed; it's gated behind F21 + F22.
2. **F21 + F14** — candidate generation: mutator must choose a valid mutation kind (prose for agent strategies; risk/filter params or mechanical params for mechanical ones) and not propose non-existent params.
3. **F22** — agentless/unavailable-provider strategies: fail fast or allow a paper-test model override.
4. **F11** — meter realized cost from `model_calls.cost_usd` so cost is honest and `--budget` actually trips.
5. **F18** — regenerate the demo fixture (unbreak the no-key smoke path).

## What's solid
- Plumbing/observability fixes from earlier waves hold: cycles run end-to-end, lineage is unified + UI-visible (F8/F13), canary is quiet + labeled (F9), eval engine is shared (F10), re-runs unblocked + ancestry safe (F12), no-candidate is explicit (F15), mutate-once/flywheel/inspect work (F16/F17/F13).
- The gate/keep machinery itself is sound (`mutate-once --mock` → `Gate: passed`). The remaining work is real candidate generation (F21/F14) and runnable paper-tests (F22), then proving a kept improvement (F20).

## Artifacts
- v3 re-run (no_candidate, param error): `/root/xvn-work/night-watch/optrun-v3-f11f19-112351.log` (cycle `01KT9605JTQMT9MV14P4BY230S`)
- example-trend-follower (anthropic 400): `/root/xvn-work/night-watch/optrun-example-tf-113559.log` (cycle `01KT96PCDNER7EMX6AP2BWN3HB`)
- Cost evidence: `model_calls` — gemini-3.1-flash-lite 62 calls / 1.44M in-tok / **$0.369** vs printed `$0.00`; `xvn provider models --name openrouter` shows the model priced `—`
- mechanical_params survey: 3/28 strategies non-empty (all seeded examples)
- Lineage: no active node has a parent (no kept candidate ever)
