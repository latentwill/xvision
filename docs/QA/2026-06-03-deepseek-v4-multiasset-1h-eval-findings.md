# Eval QA — DeepSeek V4 Pro × multi-asset 1h (2026-06-03)

**Image:** `xvision:deploy-latest` built 2026-06-03 03:42 UTC, `xvn 0.21.0`, branch `codex/multi-asset-tool-asset-guard` (HEAD "Guard market data tools by decision asset").
**Container:** `xvn-app` (`$XVN_HOME=/data`).
**Model:** `deepseek/deepseek-v4-pro` (openrouter), non-reasoning, 1M ctx.
**Strategy:** `deepseek-v4-trend-1h` (`01KT64K2106GGWGSWB3JFERRCA`) — long-only 1h trend-follower, filter-gated (`adx_14>20 & di_plus_14>di_minus_14 & ema_12>ema_26 & close>ema_26`, `cooldown_bars=4`, `fire.context` carrying close/EMA/ADX/DI/ATR/RSI), `asset_universe=[BTC/USD, ETH/USD, SOL/USD]`, `execution_mode=per_asset`.
**Verification run:** `01KT64N51B9BH9MPYSXRSJF6T5` — scenario `example-quickstart-btc-bull-jan-2025` (2025-01-06..2025-01-13), backtest. Completed, `error=null`, cost $0.054, 114,526 tokens.

This run was a preflight acceptance test of the four harness gaps documented in `docs/QA/2026-06-02-eval-harness-position-communication-findings.md` (PR #763). Verdict below is from the decision-level `eval export`, not run summaries.

## Verdict summary

| # | Prior blocker | Status in this build | 
|---|---|---|
| #2 | Eval trader can't see computed indicators | ✅ **FIXED** |
| #4 | (new capability) multi-asset universe | 🔴 **BROKEN — critical** |
| #3 | No stop_loss/take_profit in eval trader schema | 🟠 **PARTIAL** |
| #1 | Filters fire only on false→true edge / sparse wakes | 🟠 **PARTIAL** |

## ✅ FIXED — indicators now reach the trader (was #2)

The trader's reasoning quotes real computed indicator values rather than "not provided":

> "All LONG conditions confirmed: EMA_12 (101657.67) > EMA_26 (100665.48), price (101737.03) above EMA_26. ADX at 48.86 … +DI (31.19) well above -DI (14.60). RSI at 65.43 …"

Mechanism: the filter's `fire.context` indicator list plus the auto-attached `agent_context_template: "compact_trade_context_v1"` render the indicator snapshot into the briefing. This is the single most important fix — the trader can now make indicator-conditioned decisions.

## 🔴 CRITICAL — multi-asset market data is not isolated per asset

With `asset_universe=[BTC/USD, ETH/USD, SOL/USD]` and `execution_mode=per_asset`, **every asset is fed BTC's bars, indicators, and fill price.** From the export's `long_open` decisions (all stamped `2025-01-07T04:00:00Z`):

| asset | fill_price | order_size | indicator values in reasoning |
|---|---|---|---|
| BTC/USD | 101769.59 | 0.014739 | EMA_12 101657.67, ADX 48.86, RSI 65.43 |
| ETH/USD | **101769.59** | 0.014739 | **identical (BTC's)** |
| SOL/USD | **101769.59** | 0.014739 | **identical (BTC's)** |

ETH traded ~$3.3k and SOL ~$200 in Jan 2025; filling them at **$101,769** is impossible. The `events`/`indicator_snapshot` stream in the export contains **only BTC's price series** (close 101737 → 94673) with **no per-asset events** (`asset` field absent). The model even rationalizes BTC levels for SOL ("Recent pullback from ~102692 high").

**Impact:** any multi-asset eval result is meaningless — it is one BTC decision cloned across the universe. This is a direct regression/incompleteness of the very work this branch is named for ("Guard market data tools by decision asset"). The guard is not routing per-decision-asset market data into the briefing/fill path.

**Repro:** author any multi-asset strategy (`strategy new --assets BTC,ETH,SOL --timeframe 1h`), run one eval, `xvn eval export <run_id>` → inspect `decisions[].fill_price` across assets (all equal) and `events[].indicator_snapshot` (BTC-only).

## 🟠 PARTIAL — SL/TP still not enforceable from the decision (was #3)

- The `TraderDecision` schema still has **no `stop_loss`/`take_profit` field**. Decision keys: `ix, ts, asset, action, conviction, justification, reasoning, order_size, fill_price, fill_size, fee, pnl_realized`.
- The model produces precise brackets **in prose** ("SL at ~100879 (1.5×ATR), TP at ~103453 (3×ATR)") and the harness **discards them** — they cannot be enforced.
- A deterministic stop exists only at the risk layer: `risk.stop_loss_atr_multiple=2.0`. There is **no take-profit** mechanism at all (neither config nor decision).
- Net: model-chosen, per-trade asymmetric SL/TP (the thing a trend strategy needs for a high Sharpe) is not honored; only a fixed 2×ATR stop is.

## 🟠 PARTIAL — wakes still sparse (was #1)

`filter_summaries`: `bars_scanned=169, wakeups=2, suppressed_cooldown=8, suppressed_in_position=0, llm_calls_saved=167`. Only **2 wakeups in a 169-bar week** — too sparse to reach 10+ genuine trades per scenario. Filter-gating works and saves cost, but level-gate + `cooldown_bars=4` produces few wakes; longer windows or crossing/looser conditions are needed for trade count.

## Minor

- **`max_concurrent_positions=2` violated:** 3 simultaneous `long_open`s were opened (one per asset). (Likely a side effect of the multi-asset contamination path bypassing the cap, or the cap not enforced in backtest.)
- **Misleading counts persist (prior #4):** `win_rate=0.00` with `n_trades=3` where the 3 are unclosed entries.

## DeepSeek V4 Pro behavior

**No model errors this run** (`errors: []`, `run.error: null`). The model was schema-adherent, returned well-formed `TraderDecision`s with calibrated `conviction` (0.78–0.85), correct indicator arithmetic, and disciplined long-only behavior. The issues flagged from prior sessions did **not** recur here. Cost is trivial ($0.054 / 114k tokens for a 3-asset week). Continue to watch across longer/multi-regime runs for schema drift, empty/`noop` decisions, or 384k-output truncation.

## Recommended fixes (dev side, `xvision-dev`)

1. **(Critical)** Route per-decision-asset bars + indicator snapshot into the briefing and fill path under `per_asset` execution; emit per-asset `events`/`filter_events`. Add a regression test asserting `decisions[].fill_price` differs across assets with divergent price levels.
2. Add `stop_loss`/`take_profit` to the eval `TraderDecision` schema and honor model-set brackets in the backtest executor (plus a config take-profit analog to `stop_loss_atr_multiple`).
3. Enforce `max_concurrent_positions` in the backtest path.
4. (Lower) Make `n_trades`/`win_rate` count closed round-trips, not legs.
