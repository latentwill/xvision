# Strategy choices — deferred for review

A short, living queue of tactical decisions made during implementation that
deserve a strategy-level look before v1 ships. ADRs (`0001`–`0007`) record
*decided* choices with full context. This file records *defaulted* choices
that were taken to keep work moving and that the operator wants to revisit
once the system is end-to-end runnable.

Format: title → origin → current default → question to settle → owner /
when to revisit.

---

## 1. `Action::Close` treatment in risk-layer limit rules

- **Origin:** Phase 5, `crates/xianvec-risk/src/rules/{max_total_exposure,max_open_positions,daily_loss_circuit}.rs`. Implementation surface required a decision because `xianvec-core::Action` has four variants (`Buy`, `Sell`, `Flat`, `Close`) and the implementation-plan §5 rule descriptions reference only `Flat`/non-flat.
- **Current default:** `Action::Close` is treated identically to `Action::Flat` for limit checks — it passes `MaxTotalExposure`, `MaxOpenPositions`, and the `DailyLossCircuit` veto unconditionally. Rationale: closing a position can only *reduce* exposure or open-count, so a limit rule that vetoes a close would be self-defeating.
- **Question to settle:** is `Close` ever valid as a *new* directional action that should be subject to size/exposure caps? Specifically: in a multi-asset world, can the agent emit `Close` against an asset it does *not* currently hold (a "flatten if I'm in" hint), and if so should that no-op pass silently or be rejected as malformed? Current default lets it pass silently.
- **Impact if revisited:** if `Close` should be rejected when no matching position exists, that's a new `VetoReason::ClosingNonexistentPosition` and a rule check. v1 BTC-only path is unaffected because the trader prompt schema constrains `Close` to existing positions.
- **Revisit when:** before multi-asset is enabled (post-headline-result, see `whitelist.toml`). Owner: pipeline / risk semantics.

---

## 2. `TakeProfitRR` modification uses `VetoReason::Custom("rr_too_low")`

- **Origin:** Phase 5, `crates/xianvec-risk/src/rules/take_profit_rr.rs`. The R/R-widening modification needs a `VetoReason` to attach to `RiskDecision::Modified { reason }`, but `xianvec-core::trading::VetoReason` does not enumerate a dedicated variant for this case.
- **Current default:** `VetoReason::Custom("rr_too_low")`. Functionally correct — the verdict is recorded, downstream code can match on the string — but the catch-all `Custom` variant erodes the value of the enum's exhaustiveness.
- **Question to settle:** add a first-class variant (e.g. `RiskRewardTooLow` or `TakeProfitTooTight`) to `VetoReason` so the rule's modification reason appears in the schema, the decision divergence analysis can group on it cleanly, and audit dashboards don't need string parsing. One-line schema add + serde rename + cascade through any `match VetoReason {...}` blocks.
- **Impact if revisited:** schema migration in `xianvec-core::trading.rs` only; no SQL migration (the `risk_outcomes` table stores reasons as JSON-tagged enum). Trade-off: every `Custom(_)` site we add is a small claim that the enum is incomplete; the cleaner read is to add the variant now while the enum is still small enough to keep cohesive.
- **Revisit when:** any other `Custom` reason gets added in Phase 6 or Phase 8 — that's the trigger that says "the enum is no longer exhaustive enough." Owner: schema / pipeline. (See FOLLOWUPS.md F22.)

---

## 3. `apca` (Alpaca client) is hyper-tls only — no rustls path

- **Origin:** Phase 6.2, `crates/xianvec-execution/src/alpaca.rs`. `apca 0.30` is the only mature Alpaca Rust client; it depends on `hyper-tls` and exposes no rustls feature flag. Workspace convention elsewhere leans rustls.
- **Current default:** accepted hyper-tls for v1 with a code comment in `alpaca.rs`. Functionally fine on macOS/Linux; ships with OpenSSL via `native-tls`.
- **Question to settle:** should the workspace standardize on rustls across the board (Orderly, future webhook listeners, etc.), and if so does that justify either (a) forking `apca` to add a rustls feature, (b) writing a thin Alpaca client in-house against the documented REST API, or (c) accepting the dual-TLS-stack reality?
- **Impact if revisited:** option (a) is a vendor + maintain burden but trivial in scope (~50 LoC change, one PR upstream). Option (b) is ~300 LoC for the surface we use (orders, positions, account, bracket). Option (c) means tracking two TLS lineages for security advisories and binary-size audits.
- **Revisit when:** any second hyper-tls client lands in the workspace (forces the decision), or a security advisory hits OpenSSL and we want a single-stack surface to patch. Owner: build/ops.

