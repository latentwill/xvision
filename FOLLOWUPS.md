# Follow-ups — operational queue

Tactical work deferred during Phase 4–8 implementation. Not strategic
re-examinations (those live in `decisions/strategy-choices.md`); these are
scheduled tasks with a clear trigger or phase that should pick them up.

Format: title → trigger → scope → blocking?

---

## Blocking the headline run

### F1. Phase 4.3 hard gate — re-run spike directional match through candle runtime

- **Trigger:** production Conviction vectors extracted via `tools/extract_vectors/extract_vectors.py` on a rented GPU.
- **Scope:** call `xianvec_inference::substrate::validate_directional_match(bundle, holdout, engine)` on the loaded production vector; assert ≥0.75 match rate (matches spike empirical PASS). Drop the `#[ignore]` on the integration test.
- **Blocking:** YES for Phase 9.3 headline run. Without this gate, we don't know whether the candle runtime applies the loaded vector with the same semantics as the MLX spike.

### F2. Extract production Conviction vector (and Patience/Risk/Trend pipeline-only)

- **Trigger:** GPU access (Vast.ai/RunPod) provisioned.
- **Scope:** run `python tools/extract_vectors/extract_vectors.py --model Qwen/Qwen3-32B --spec specs/conviction.yaml --layers 20,32,42,50 --out data/vectors/conviction_v1` plus the same for `patience.yaml`, `risk.yaml`, `trend.yaml`. Generate Random + Orthogonal control vectors against the Conviction axis. Verify all manifests parse cleanly via `xianvec_inference::substrate::load_vector`.
- **Blocking:** YES for Phase 9 (cannot A/B vectors-on/off without vectors-on).

### F3. Live Trader-as-Strategy adapter

- **Trigger:** F1 + F2 land.
- **Scope:** thin wrapper in `crates/xianvec-eval/src/baselines/` (or its own `crates/xianvec-trader/` module) that implements `Strategy` over the Stage 1 Intern + Stage 2 Trader pipeline with a configurable `VectorConfig` (off | on | random | orthogonal). Phase 8.2 harness already takes `Box<dyn Strategy>`, so no harness changes.
- **Blocking:** YES for Phase 9.2 A/B runner.

---

## Blocking forward paper / on-chain

### F4. ERC-8004 manifests for both arms

- **Trigger:** Phase 6.5 lands.
- **Scope:** write `identity/vectors_off.agent.json` and `identity/vectors_on.agent.json` with model id, vector config, code commit, contact. Pin to IPFS or HTTPS. Mint via Phase 6.5's `IdentityClient::register` on Mantle testnet first; mainnet after Phase 9 eval clears.
- **Blocking:** YES for Phase 11.5 Orderly forward run on Mantle.

### F5. Orderly testnet credentials + smoke trade

- **Trigger:** Phase 6.3 lands.
- **Scope:** complete brokered onboarding once (`xvn setup --orderly-onboard` per plan §6.3); store `(orderly_key, orderly_secret, orderly_account_id)` in `op` (1Password); place + cancel a small `PERP_BTC_USDC` order against testnet to validate the full path. SDK errors mapped to `ExecutorError`.
- **Blocking:** YES for Phase 11.5.

---

## Phase 9 harness pre-flight (from Phase 4.5 audit)

### F6. `setup_id` reuse guard in the harness

- **Trigger:** Phase 9.1 ops crate work.
- **Scope:** harness rejects setups whose `setup_id` was already cached this run; cache key is `(setup_id, intern_provider, intern_model)` per Tier 1 fix #1. From `decisions/0005-lookahead-audit.md` follow-up #1.
- **Blocking:** non-blocking; defensive.

### F7. Lookahead-bias boundary-condition test

- **Trigger:** Phase 9.1 ops.
- **Scope:** unit test that constructs a `MarketSnapshot` whose `recent_bars.last().timestamp` is *after* `snapshot.timestamp` (an impossible state); harness should reject the snapshot rather than process it. From `decisions/0005-lookahead-audit.md` follow-up #2.
- **Blocking:** non-blocking; defensive.

### F8. Document `MarketSnapshot` invariants

- **Trigger:** Phase 9.1 ops.
- **Scope:** doc comment on `xianvec-core::market::MarketSnapshot` listing the temporal invariants (recent_bars.last().timestamp ≤ snapshot.timestamp; recent_bars chronologically ordered; horizon_hours non-negative). From `decisions/0005-lookahead-audit.md` follow-up #3.
- **Blocking:** non-blocking; documentation hygiene.

---

## Throughput / performance

### F9. Measure `target-cpu=native` numeric delta vs default codegen

- **Trigger:** stable bench rig (controlled thermal state, repeated trials with statistics).
- **Scope:** rerun smoke-qwen3 with and without `RUSTFLAGS="-C target-cpu=native"`, ≥10 trials each, report median + p95 decode/prefill tok/s. The current ADR 0007 §"Re-measured" cites 5–16 tok/s with 3.2× variance across 5 runs — that's not enough to commit to a single number. From `decisions/0007-inference-throughput-routes.md`.
- **Blocking:** non-blocking; numbers are advisory until forward paper exposes whether warm cache holds.

