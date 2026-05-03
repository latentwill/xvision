# XIANVEC — Steering Vector Architecture & Forward Thinking

Companion document to `architecture.md`. This file consolidates the design conversation around control vectors, conditional steering, the Glamin-derived patterns, telemetry, the Rust-from-day-one decision, and the offline Python extraction boundary. Where this conflicts with `architecture.md`, the canonical doc wins for hackathon-scope decisions; this doc is the forward-thinking sibling.

> **Reading note (added 2026-05-03).** This document predates the v1 scope cuts and the Byreal → Vertex vendor correction. References to `byReal` throughout (especially in "Speed Layer (Mercury)" and the planetary map) reflect the original execution-venue assumption; the architectural reasoning around speed layers, async substrates, and pre-signed transactions applies equally to **Vertex on Mantle** (`ClientMode::MantleProd`) — substitute the venue when reading. Similarly, references to `crates/lodestar-*` describe the v2 split that v1 collapses into a single `crates/xianvec-*` tree (`architecture.md` §10.2). The Karpathy-loop / probe-gated / single-axis-first prescription in this doc is the *correct* prescription that the v1 implementation-plan now enforces explicitly.

## Thesis

Steering vectors are an inference-time finishing pass on a system that must already work without them. The trading agent's success will be determined by data quality, tool correctness, hard-rule risk management, and execution speed. Vectors enter only after the unsteered baseline behaves coherently, are added one at a time, are probe-gated rather than always-on, and are retained only if they survive evaluation across normal and stressed market regimes.

The Karpathy-style self-improvement loop — letting the model discover useful steering directions that may not have English-language names — is a defensible long-term research direction. It pays off only if the validation harness is in place before the loop is. **The harness is the moat. The vectors are the dividend.**

## What Vectors Do, and Don't

Established by Mitra's 2026 field guide and consistent with the broader steering literature:

Vectors reliably move *stance* — refusal/compliance, sentiment/tone, conciseness, uncertainty expression. They do not move *skill* — factual accuracy, multi-step reasoning, specific knowledge injection. Effects are prompt-dependent with high variance. Effects fade after ~300–500 tokens of generation. Vectors that look fine on standard evaluation can fail under adversarial conditions — exactly the regime in which a trader most needs them.

Implication: vectors can shape *posture during reasoning*. They cannot shape *quality of reasoning*. Posture-shaping has value, but it is bounded — and that bound is what makes them a polish layer, not a source of edge.

## Order of Operations

The default mistake is to start with vectors. Don't.

**Phase 0 — Foundation (no LLM steering).** Cargo workspace skeleton. Candle smoke test on Qwen3.6-27B Q4 with hidden-state hooks verified. Data pipeline, valuation primitives, regime classifier, slippage model, position-sizing function. Pre-signed transaction templates, bundle submission on Mantle/byReal, kill-switch and drawdown circuit breakers as **hard rules outside the LLM**. Hard rules are not vectors; they are not overridable by the model. Discipline the model can ignore is not discipline.

**Phase 1 — Baseline agent.** Tight system prompt, Phase 0 tools, rule-based constraints (circle-of-competence filter, position-size cap, cooldown after losses). Backtest until performance is at minimum *not loss-making* across multiple regimes. No vectors yet. This is the unsteered control that all later evaluation pairs against.

**Phase 2 — Identify residual failures.** Catalog the behavioral failures that survive Phase 1. Most will be addressable by adding a tool or tightening a constraint. Do those fixes first. The residue — failures that are genuinely about model stance, not knowledge or rules — is the candidate set for vector work.

**Phase 3 — One vector.** The strongest single use case is **probe-gated narrative-skepticism** (or its dual: probe-gated calibration/humility). Train an activation probe to detect "confabulating-thesis" patterns. Gate a humility/skepticism vector that fires only when the probe trips. Validate against the unsteered baseline. Keep only if the lift is real and survives stressed regimes.

**Phase 4 — Validation harness.** Before adding a second vector, build the evaluation infrastructure that everything subsequent will rely on. The harness becomes the moat.