---

## 4. `BacktestConfig.instrument: AssetSymbol` pins one asset per backtest run

- **Origin:** Phase 6.4, `crates/xianvec-eval/src/backtest.rs`. `TraderDecision` has no `asset` field (cf. choice #1's Option A in Phase 5 risk wiring), so the simulator pins `instrument` at config time and applies every submitted decision to that asset.
- **Current default:** one `BacktestExecutor` per asset; multi-asset backtests would require multiple parallel runners and a higher-level orchestrator that assigns each setup to its instrument's runner.
- **Question to settle:** sibling of #1. If `TraderDecision` ever carries `asset`, `BacktestConfig.instrument` becomes redundant and the runner can host a single portfolio across assets. The richer alternative is what enables Tier 2 fix #10 (multi-asset backtests with cross-asset correlation + concentration risk).
- **Impact if revisited:** schema change in `xianvec-core::trading.rs` cascades through the harness, the executors (Alpaca + Orderly), the risk layer's `evaluate(asset)` parameter, and the backtest's per-asset state. Non-trivial but mechanical.
- **Revisit when:** multi-asset is enabled in `whitelist.toml`. Owner: schema / pipeline.

---

## 5. Decision-divergence rate denominator = paired-setups-only

- **Origin:** Phase 8.3, `crates/xianvec-eval/src/metrics.rs`. The `decision_divergence_rate` field of `PreCommittedMetrics` walks `arm_a.decisions` and `arm_b.decisions` paired by `setup_id`; the denominator is `min(arm_a.decisions.len(), arm_b.decisions.len())`.
- **Current default:** "of the setups where both arms decided, what fraction had different (action, direction, size_bucket)?". Excludes setups where one arm returned `None` (e.g. RSI baseline silent on neutral RSI).
- **Question to settle:** is "no decision" itself a divergence? If arm A buys and arm B is silent, the trader behaviour *did* diverge in a behaviorally meaningful way — but the metric currently doesn't count it. A "loud-vs-silent" divergence might be material when comparing the live Trader (always emits) against a sparse baseline (RSI / MACD).
- **Impact if revisited:** broaden the denominator to `union(setup_ids)` and treat any single-side decision as a divergence in `decision_divergence_rate`. Headline number changes — likely *increases* — when one arm is sparser than the other. May want to surface both numbers (paired-only and union) and pick the headline at report time.
- **Revisit when:** the headline run pairs the live Trader against any baseline that returns `None` frequently. Owner: eval.

---

## 6. Block bootstrap is fixed-block, not stationary (Politis & Romano 1994)

- **Origin:** Phase 8.1, `crates/xianvec-eval/src/bootstrap.rs`. Implementation uses non-overlapping fixed-length blocks drawn with replacement.
- **Current default:** fixed block size `b` configured at call time; deterministic given seed.
- **Question to settle:** if the chosen block length materially shifts CI width (e.g. running with `b = 4` vs `b = 8` produces qualitatively different gate verdicts on the same arm pair), the stationary block bootstrap (random block lengths drawn from a geometric distribution) is the technically correct upgrade. Otherwise fixed-block is fine.
- **Impact if revisited:** ~80 LoC change in `bootstrap.rs` to add the geometric-distribution variant; deterministic-seed contract preserved.
- **Revisit when:** the headline run's `delta_sharpe.ci_low` straddles zero AND the block-size choice flips the verdict. That's the trigger that says "the test is sensitive to a methodology choice we waved at." Owner: eval / methodology.

---

## See also

- `decisions/0007-inference-throughput-routes.md` — option B (mlx-rs spike) is deferred until cold-start latency materially blocks forward paper. Not a strategy choice in the same sense (it's a measurable trigger), but related deferral.
- `decisions/0005-lookahead-audit.md` "Follow-ups" — three Phase 9 harness items (setup_id reuse guard, boundary-condition test, snapshot-invariant docs). Tactical, not strategic; tracked in `FOLLOWUPS.md`.
- `FOLLOWUPS.md` — operational TODO queue keyed by phase / trigger. Tactical work that's been deferred but isn't a decision-revisit.