### F10. Codify `RUSTFLAGS=-C target-cpu=native` in `.cargo/config.toml`

- **Trigger:** F9 confirms the win is material (≥1.5×) and stable.
- **Scope:** add `[build] rustflags = ["-C", "target-cpu=native"]` so contributors don't have to remember the flag. From ADR 0007 recommendation.
- **Blocking:** non-blocking.

### F11. Shader pre-warm pass during engine init

- **Trigger:** cold-start latency materially affects the forward-paper experience (a 17–90 s wait per process start).
- **Scope:** `Qwen3Engine::new` runs a discard 1-token forward pass on a fixed prompt shape before returning, so the first user-visible prefill doesn't pay the JIT. From ADR 0007 v1 cold-start workaround.
- **Blocking:** YES for Phase 11.1 if cold-start latency is unacceptable to the operator; non-blocking otherwise.

### F12. mlx-rs viability spike (ADR 0007 option B)

- **Trigger:** F11 isn't enough OR the rented-GPU run (Phase 9.3) shows the same shader-JIT pattern at higher precision.
- **Scope:** 1–2 day spike: run `oxideai/mlx-rs` against the same Q4 weights; record TTFT + decode tok/s; verify `mlx-rs` exposes (or can be extended to expose) a pre-/post-block forward hook. From `decisions/0007-...` proposed plan.
- **Blocking:** conditionally blocking on cold-start.

---

## Deferred scope that's expected to come back

### F13. Phase 8.5 boundary probes (Glamin pattern formalization)

- **Trigger:** Phase 9.2 A/B runner stable; need a regression-detection net for vector / prompt / model changes.
- **Scope:** curated `data/probes/` corpus (ambiguous regime transitions, low-liquidity setups, hardest historical decisions, flash-crash conditions, regulatory edge cases). `ProbeRunner` in `xianvec-eval`; `IntrospectionHook`-attached when `--introspect` set. From implementation-plan §8.5.
- **Blocking:** non-blocking for v1 demo; recommended before Phase 11 forward paper.

### F14. Phase 7.5 onchain baselines

- **Trigger:** post-headline result if onchain comparison is needed for the demo narrative.
- **Scope:** Nansen smart-money copy-trader, funding-rate fader, stablecoin exchange-inflow risk-off, liquidation cascade fader. Each consumes `OnchainPanel` fields already present on `MarketSnapshot`. From implementation-plan §7.
- **Blocking:** non-blocking; data sourcing (Nansen API / DefiLlama-like aggregator) is its own project.

### F15. Bollinger / Donchian / Fibonacci baselines

- **Trigger:** v1.1; nice-to-have for richer comparison.
- **Scope:** three more `Strategy` impls under `xianvec-eval::baselines/`. Bollinger uses pre-computed `bb_*` fields; Donchian uses `donchian_*`; Fibonacci needs a small peak detector over `recent_bars`.
- **Blocking:** non-blocking.

### F16. Vectors-OFF / RANDOM / ORTHOGONAL experimental controls

- **Trigger:** F2 (extracted vectors) + F3 (Trader-as-Strategy adapter).
- **Scope:** the experimental "control" arms in Phase 9.2 A/B runner — three more Strategy adapters that wrap the Trader with each `VectorConfig`. Reuses the same Trader + Intern; differs only in which vector bundle is loaded.
- **Blocking:** YES for Phase 9.2 if the headline experiment depends on the Random/Orthogonal nulls. Non-blocking if a vectors-OFF vs vectors-ON two-arm comparison is acceptable for v1.

### F17. Indicator panel: add SMA(30) and SMA(90)

- **Trigger:** any baseline beyond MA-crossover wants 30/90; or v1.1 cleanup.
- **Scope:** `IndicatorPanel` currently exposes `sma_20/50/200` only. MA-crossover baseline computes 30/90 inline from `recent_bars` to avoid the schema change. Adding `sma_30: Option<f64>` and `sma_90: Option<f64>` to the panel pushes this computation upstream into `xianvec-data::indicators`. From Phase 7 implementation note.
- **Blocking:** non-blocking; cosmetic.

---

## Schema decisions awaiting trigger

### F18. Add `asset: AssetSymbol` to `TraderDecision` (resolves choices #1, #4 in `strategy-choices.md`)

- **Trigger:** multi-asset enabled in `whitelist.toml` (post-headline).
- **Scope:** schema field add + cascade through xianvec-trader (prompt schema), xianvec-intern (briefing format), xianvec-risk (drop the separate `asset` parameter), xianvec-execution (Alpaca + Orderly stop pinning to BTC), xianvec-eval (drop `BacktestConfig.instrument`). Mechanical but wide.
- **Blocking:** YES for multi-asset.

### F19. Add `VetoReason::TakeProfitTooTight` (resolves choice #2 in `strategy-choices.md`)

- **Trigger:** any other `VetoReason::Custom(...)` site lands in the codebase.
- **Scope:** one line in `xianvec-core::trading.rs` enum + serde rename + cascade through any exhaustive `match VetoReason {...}` — `xianvec-risk::rules::take_profit_rr` switches off `Custom("rr_too_low")`.
- **Blocking:** non-blocking; quality-of-enum.
