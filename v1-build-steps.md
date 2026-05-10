# XVISION v1 — Step-by-Step Build

> **2026-05-07: CV build steps removed per ADR 0011.** Phase 0 vector
> validation spike, Phase 4 vector extraction, and Task 4.4.1 introspection
> hook installation are gone. xvision is now a CV-free multistrategy +
> ERC-8004 marketplace codebase.

Sequential build order distilled from `implementation-plan.md`. Each step links back to its phase/task for full context.

---

## M0. Venue verification ✅ (2026-05-03)
- `probes/m0-orderly/` — Orderly EVM gateway, BTC-PERP live, Mantle vault confirmed. **PASS**
- `probes/m0-byreal/` — Byreal Perps CLI v0.3.7, 20 capabilities. **PASS** (fork option)

---

## Phase 0 — Foundation
1. **0.1** `cargo new` workspace; stub all `crates/xvision-*` with one passing test each.
2. **0.4** Vendor `byreal-agent-skills` and `mantle-skills` as submodules under `.claude/skills/`.

## Phase 1 — Schemas, config, persistence
1. **1.1** Schema crate (`xvision-core`): types + `serde + garde` validation.
2. **1.2** Config loader (`config/default.toml`, `whitelist.toml`, `risk.toml`).
3. **1.3** SQLite persistence (`store.rs`) — decisions, briefings, traces. Decisions keyed on `(setup_id, arm_name)`.
4. **1.4** Technical indicators (`xvision-data/indicators.rs`).

## Phase 2 — Stage 1 Intern
1. **2.1** Intern prompt builder.
2. **2.2** Intern via Anthropic SDK or any OpenAI-compatible HTTP backend; **`temperature=0`**; cache briefings keyed by `setup_id` (Tier 1 fix #1).

## Phase 3 — Stage 2 Trader
1. **3.1** Trader backend (`xvision-trader`): `TraderBackend` HTTP trait + `OpenAiCompatBackend` impl. Optional local candle inference for air-gapped runs.
2. **3.2** Trader prompt + JSON-constrained generation.
3. **3.3** Smoke pipeline: Intern → Trader.

## Phase 5 — Risk Layer
- Deterministic rules in `xvision-risk`. Pipeline owns risk; harness trusts the decision (Tier 3 cleanup).

## Phase 6 — Stage 3 Execution
1. **6.1** `Executor` trait.
2. **6.2** Alpaca executor (`alpaca.rs`).
3. **6.3** Orderly executor (`orderly.rs`) using `orderly-connector-rs = "0.4.15"` against `https://api-evm.orderly.org`.
4. **6.4** Backtest simulator with **stateful portfolio** — NAV, positions, daily PnL, loss streak, ATR (Tier 1 fix #3).

## Phase 6.5 — ERC-8004 identity (Mantle)
- Mint per-strategy NFTs via Identity Registry on Mantle mainnet using `alloy`. One manifest per Strategy variant in `identity/`.

## Phase 7 — Baselines
- Buy-and-hold, momentum, etc. in `xvision-eval/baselines/`.

## Phase 8 — Eval framework
1. **8.1** Returns + Sharpe; use `pnl_i / nav_initial` (constant denominator) (Tier 2 fix #5).
2. **8.2** Backtest harness; **`step >= horizon`** (default 24); block-bootstrap option (Tier 1 fix #4).
3. **8.3** Pre-committed metrics; risk layer at pipeline scope only (Tier 3).
4. **8.4** Anti-overfitting gate (reportable, not blocking).

## Phase 9 — A/B experiment
1. **9.1** Ops (`xvision-cli/src/ops.rs`).
2. **9.2** A/B runner: paired strategy arms (e.g., `trader_arm`, `buy_hold`, classical TA, onchain), **`temperature=0`** for LLM-driven arms (Tier 1 fix #2), divergence on `(action, direction, size_bucket)`. BTC-only v1.

## Phase 10 — Demo polish
1. **10.1** CLI: `run-setup`, `show-decision`, `show-metrics`.
2. **10.2** Report generator: Markdown + notebook plots; Δ-Sharpe inferential, MDD/PF/WR descriptive.

## Phase 11 — Forward trading
1. **11.1** Alpaca paper forward run, 4–7 days, paired alternating setups.
2. **11.5** Orderly forward run on Mantle (`PERP_BTC_USDC`), 5–20 paired trades; each closed trade posts to ERC-8004 reputation + validation registries on Mantle.

## Phase 12 — Acceptance checklist
See `implementation-plan.md` §Phase 12 for the full submission checklist.

---

## Telemetry (v1)
- `tracing` console subscriber + SQLite `traces` table flight recorder. OTel/Langfuse deferred to v2.

## Critical-path sequencing
M0 ✅ → Phase 0–8 (venue-independent) → 6.5 ERC-8004 ∥ 6.3 Orderly → Phase 9 backtest → 11.1 Alpaca paper → 11.5 Orderly on Mantle → Phase 12.
