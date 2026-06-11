# DeepSeek V4 Pro × 1h Multi-Asset Eval — Requirements

**Date:** 2026-06-03
**Status:** Ready for execution
**Skill:** `xvision-cli` (operator/usage)
**Image under test:** `xvision:deploy-latest` (built 2026-06-03 03:42 UTC), branch `codex/multi-asset-tool-asset-guard`, `xvn 0.21.0`, container `xvn-app`.

## Goal

Author a new strategy and backtest it with **DeepSeek V4 Pro** (`deepseek/deepseek-v4-pro`), targeting **Sharpe ≥ 3 over ≥ 10 closed trades, pooled across a multi-regime scenario set**. The run doubles as the acceptance test for the newly-pushed image, which is claimed to fix the harness gaps that previously made this target unreachable.

## Why now

The 2026-06-02 findings (`docs/QA/2026-06-02-eval-harness-position-communication-findings.md`, PR #763) concluded a Sharpe > 2 over 10+ trades was **not honestly reachable** on `xvn 0.21` — not for strategy-design reasons, but because of harness gaps. The operator reports the new image addresses them, so this eval re-tests feasibility on the hardest path (1h intraday, multi-asset).

## Success criteria

- **Primary:** pooled Sharpe ≥ 3.0 **and** ≥ 10 closed (round-trip) trades, measured on the combined trade stream across all scenarios — not the best single window.
- **Honesty guard:** report per-scenario and per-asset breakdowns alongside the pooled number. A pooled Sharpe driven entirely by one regime is flagged, not claimed as a win.
- **Acceptance-test deliverable (independent of hitting the number):** a clear verdict on whether the four claimed fixes actually work in the deployed image, with evidence.

## Preflight gate — verify the rebuild BEFORE chasing Sharpe

Run the safe-launch order (`doctor` → `provider check/models` → `strategy diagnostics` → `eval validate`) and additionally confirm each previously-broken capability. If any of 1–3 fails, **stop and report** — a blind trader cannot produce an honest edge.

1. **Indicator panel → trader (was finding #2).** Confirm the eval trader actually *receives* computed indicators (not "SMA50 not provided"). Check via `xvn trader preview` / a single `eval validate` + run inspection that the briefing contains real indicator values.
2. **SL/TP in TraderDecision (was finding #3).** Confirm stop-loss / take-profit fields exist in the eval trader schema and that positions auto-close on them (not only on a later wake).
3. **Filter wake frequency (was finding #1).** Confirm filters wake on every qualifying bar / crossing operators behave, so trade count can clear 10+ rather than firing only on false→true edges.
4. **Multi-asset end-to-end (the `asset-guard` commit).** Confirm a single strategy can trade a multi-asset universe in one eval, market-data tools are guarded by the decision asset, and PnL pools correctly on the shared `$100,000` PortfolioBook. Resolve the pooling model here: **one strategy with an asset universe** vs **per-asset batch** — pick whichever the harness actually supports and document it.

## Strategy design (baseline)

- **Archetype:** trend-following with ADX/EMA confirmation, now that indicators reach the model. Trends give asymmetric payoff; SL/TP control the downside — the combination most likely to yield a high Sharpe.
- **Asset universe:** BTC/USD + ETH/USD, add SOL/USD if 1h bars are cached. Verify cache with `xvn bars ls` before committing.
- **Timeframe:** 1h.
- **Provider/model:** `openrouter` / `deepseek/deepseek-v4-pro` (1M ctx, 384k max output, non-reasoning).
- **Filter-gate (mandatory for cost):** selective deterministic filter (e.g. `adx_14 > 20`, DI alignment, EMA fast>slow, with a short `cooldown_bars`) so the model is woken only on genuine trend setups — controls both overtrading and token spend. Consult `xvn strategy filter-catalog --json` before authoring.
- **Risk sizing:** set `risk.risk_pct_per_trade` deliberately (note the known `set-filter`-resets-risk ordering gotcha from finding #8 — verify the final strategy JSON has both the filter and the intended risk).

## Scenario set (multi-regime, multi-asset)

Select ~4–6 1h windows spanning **bull / chop / selloff** using `xvn scenario select` / `classify` so the pooled trade stream sees every regime. Reuse the regime anchors from prior work where 1h bars exist (e.g. FTX-era crash/rally/chop weeks plus a clean trend week). Each scenario should contribute ~2–3 trades toward the 10+ pooled target.

## Execution plan

1. Preflight gate (above). Gate the rest on it.
2. `xvn bars ls` — confirm 1h cache for each universe asset; fetch gaps.
3. Author strategy (`strategy new`), install filter (`set-filter`), verify final JSON, `strategy diagnostics`.
4. `eval validate` per scenario, then a small **cost probe** (1 scenario) to measure tokens/$ per run before the full sweep.
5. Full sweep via `eval batch` / `experiment run --wait --compare --markdown`.
6. Compute pooled Sharpe + trade count; produce per-scenario/per-asset breakdown.
7. Iterate filter/risk/prompt if short of target; re-run.

## Risks & assumptions

- **1h pooled Sharpe 3 is ambitious** even with the fixes. If we miss, the deliverable is the best achievable plus a precise statement of what still gates 3.
- **Token/cost:** 1h × multi-asset × ~17–24k tokens/model-call. Filter-gating is mandatory; estimate cost on the 1-scenario probe before the full sweep.
- **DeepSeek V4 Pro known-issue watch:** instrument for schema/JSON-adherence failures, empty/`noop` decisions, refusals, and 384k-output truncation. Log concrete repro for any error (the operator flagged prior issues with this model). Capture findings to memory and/or a `docs/QA/` note.
- **Synthesized rows:** treat high `noop_skip` / early-stop-inheritance counts as a QA caveat when reading trade counts (eval details separate real model decisions from synthesized rows).

## Out of scope / deferred

- Walk-forward / out-of-sample validation (deferred until pooled multi-regime passes).
- DSPy `optimize` prompt/demo tuning.
- Live / forward-paper trading.
- Dashboard UI changes.

## Handoff

Findings (especially the four-fix verdict and any DeepSeek V4 Pro errors) should land in a dated `docs/QA/` note mirroring the PR #763 format, and the reusable workflow/gotchas captured to operator memory.