**Phase 5 — Self-improvement loop.** Once the harness can catch Goodhart, run iterative vector discovery on top of it. The Rust orchestrator invokes the Python extraction utility with model-generated contrast specs; the harness runs the resulting vectors against the probe corpus; survivors enter the geometry. Some discovered directions will be nameable, some won't. Both are equally welcome if they survive validation.

## Vector Strategy (When Vectors Are In Play)

**Stance over personality.** Personality vectors are entangled bundles. Stance vectors satisfy Mitra's one-dimension rule by construction. The four disposition axes already locked in `architecture.md` §7.1 (Conviction, Patience, Risk appetite, Trend disposition) are the v1 set. Forward candidates worth adding once v1 is validated: narrative skepticism, calibration/uncertainty, convexity preference, stand-down disposition (refusal repurposed as a veto on ambiguous setups).

**Layer placement.** Qwen3.6-27B places stance representations in late layers (~75% depth). Probe a window centred there per vector. There is no global "best layer" — different stances live cleanest at different layers, and that's a feature for composition (Weij et al. show injection at distinct layers reduces interference relative to stacking).

**Alpha tuning.** Alpha is non-monotonic and the single most important hyperparameter. Binary search from α = 1.0; double if no effect, halve if degradation. 20–30 test prompts per setting. Posture-style vectors typically land in [1.0, 2.5].

**Contrast pairs.** 60–80 templated pairs per vector, differing on **exactly one dimension**. Same template, same topic distribution, only behavior changes.

**Composition.** Cap at 2–3 simultaneously active vectors. Inject at distinct layers (different `candle` hooks) rather than stacking at one. Stacking at a single layer only works for genuinely orthogonal axes.

**Gating (CAST + entropy).** Default to conditional steering. Two complementary mechanisms:

The lightweight gate, v1: decision-token entropy at the emit layer (`buy`/`sell`/`flat`). Implemented as a candle hook that computes softmax entropy over the small decision-token set and dampens magnitude proportionally. Low entropy = tight corridor = full magnitude. High entropy = wide corridor = dampen or skip.

The full gate, post-v1: project the live hidden state onto a condition vector; only steer when the condition matches (CAST). The Rust gating crate exposes this as a configurable strategy — entropy or CAST — chosen per vector.

**Adaptive strength.** PID-style control over alpha within hand-set bounds, implemented in `crates/lodestar-gating/`. Hard floor on critical vectors (skepticism, stand-down) so the controller cannot zero them out. Re-inject at every decision boundary to defeat the 300–500 token decay.

## Glamin-Derived Patterns (the bits we are copying)

Glamin is itself unfinished and Fortran/C-cored, so we are not adopting the project — we are adopting its design vocabulary, rebuilt in Rust. Below are the patterns we are bringing, what they mean for xianvec, and what we leave behind.

### Bring: Corridors as Decision Boundaries

A corridor is a parametrized region between two (or more) anchor points in vector space, with explicit `width`, `risk_profile`, and a firing rule. For xianvec, each major decision (long vs flat, scale-in vs wait, act vs stand-down) becomes a corridor. Width encodes how confident we have to be before crossing. Risk profile encodes what fires when we're deep in the corridor (i.e. ambiguous): kill-switch, human-in-the-loop hold, reduced size, or just dampened vector magnitude.

Implementation in `crates/lodestar-geometry/`:

```rust
pub struct Corridor<L: Layer, M: Model> {
    pub anchors: (Mint<L, M>, Mint<L, M>),
    pub width: f32,
    pub risk_profile: RiskProfile,
    pub firing_rule: FiringRule,
    pub manifest: Manifest<L, M>,
}
```

Type parameters carry the layer and model the corridor was derived for. Cross-layer or cross-model misuse is a compile error.

### Bring: Contract Layer

Every vector and corridor write carries a manifest hash: `(model_version, embedder_version, layer_id, contrast_pair_set_hash, alpha_curve_hash, derivation_timestamp)`. Mismatched writes are rejected at the storage boundary.

