# xvision 30-Day 1d-Candle Eval QA Report

**Goal:** Create a new strategy, run a 30-day daily-candle backtest eval through
xvision, and loop until COMPLETE with 30 decisions + finalized review.

**Agent model target:** DeepSeek V4 Pro (OpenRouter)

**Strategy:** BollingerATRBreakout — Bollinger Band breakout confirmed by ATR
volatility threshold.

---

## 1. Strategy Design

### 1.1 New strategy: BollingerATRBreakout

- **Thesis:** Bollinger Band breakouts are more reliable when ATR confirms
  elevated volatility. Enter in breakout direction; size risk via ATR.
- **Signal:**
  - `close > bb_upper && atr_14 / close > 0.8%` → Long
  - `close < bb_lower && atr_14 / close > 0.8%` → Short
- **Risk:** Stop = 1.5×ATR, Target = 3.0×ATR, Size = 600 bps.
- **Files created:**
  - `strategies/bollinger_atr_breakout.md`
  - `crates/xvision-eval/src/baselines/bollinger_atr_breakout.rs`
  - Updated `crates/xvision-eval/src/baselines/mod.rs` (module + re-export + v1 set + tests)

### 1.2 Build verification

- **Status:** BLOCKED — no Rust toolchain on this host (`cargo` / `rustc` not found).
- **Impact:** Cannot run `cargo check`, `cargo test`, or compile the baseline.
- **Workaround:** The code is syntactically correct by inspection; it follows the
  exact patterns of `rsi_mean_reversion.rs` and `macd_momentum.rs`.
- **Next step:** Build must be verified on a host with Rust installed, or via
  the GHCR CI pipeline.

---

## 2. Eval Surface Assessment

### 2.1 Available surfaces

| Surface | Status | Notes |
|---------|--------|-------|
| Local `xvn` CLI | NOT AVAILABLE | Binary not on PATH; no Rust toolchain to build |
| Remote CLI / Tailscale | UNKNOWN | `xvn.tail2bb69.ts.net` may exist but not probed yet |
| WebUI dashboard | UNKNOWN | Not inspected yet |
| Direct HTTP API | UNKNOWN | Needs endpoint + auth |

### 2.2 Key constraints from skill references

- **Backtest warmup:** `WARMUP_BARS = 200`. A 30-day daily scenario has only
  ~30 bars → **insufficient for native backtest eval**.
  - Need **231+ daily bars** to get 30 post-warmup decisions.
  - This is a **product/workflow gap** for literal "30-day daily" evals.
- **Provider requirement:** Eval needs an explicit provider/model on the
  strategy slot or attached agent. DeepSeek V4 Pro on OpenRouter is the
  requested model.
- **Local-candle mode:** Can bypass model checks for deterministic baseline
  eval, but still needs >201 bars.

---

## 3. Blockers

| # | Blocker | Severity | Resolution path |
|---|---------|----------|-----------------|
| 1 | No Rust toolchain on host | HIGH | Install Rust, or build via GHCR Actions, or use remote eval surface |
| 2 | 30-day daily candles < 200 bars = backtest warmup fail | HIGH | Use 231+ day scenario, or use paper/live eval instead of backtest |
| 3 | Remote xvision endpoint not yet probed | MEDIUM | Test connectivity to `xvn.tail2bb69.ts.net` / Coolify deploy |
| 4 | DeepSeek V4 Pro provider config not yet set in xvision | MEDIUM | Add OpenRouter provider with `deepseek/deepseek-v4-pro` model |

---

## 4. Next Actions

1. **Install Rust** on this host or confirm a build host:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   cargo check -p xvision-eval
   cargo test -p xvision-eval baselines::bollinger_atr_breakout
   ```

2. **Probe remote eval surface:**
   - Check if `https://xvn.tail2bb69.ts.net/` is reachable.
   - Verify `/api/health`, `/api/settings/providers`, `/api/strategies`.
   - If unreachable, check Coolify dev deploy status.

3. **Configure DeepSeek V4 Pro provider** in xvision:
   - Provider: `openrouter`
   - Model: `deepseek/deepseek-v4-pro`
   - API key: `OPENROUTER_API_KEY`

4. **Create scenario with 231+ daily bars** (e.g. 2024-06-01 to 2025-01-18)
   instead of literal 30 days.

5. **Run eval** via preferred surface (remote CLI job API or WebUI).

6. **Loop until:** status = COMPLETE, 30 decisions present, review finalized.

---

## 5. QA Notes / Friction Log

- **Friction #1:** No local Rust toolchain means we cannot verify compilation
  before pushing. This is a dev-environment gap.
- **Friction #2:** The "30-day strategy with 1-day candles" request conflicts
  with the 200-bar warmup minimum. Need to educate user or adjust scenario
  length.
- **Friction #3:** `xvn` binary is not on PATH on this host — consistent with
  skill warning that `xvn` may only exist inside the build/runtime container.
- **Friction #4:** No `CARGO_TARGET_DIR` set; if we do install Rust, should set
  shared target to avoid disk bloat per CLAUDE.md guidance.

---

*Report started: 2026-05-16*
*Strategy implemented: BollingerATRBreakout*
*Build status: UNVERIFIED (no Rust toolchain)*
*Eval status: NOT STARTED (blocked by build + scenario length + remote surface)*
