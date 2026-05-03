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

### F4. ERC-8004 manifests for both arms + harness wiring (runtime-optional)

- **Trigger:** Phase 11.5 forward run on Mantle.
- **Scope:** Phase 6.5 already shipped placeholder `identity/vectors_{off,on}.agent.json` with `code_commit=PENDING`, `contact=PENDING`, and (for vectors_on) `manifest_hashes=["PENDING_PHASE_4_2_EXTRACTION"]`. Before the forward run, fill these from `git rev-parse HEAD` and the actual production vector manifest hashes from F2; mint via `IdentityClient::register` on Mantle testnet first, mainnet after Phase 9 eval clears. Harness wiring must be **runtime-optional** — gate behind a config flag (`identity.enabled = true/false` in `config/default.toml`) so the harness runs without Mantle credentials when identity is disabled, and `xianvec-identity` stays an opt-in workspace member (excluded from `default-members`; explicit `--workspace` or `-p xianvec-identity` to include).
- **Blocking:** YES for Phase 11.5 Orderly forward run on Mantle. Non-blocking for Phase 9 backtest (no on-chain dep).

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

### F19. Re-adopt `orderly-connector-rs` SDK when its `zeroize` pin loosens

- **Trigger:** `orderly-connector-rs` releases a version that no longer transitively pins `zeroize = "=1.3.0"` (currently 0.4.15 does, via `solana-sdk` → `ed25519-dalek 1.x`). The pin conflicts with `rustls 0.23` (workspace `reqwest 0.13`'s TLS) which needs `zeroize ≥ 1.7`.
- **Current state:** Phase 6.3 reimplements the five required Orderly REST endpoints directly via signed `reqwest` + `ed25519-dalek 2.x` calls. Signing scheme is byte-identical to the SDK's `auth::generate_signature` (Ed25519 over `${ts}${METHOD}${path}${body}`, base64-encoded, secret base58). Tests cover the path; ergonomics of the SDK are gone.
- **Scope:** swap the in-house REST shims for SDK calls (`OrderlyService::create_order`, `create_algo_order`, `cancel_order`, `get_account_info`, `get_positions`, `get_futures_info`). Keep the `OrderlyApi` trait so tests stay independent. Strip the local signing code.
- **Blocking:** non-blocking; current implementation is functional. Follow-up only matters for code-mass and SDK-feature pickup (e.g. WebSockets if v2 wants live mark-price streams).

### F20. Upstream PR: gate Solana stack in `orderly-connector-rs` behind a feature

- **Trigger:** any time before F19's re-adoption (or never, if Orderly upstream fixes it without our PR).
- **Current state:** F19 documents the workspace-side workaround. The conflict is *not* workspace-specific — `orderly-connector-rs 0.4.15` has no `[features]` section, hard-pulls `solana-sdk = "=1.16.13"` + `solana-client = "=1.16.13"` + `ed25519-dalek 1.0` + `zeroize = "=1.3.0"` even for EVM-only users (the only consumer surface that actually exists for Mantle v1). Anyone in the modern async/rustls Rust ecosystem hits it.
- **Scope:** PR against `ranger-finance/orderly-connector-rs` adding:
  - `[features] default = ["solana", "evm"]` to preserve current behavior.
  - `solana-sdk`/`solana-client`/`solana_vault_cpi` and `ed25519-dalek 1.x` made `optional = true`, gated behind `feature = "solana"`.
  - For the `evm` feature, depend on `ed25519-dalek 2.x` (no zeroize pin); the EVM gateway's Ed25519 signing scheme works under either major.
  - Drop the `zeroize = "=1.3.0"` exact pin; let cargo resolve it.
- **Impact if landed upstream:** F19 collapses to "switch from in-house REST shims to `OrderlyService` calls behind `default-features = false, features = ["evm"]`." ~30–50 LoC PR upstream; tests should cover both `--features solana` and `--features evm` invocations.
- **Blocking:** non-blocking. Worth filing whether or not we want to take F19 ourselves; the wider Rust EVM ecosystem benefits.

### F21. Replace HTTP-backend Intern with an OpenClaw / ACPX agent-harness backend

- **Trigger:** Phase 9 result is positive and we want to push the Intern's analytical depth before forward paper, OR Phase 11 forward run shows the Intern is the bottleneck on hard setups.
- **Current state:** Phase 2.2 ships `OpenAICompatIntern` and `AnthropicIntern` — both single-shot LLM calls that take a prompt and emit `InternBriefing`. The backend trait surface is interchangeable by design (Tier 1 fix #1 + plan §2.2), so a new backend impl plugs in cleanly without touching the prompt builder, cache, or trader.
- **Scope:** add a third Intern backend that drives an agent harness (OpenClaw / ACPX or equivalent — research the current options before committing) instead of a single completion call. Multi-step reasoning, tool use (price fetchers, indicator recomputation, onchain queries), constrained-output gating, retries with critique. The harness still has to terminate at an `InternBriefing` matching the existing schema; everything new is internal to the backend.
- **Open questions to resolve in the spike:**
  - Which harness — pinned upstream framework, or a thin home-rolled loop? OpenClaw and ACPX are research candidates; LangGraph / Autogen / CrewAI / Inngest agent kit are alternatives.
  - Does the harness call out to `xianvec-data` for indicator recomputation (giving the Intern a tool to interrogate market state beyond what the snapshot prebakes), or does it stay snapshot-only?
  - Cost / latency profile vs single-shot — agent harnesses can 5–10× the wall time and token spend; need a budget cap and a fallback to single-shot when the budget is hit.
  - Determinism for backtest (Tier 1 fix #2) — agent loops with tool use are inherently non-deterministic unless temperature=0 *and* all tool calls are deterministic. Backtest may have to use the simpler single-shot backend even after this lands.
- **Blocking:** non-blocking; pure capability lift. The current single-shot Intern is sufficient for the v1 headline result.

### F22. Add `VetoReason::TakeProfitTooTight` (resolves choice #2 in `strategy-choices.md`)

- **Trigger:** any other `VetoReason::Custom(...)` site lands in the codebase.
- **Scope:** one line in `xianvec-core::trading.rs` enum + serde rename + cascade through any exhaustive `match VetoReason {...}` — `xianvec-risk::rules::take_profit_rr` switches off `Custom("rr_too_low")`.
- **Blocking:** non-blocking; quality-of-enum.