In Rust, contracts express naturally as types — `Vector<Layer22, Qwen36_27B>` cannot be stored in a slot expecting `Vector<Layer24, _>`. The `crates/lodestar-contracts/` crate defines the manifest types; downstream crates are generic over them. The compile-time enforcement is the type-system-as-discipline payoff that motivated the Rust-from-day-one choice.

For the parts where compile-time enforcement isn't possible (loading from disk, RPC boundaries, the Python subprocess call), the manifest sidecar JSON is validated at runtime via `serde` + `garde`.

### Bring: Boundary Probes

Curated edge-case inputs paired with expected decisions, stored as first-class artifacts in `data/probes/`. Probes are versioned, replayable, and diffable across changes.

For xianvec, probes are: ambiguous regime transitions, low-liquidity setups, hardest historical decisions, flash-crash conditions, regulatory edge cases. They get re-run on every model change, vector update, prompt change, and tool change, by the harness in `crates/xianvec-harness/`. Output is a structured delta report (decisions flipped, corridor drift, capability-floor delta) emitted as JSON for offline analysis and as OTel spans for live observability.

This is the cleanest articulation of "the harness is the moat." Phase 4 of the order-of-operations *is* building the boundary-probe corpus and runner. Without it, the Phase 5 self-improvement loop has nothing to validate against.

### Bring: Document / Geometry Separation

Two distinct vector spaces with explicit, contracted bridges between them. Document space (`crates/xianvec-data/`) holds market data, news, filings, on-chain events. Geometry space (`crates/lodestar-geometry/` + `crates/lodestar-substrate/`) holds steering directions, decision corridors, regime classifiers, probe corpora. Cross-transforms are explicit functions with their own contracts.

The crate boundary is the discipline. A function in `xianvec-trader/` can depend on `xianvec-data/` for inputs and `lodestar-geometry/` for steering, but `xianvec-data/` and `lodestar-geometry/` cannot depend on each other. Cargo's dependency graph prevents the silent contamination bug at compile time.

### Bring: Async-First Vector Storage

The default vector library is synchronous. For a real-time agent on byReal where the decision loop is latency-sensitive, blocking on index updates is unworkable. The async wrapper in `crates/lodestar-substrate/` provides:

- Non-blocking add/search returning `tokio::task::JoinHandle`
- Snapshot read semantics via `arc-swap` (a query started at time T sees a consistent index state through completion, even while writes land)
- Worker pool with backpressure
- Priority queue (gating reads beat probe re-evaluations beat batch maintenance)
- Cancellation as first-class (drop the handle, the worker abandons)

Built as a thin wrapper around `faiss-rs` HNSW indexes. ~300 lines of `tokio` + `arc-swap`.

### Bring: FAISS File Format Compatibility

FAISS is the lingua franca of approximate nearest neighbor search. The `.index` binary format is documented; the distance kernels are well-specified. Writing FAISS-compatible files means our vector data interchanges freely with the broader ecosystem — the Python extraction utility writes it, the Rust runtime reads it, and any FAISS-aware analysis tooling can consume the same files for offline inspection.

`faiss-rs` provides idiomatic Rust bindings. HNSW is the pragmatic index choice at our scale (probes in the thousands, vectors in the hundreds, activation snapshots maybe in the tens of thousands).

### Leave: Fortran/C Performance Maximalism

Glamin pays for cycles by giving up developer ergonomics. Modern Rust gets within a small factor of C performance with vastly better tooling. At our corpus sizes the gap is invisible.

### Leave: Hand-Written SIMD/AVX Kernels

`faiss-rs` and `ndarray` route through vendor BLAS. Hand-tuned kernels are unnecessary at our scale.

### Leave: The Custom YAML Geometry-Spec DSL

Express corridors and probes in code or in our existing YAML config (loaded into typed `serde` structs). Glamin's spec authoring layer is interesting research, unnecessary overhead.

### Leave: "Everything Is Geometry" Maximalism

