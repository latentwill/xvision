# XIANVEC v1 — Step-by-Step Build

Sequential build order distilled from `implementation-plan.md`. Each step links back to its phase/task for full context.

---

## M0. Venue verification ✅ (2026-05-03)
- `probes/m0-orderly/` — Orderly EVM gateway, BTC-PERP live, Mantle vault confirmed. **PASS**
- `probes/m0-byreal/` — Byreal Perps CLI v0.3.7, 20 capabilities. **PASS** (fork option)

---

## Phase 0 — Foundation & vector validation spike
1. **0.1** `cargo new` workspace; stub all `crates/xianvec-*` with one passing test each.
2. **0.2** Pull Qwen3-27B locally; candle smoke test (load + single forward pass).
3. **0.3** Vector validation spike — **CRITICAL GATE**, introspection mandatory. Document in `decisions/0002-spike-validation.md`.
4. **0.4** Vendor `byreal-agent-skills` and `mantle-skills` as submodules under `.claude/skills/`.

## Phase 1 — Schemas, config, persistence
1. **1.1** Schema crate (`xianvec-core`): types + `serde + garde` validation.
2. **1.2** Config loader (`config/default.toml`, `whitelist.toml`, `risk.toml`).
3. **1.3** SQLite persistence (`store.rs`) — decisions, briefings, traces.
4. **1.4** Technical indicators (`xianvec-data/indicators.rs`).

## Phase 2 — Stage 1 Intern
1. **2.1** Intern prompt builder.
2. **2.2** Intern via Anthropic SDK or local Qwen-7B; **`temperature=0`**; cache briefings keyed by `setup_id` (Tier 1 fix #1).

## Phase 3 — Stage 2 Trader (no vectors yet)
1. **3.1** Local model loader (`xianvec-inference`).
2. **3.2** Trader prompt + JSON-constrained generation.
3. **3.3** Smoke pipeline: Intern → Trader, no vectors.

## Phase 4 — Vector extraction
1. **4.1** Contrastive datasets per axis (200 pairs/axis). Extract Conviction (active) + Patience/Risk/Trend (pipeline-only).
2. **4.2** Python extractor (`tools/extract_vectors/`) using repeng.
3. **4.3** Rust vector loader (`xianvec-inference::substrate`); re-run spike's directional-match through runtime path as **Phase-4 hard gate** (Tier 1 fix #9).
4. **4.4** Steering hooks + confidence gating (`xianvec-gating`); gate at the `action` choice point, not `{` (Tier 1 fix #5). Backtest logs gate magnitude only — no dampened re-run (Tier 1 fix #7).
5. **4.4.1** Introspection hook (`xianvec-introspect`).
6. **4.1 controls** Extract random (norm-matched Gaussian) + orthogonal control vectors (Tier 2 fix #6).
7. **4.5** Lookahead bias audit → `decisions/0005-lookahead-audit.md`.

## Phase 5 — Risk Layer
- Deterministic rules in `xianvec-risk`. Pipeline owns risk; harness trusts the decision (Tier 3 cleanup).

## Phase 6 — Stage 3 Execution
1. **6.1** `Executor` trait.
2. **6.2** Alpaca executor (`alpaca.rs`).
3. **6.3** Orderly executor (`orderly.rs`) using `orderly-connector-rs = "0.4.15"` against `https://api-evm.orderly.org`.
4. **6.4** Backtest simulator with **stateful portfolio** — NAV, positions, daily PnL, loss streak, ATR (Tier 1 fix #3).

## Phase 6.5 — ERC-8004 identity (Mantle)
- Mint two `agentURI` NFTs (vectors-OFF, vectors-ON) via Identity Registry on Mantle mainnet using `alloy`. Manifests in `identity/`.

## Phase 7 — Baselines
- Buy-and-hold, momentum, etc. in `xianvec-eval/baselines/`.

## Phase 8 — Eval framework
1. **8.1** Returns + Sharpe; use `pnl_i / nav_initial` (constant denominator) (Tier 1 fix #8).
2. **8.2** Backtest harness; **`step >= horizon`** (default 24); block-bootstrap option (Tier 1 fix #4).
3. **8.3** Pre-committed metrics; risk layer at pipeline scope only (Tier 3).
4. **8.4** Anti-overfitting gate (reportable, not blocking).
5. **8.5** Boundary probes — minimal v1 corpus.

## Phase 9 — A/B experiment
1. **9.1** Ops (`xianvec-cli/src/ops.rs`).
2. **9.2** A/B runner: arms `off,on,random,orthogonal`, **`temperature=0`** both arms (Tier 1 fix #2), divergence on `(action, direction, size_bucket)`. BTC-only v1.
3. **9.3** Headline run on rented GPU at **8-bit (Q8_0) or 16-bit (bf16)** depending on card memory (Vast.ai/RunPod).

## Phase 10 — Demo polish
1. **10.1** CLI: `run-setup`, `show-decision`, `show-metrics`, `explain-vectors`.
2. **10.2** Report generator: Markdown + notebook plots; Δ-Sharpe inferential, MDD/PF/WR descriptive.

## Phase 11 — Forward trading
1. **11.1** Alpaca paper forward run, 4–7 days, paired alternating setups.
2. **11.5** Orderly forward run on Mantle (`PERP_BTC_USDC`), 5–20 paired trades; each closed trade posts to ERC-8004 reputation + validation registries on Mantle.

## Phase 12 — Acceptance checklist
See `implementation-plan.md` §Phase 12 for the full submission checklist (16 items).

---

## Telemetry (v1)
- `tracing` console subscriber + SQLite `traces` table flight recorder. OTel/Langfuse deferred to v2.

## Critical-path sequencing
M0 ✅ → Phase 0–8 (venue-independent) → 6.5 ERC-8004 ∥ 6.3 Orderly → Phase 9 backtest → 11.1 Alpaca paper → 11.5 Orderly on Mantle → Phase 12.
