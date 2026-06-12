> Companion to the 2026-06-12 platform findings; capability issues #941–#945. Strategy/eval evidence lives in the xvn-app store (run IDs inline).

# Filter-centric profitability roadmap (mined 2026-06-12)

Premise (validated twice over): **the deterministic filter is the edge and the economics;
the LLM is a judgment layer consulted at moments the filter selects.** Every improvement
below either (a) lets filters select *more profitable moments* or (b) makes filter authoring
cheaper/safer. Items marked ✅ are filed as issues.

## Wave 2 — filed

| # | Capability | Profitability mechanism | Evidence |
|---|---|---|---|
| ✅ #941 | Position-aware tokens + `manage` block (pnl%, peak, giveback, bars_held, entry_price) | Deterministic profit/loss/time wakes → model banks or trails with judgment; beats fixed TP in trends, beats no-exit everywhere | s2b peaked ~+$11.5k, kept $4.5k; s3b +5% → −13.8% round-trip |
| ✅ #945 | Default-on trigger context + any-branch attribution | Model decisions measurably keyed to actual trigger; enables multi-setup filters with per-setup framing | fire-block-less filters wake the model blind today |
| ✅ #942 | Offline filter-replay + per-condition attribution + sweeps | Selectivity tuning in seconds, zero tokens; kills the dominant failure mode (over-selectivity) | v1 filters: 1–3 fires/run; each tuning loop cost a full eval |
| ✅ #943 | Conviction-scaled + risk-at-stop sizing | Monetizes conviction signal already emitted; equalizes realized risk per trade | deepseek conviction cleanly separated skips (≤0.35) from entries (0.75–0.85) |
| ✅ #944 | New tokens: choppiness, atr_dist_<ma>, close_pos_in_range, wick fractions, consec bars (stretch: Hurst, cross-TF anchors) | Replaces LLM judgment calls with free deterministic checks; fixes anti-chase caps | S1's RSI≤78 cap cost a +49% year; S4's model manually rejected a wick bar |

## Mined but NOT yet filed (next tranche, in priority order)

1. **Multi-setup filters as first-class** — one strategy, N entry archetypes via labeled
   `any` branches (breakout / pullback / squeeze), per-branch cooldowns and fire reasons
   (#945 is the prerequisite). Directly attacks fire sparsity without loosening any single
   setup. Strategy-side experiment as soon as branch attribution lands.
2. **Short-side strategy family** — symmetric filters (DI− dominance, close < MAs,
   breakdown crosses) for bear scenarios. Both bear kill-tests "passed" by sitting out;
   shorts monetize them. Needs prompt care re long bias: same trick as longs — only offer
   structurally-justified shorts. No platform work required today (short_open exists).
3. **Always-enter-on-fire baseline arm** — no-LLM arm reusing the strategy's filter
   (extends the baselines block). THE ablation for "does the model add ≥0.3 Sharpe over
   the filter alone" per research-wiki. Should ride the eval-structure rework.
4. **Partial-close action for manage wakes** — model banks half, trails rest (sltp.rs
   already has tp1 fraction machinery mechanically; the action surface doesn't).
   Follow-up flagged inside #941.
5. **Filter-aware episodic memory** — prior_episodes seed (#844) keyed by trigger context
   already lands; next: include "last N fires of THIS filter → outcomes" so the model sees
   its own setup's recent hit rate. Cheap, likely behavior-shaping; needs outcome-bias
   guard (only resolved past trades).
6. **Per-scenario fire-rate guardrail at eval validate** — warn when installed filter fires
   <3 or >max_decisions on the scenario's cached bars (one replay call once #942 exists);
   prevents burning evals on degenerate configurations.
7. **Daily/weekly loss-pause tokens** — `day_realized_pnl_pct`, `consecutive_losses` as
   manage/entry conditions (circuit-breaker style; complements daily_loss_kill_pct which is
   NAV-level and binary).
8. **Cooldown-after-loss vs cooldown-after-win asymmetry** — re-entry after a TP exit should
   be cheap; after a stop-out, expensive. `cooldown_bars_after_loss` / `…_after_win` once
   realized exits exist (#932/#941 prerequisite).

## Sequencing logic

Wave 1 (#932–#935) makes exits real → rerun 8-run matrix as realized-PnL baseline.
Wave 2 (#941/#945 first, #942 parallel, #943/#944 next) makes exits *smart* and authoring
cheap. Then the unfiled tranche, where 1–3 are strategy-side experiments more than platform
work. At each stage the same 4 strategies × 8 scenarios matrix re-runs for comparability.