Hard rules — kill switch, position sizing, drawdown limits — stay discrete, non-overridable code in `crates/xianvec-risk/` outside the manifold entirely. Mars stays Mars. Trying to express a circuit breaker as a corridor with very narrow width is the wrong abstraction.

### Leave: The Unfinished Geometric-Logic DSL

We write the actual gating logic ourselves in `crates/lodestar-gating/`: `distance_to_corridor()`, `gate()`, composition over multiple active corridors. ~500 lines of careful Rust, rather than waiting on Glamin's geometric-logic layer to ship.

## Layer Introspection (the lamp Saturn carries)

Output text alone is an underspecified diagnostic for steering work. A vector that "doesn't seem to do anything" might be unapplied, applied at the wrong layer, washed out by subsequent layers, attenuated by quantization, or pulling the residual stream in the right direction with effects only visible deeper in the network. Without layer-level visibility you cannot distinguish these failure modes from each other.

`lodestar-introspect` provides this visibility — opt-in by composition, zero overhead when not installed. When you want diagnostics (the validation spike, probe runs, debugging, magnitude sweeps), wrap the steering hook in an `IntrospectionHook` and drain a structured report after generation.

**Captures, configurable per run:**

- Per-layer residual stream norms, pre and post hook
- Per-layer activation diff (`||post - pre||`)
- Vector–residual cosine similarity at each hooked layer
- **Logit lens** at every captured layer — apply final layer norm + unembedding to the residual stream as if generation stopped there; reveals how steering propagates through subsequent layers
- Decision-token logits, probabilities, entropy at the gate point (the position immediately after `"action": "`)
- Magnitude-sweep diagnostics confirming Mitra's non-monotonicity on this specific vector
- Per-layer ablation finding the right injection layer empirically
- Multi-vector composition diagnostics catching interference invisible from output alone

**Required for:** Phase 0.3 spike validation (extends pass criteria with logit-lens shift + non-monotonic magnitude sweep), Phase 4.4 steering hook QA, Phase 8.5 probe runner (every probe captures its layer signature for Goodhart-resistance).

**Optional for:** any backtest, forward paper, or live run. Off by default in production; on by default in dev and CI.

The output is structured JSON consumed by `notebooks/inspect_vector.py` for multi-panel plots — what you look at when you want to know whether your vector works. This is the diagnostic layer the cvidialog-style "vector seems to do something but you can't tell what or why" problem actually needs.

## Telemetry & Observability (Mercury's Diary)

A self-improvement loop without traces is just drift wearing a confidence interval. Tracing is not optional — it's the substrate that makes the whole loop honest.

### Stack

The `tracing` crate (Tokio team) for structured spans. `tracing-opentelemetry` to bridge into OTel. `opentelemetry-otlp` to ship spans over OTLP. `tracing-subscriber` for the global subscriber configuration.

Backend: **self-hosted Langfuse** as primary — open source, LLM-native (token counts, cost rollups, prompt diffs as first-class), Docker compose deployment (Postgres + Clickhouse), free forever, no metering. Honourable mentions: Phoenix (Arize) if the validation harness grows into a deeper eval surface; Honeycomb for general serving APM; Logfire-via-OTLP as fallback.

### Required Span Coverage

Every Stage 1 (Intern) and Stage 2 (Trader) call emits spans tagged with the OpenTelemetry GenAI semantic conventions (`gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`) plus xianvec-specific attributes:

- `xianvec.run_id`, `xianvec.setup_id`, `xianvec.stage`
- `xianvec.vectors.enabled`, `xianvec.vectors.config_hash`, `xianvec.vectors.magnitudes`
- `xianvec.gating.entropy`, `xianvec.gating.applied_magnitude`
- `xianvec.regime.classification`, `xianvec.regime.confidence`
- Tool calls as nested spans with input/output payloads
- Vector reads as nested spans with corridor IDs and distances
- Decision outputs and parse success/failure

The Python extraction utility also emits OTel spans (via `opentelemetry-python`) so subprocess invocations from the Rust orchestrator appear in the same trace tree as the calls that triggered them.

## The Karpathy Self-Improvement Bet

**The plausible case.** Language is a compressed view of cognition. SAEs find behaviorally meaningful features without English labels. Outcome-based vector discovery — finding directions where profitable-decision activations differ from unprofitable ones — is real and doable. YaPO learns sparse vectors without contrast pairs at all.

**The killing case.** Goodhart. Optimize directly on PnL and you find vectors that exploit backtest artifacts. The vector looks brilliant in-sample, does nothing or hurts out-of-sample.

**The shape of the bet that survives.** Validation infrastructure that's at least as good as the optimizer. True out-of-sample regimes (held-out structure, not just held-out time). Adversarial test sets. Capability floors held. Position sizing tied to validation breadth. Probes that verify discovered vectors aren't exploiting narrow representational artifacts. With this harness, the self-improvement loop compounds. Without it, it compounds the wrong thing faster.

This is why Phase 5 follows Phase 4. The order matters more than any individual technique.

**Implementation shape.** The Rust orchestrator (`crates/xianvec-harness/`) generates contrast specs, writes them to disk as JSON, invokes the Python extractor as a subprocess, reads back the resulting FAISS-compatible vector with its contract manifest, validates the manifest against the current runtime configuration, and runs the boundary probe corpus against the new geometry. Pass/fail is decided in Rust based on probe results, capability-floor delta, and regime-stratified evaluation. Survivors enter the active geometry; failures are logged with provenance for post-hoc analysis.

The Python subprocess is a tool the agent calls. Its lifecycle is owned by the Rust orchestrator. A crashed extraction does not bring down the agent — it fails one extraction attempt and surfaces the error in the next probe report.

## Speed Layer (Mercury — the byReal piece)

Independent of vector work and worth pursuing in parallel:

*Two-tier inference.* Tiny classifier (or rules) in the hot path, full LLM reasoning only when the classifier flags ambiguity. Most ticks should not wake the LLM.

*Pre-signed transaction templates.* A quiver of partially-formed transactions ready to fill and fire. The "decision" becomes parameter selection.

*Local state mirroring.* Subscribe to byReal state, mirror in-process. The agent reads from RAM, not RPC.

*KV-cache the system scaffold and steering vectors.* Only the market-context delta should change per call.

*Bundle submission* on the execution path with re-bundling logic for failed inclusion.

## Evaluation Discipline

(Reference: `architecture.md` §9 for the canonical metrics — Δ-Sharpe, max drawdown, profit factor, decision divergence, anti-overfitting gate.)

The Glamin-derived additions to the eval surface, all implemented in `crates/xianvec-harness/`:

- **Boundary probe pass rate.** What percentage of probes produce the expected decision under current vector configuration?
- **Decision-flip count vs prior version.** How many probes change decision between version N and N+1? High flip count under nominally similar configurations is a warning sign.
- **Corridor drift.** How far have decision boundaries moved since baseline? Quantified as average activation-space distance between corresponding mints.
- **Capability floor delta.** No greater than 2-point degradation on a held-out reasoning battery. Tool-call JSON validity preserved. Numerical reasoning intact.
- **Regime-stratified probe results.** Same probes run under different volatility/liquidity/correlation conditions.

Treat *behavior* as the artifact, not the vector. Vectors are non-identifiable; many produce equivalent effects. Validate the behavior, then keep whichever vector produces it most cleanly.

## Open Questions

- Does candle's Qwen-3 quantization story hold up in Phase 0 validation, or does the project fall back to `llama-cpp-rs`?
- Which of the four disposition axes will produce the highest validated lift in practice? Resolved only by running the harness.
- Is decision-token-entropy gating (v1) sufficient, or does the agent need full CAST projection-based gating to handle multi-vector composition cleanly?
- Can outcome-based vector discovery (Phase 5) find directions that survive regime change, or will discovered vectors be regime-specific by construction?
- What is the latency cost of CAST gating in the byReal hot path? May force gated vectors to live only in the deliberation tier, not the reactive tier.
- At what frequency should re-injection occur — every decision boundary, every N tokens, or driven by an entropy/uncertainty signal?
- Self-hosted Langfuse vs cloud Langfuse vs Logfire-via-OTLP: which has lowest operational burden for the team's actual workload?

## The Lodestar Boundary

The inference and control-vector substrate is split off as `crates/lodestar/` — domain-agnostic, designed to be lifted into a sibling project (EditEngage, character/voice work, any other domain) without modification. lodestar provides: candle-backed inference with hidden-state hooks, FAISS-compatible async vector storage, contracts and manifests, generic Mint/Corridor/Probe primitives parametrized over domain types, entropy + CAST + PID gating, optional layer introspection, OTel span schema, and a generic diagnostic CLI.

xianvec consumes lodestar and adds the trading-specific layer: schemas (Action / Direction / AssetSymbol / Regime), the four disposition axes specialized for trading, the Δ-Sharpe eval framework, the regime-stratified anti-overfit gate, the byReal / Alpaca executors, ERC-8004 identity, the trading probe corpus.

The boundary lives in-tree for now (no separate repository) but is enforced as if it were external: `cargo deny` audits the dependency graph and CI fails on any lodestar-to-xianvec dependency. When a second consumer materializes, lodestar lifts via `git mv` plus a path-to-git swap in xianvec's `Cargo.toml`. The mechanical cost of the lift is small precisely because the discipline is maintained from day one.

A useful test of lodestar's API: imagine writing EditEngage against it. Voice-steering vectors. Probes that ask "does this paragraph hold character X's voice?" Corridors gating in-voice continuation versus drift. The gating strategies, the introspection diagnostics, the contract layer — all identical to trading consumption. Only the schemas change. If a piece of lodestar can't survive that thought experiment unchanged, it has trading specifics leaking and belongs in xianvec.

## Planetary Map

*Sun* — the agent itself, the synthesizing center where context, geometry, tools, rules, and execution collapse into a single act of decision. Not the vectors. Not the tools. Not the harness. The integration moment.

*Moon* — recent state, drawdown counter, position book, last-N decisions. Reflective, fast-changing, always read but rarely written. Lives in `arc-swap`-protected state — fast reads, atomic updates.

*Mercury* — speed and communication. byReal execution path, pre-signed templates, two-tier inference, KV-caching. The fast quiet messengers. Also: telemetry — Mercury's diary, the written record that lets the slow planets audit later.

*Mars* — execution and discipline. Hard rules: position sizing, stop-loss, drawdown circuit breakers, kill switch. `crates/xianvec-risk/`, outside the LLM, non-overridable. The warrior does not negotiate with geometry.

*Jupiter* — expansion. Tool quality. Data breadth. Reasoning surface area. DCF, tail-risk samplers, regime classifiers. Big, generous, broad in scope. Not in the geometry; consulted by the agent that lives in the geometry.

*Saturn* — structure and constraint. The validation harness. The contract layer. The Cargo workspace boundaries themselves. The unglamorous load-bearing infrastructure that everything else depends on. Built first, always. Rust's type system is Saturn's natural ally.

*Neptune* — the model's representational space. Where vectors and corridors live. Useful but slippery; not where edge originates. Always answering to Saturn's contracts and Mars's hard rules.

*Pluto* — transformation. The Karpathy self-improvement loop. The Rust orchestrator calling the Python extractor. Operates only with Saturn underwriting.

Build order follows the planets: Saturn first, Mars and Mercury in parallel, Jupiter through tool work, Neptune as a finishing pass, Pluto as long-horizon research. The Sun, the Moon, and Mercury-as-diary are present throughout — they are not phases but conditions of the work.

☯️

---

*Document version: 2026-05-02. Companion to `architecture.md`. Captures the May 2026 design conversation around Mitra's field guide, Glamin pattern adoption, the Rust-from-day-one decision, and the offline Python extraction boundary.*
