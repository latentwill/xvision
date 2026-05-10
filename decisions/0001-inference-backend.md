# ADR 0001 — Inference backend + extraction precision

> **2026-05-10:** Project renamed `xianvec` → `xvision`. References below reflect the post-rename name; project history prior to this date used `xianvec`.

**Date:** 2026-05-03
**Status:** Accepted (revised 2026-05-07 per ADR 0011)
**Phase:** 0.2 / 0.3 prerequisite

> **2026-05-07 revision:** Per ADR 0011, CV substrate moved to xvision-play.
> This ADR retains candle as a local-inference option for the Trader, but
> steering-hook flexibility is no longer the load-bearing justification.
> The MLX extraction path and `repeng` references below are historical
> record — the CV pipeline they describe lives in xvision-play. ADR 0007
> covers the surviving Trader-only inference throughput considerations.

## Context

The implementation plan (§0.2, §4.2) specifies:

- Runtime inference: candle Q4 (no-thinking variant of Qwen3-class model).
- Vector extraction: Python `repeng` against `transformers`-loaded model at fp16.

Local development happens on an **Apple M4 Max with 36 GB unified memory**. Two
constraints emerged when concretizing the model choice:

1. The plan's nominal `Qwen3.6-27B` is aspirational — no such checkpoint exists.
   Closest production-grade dense models in the Qwen3 family (Jan 2026): Qwen3-8B,
   Qwen3-14B, Qwen3-32B, plus the Qwen3-30B-A3B MoE variants.
2. `repeng` against Qwen3-32B at bf16 requires ~64 GB of weights plus activation
   memory — exceeds the 36 GB budget by ~2×, even with mmap. bitsandbytes int8/int4
   is CUDA-only and not viable on Apple Silicon.

## Decision

**Use Qwen3-32B as the production model — the candidate that best matches the
plan's intent.** Run both extraction and inference at 4-bit locally:

| Stage | Backend | Checkpoint | On-disk | Phase |
|---|---|---|---|---|
| Extraction | MLX (Apple-native) | `mlx-community/Qwen3-32B-4bit` | ~37 GB | 0.3, 4.x |
| Inference (Q4) | candle 0.10 `quantized_qwen3` | `bartowski/Qwen_Qwen3-32B-GGUF` Q4_K_M | ~20 GB | 0.2, 3.x, 4.4, 9.x |

The MLX route preserves the plan's "Python is a build tool, Rust is the runtime"
property — extraction stays in Python (re-implementing repeng's mean-difference /
PCA against MLX hooks; the algorithm itself is ~50 lines), while the production
binary remains a candle process with no Python in its tree.

## Why not …

- **bf16 transformers extraction.** Won't fit in 36 GB. Forcing it requires
  rented GPU now, before the spike has even validated the methodology. We chose
  velocity-over-purity for Phase 0.3 and accept the 4-bit-quantization-noise
  tradeoff in extraction quality.
- **bitsandbytes int4/int8.** CUDA-only path; broken on Apple Silicon.
- **In-candle extraction (skip Python entirely).** Architecturally cleaner — the
  same hooks used for steering would capture activations during contrast-pair
  forwards, mean-diff in Rust. Deferred to a possible Phase 4 follow-up if the
  Python pathway proves friction-prone. Not adopted in Phase 0 because it's a
  larger plan deviation than scope warrants here.
- **Smaller model (Qwen3-8B / 14B).** Spike on a non-production model would
  require a second validation pass on the production model later. Direct 32B
  spike at Q4 closes the question once.
- **Qwen3-30B-A3B MoE.** Steering-vector behavior interacts with expert routing
  in ways the plan does not analyze. Dense 32B keeps the experimental surface
  unambiguous.

## Spike runtime split (added 2026-05-03 during Phase 0.3 wiring)

After scaffolding the candle Q4 GGUF engine, we discovered that
`candle_transformers::models::quantized_qwen3::ModelWeights` does not expose
per-layer residual mutation (the layer loop is private — `for layer in &mut
self.layers { h = layer.forward(...)?; }`). Adding a `LayerHook` requires
**vendoring the file plus its private deps** — `super::with_tracing::QMatMul`,
`crate::quantized_nn::RmsNorm`, `crate::utils::repeat_kv`, the flash-attention
CPU path, and the KV-cache impls. ~1000 LOC pulled in.

That vendoring belongs in Phase 4 where Tier 1 fix #5 (gate at the action
choice point) is implemented anyway. For the **Phase 0.3 spike specifically**,
we run extraction *and* steering in Python/MLX:

| Question | Phase | Backend | Why this split |
|---|---|---|---|
| "Do disposition vectors steer this model at 4-bit?" | 0.3 spike | MLX (Python) | Hooks are first-class — monkey-patch `model.layers[L].__call__`. The hypothesis question. |
| "Does the candle Q4 runtime respond to the same vector?" | 4.3 hard gate | candle (vendored qwen3_steered.rs) | Already a planned gate (re-runs spike's directional-match through runtime path). The engineering question. |

The Phase 4.3 hard gate was already specified to "re-run the spike's directional-
match criterion against the loaded vector through the runtime path" (Tier 2 fix
#9 / plan §4.3). This split makes that gate load-bearing rather than redundant
and lets the Phase 0.3 question be answered without paying the candle-vendor
cost upfront.

## Consequences

- Extraction quality is dampened by Q4 quantization noise on activations.
  The spike's directional-match criterion (≥80% on holdout) is the gate; if Q4
  extraction can't clear it, we revisit with a rented GPU.
- The Phase 4.3 hard gate (re-run spike's directional-match through the runtime
  candle path) becomes redundant with the Phase 0.3 spike *if* both Phase 0.3 and
  Phase 4.3 use the same Q4 GGUF. This is fine — one of the two is now a regression
  test rather than a fresh validation.
- Phase 9.3's "headline run on rented GPU at Q8_0/bf16" remains unchanged. Local
  Q4 is for development and the v1 demo; the headline number for the report
  comes from rented hardware as planned.

## Phase 3 addendum (2026-05-03): candle Q4 throughput on M4 Metal

After wiring `xvision-trader::run_trader` against the same `Qwen3Engine` from
Phase 0.2, end-to-end smoke surfaced a sharp throughput cliff on M4 Max with the
default `candle-core/metal` backend:

```
smoke-qwen3 (release, default features = ["metal"])
  prompt_tokens=20  completion_tokens=2
  prompt_dt_ms=16165   →  ~1.2 tok/s prefill
  completion_dt_ms=3138 →  ~0.64 tok/s decode
```

A 600-token Trader prompt + 384-token decode therefore costs ~17 min wall — the
`smoke-trader` binary is correct but unusable interactively at this rate.

The Q4_K_M Metal kernels in `candle_transformers::models::quantized_qwen3` do
not currently match `llama.cpp` throughput on the same GGUF and the same
hardware (where the same model runs ~30+ tok/s). This is a candle limitation,
not a Phase 3 wiring bug — the smoke pipeline produces correct output, just
slowly.

**Implications:**
- Phase 3 acceptance is met by unit-test evidence (29 trader tests, including
  the synthetic 95% / 99% parse-rate gate) plus a verified end-to-end
  engine→parse round-trip on a tiny prompt. Live large-prompt timing is
  recorded here, not asserted.
- Phase 4 vector-application work continues against this engine; per-token
  cost is irrelevant for hook-correctness validation (single-token forward
  passes are the unit of work).
- **Phase 9 (forward paper / live) cannot use the candle Q4 path at this
  throughput.** The full route analysis (mlx-rs vs vendoring candle qwen3
  vs HTTP backends, with the steering-hook constraint front-and-center) lives
  in `decisions/0007-inference-throughput-routes.md`.

The Phase 4.3 hard gate (vectors must steer the candle runtime equivalently to
the MLX spike path) is not affected — it depends on hook correctness, not
throughput.

## Qwen3-Next addendum (2026-05-05): footnote in §Context line 17 is now stale

The §Context note "The plan's nominal `Qwen3.6-27B` is aspirational — no
such checkpoint exists" was true on 2026-05-03 but is no longer true:
Qwen3.6 dropped 2026-04-16 (which we missed at the time of writing) and
weights are now on disk. A fear-greed vector was extracted against
Qwen3.6 fp16 on 2026-05-05.

This **does not change the production decision in this ADR.** Production
remains Qwen3-32B, candle Q4_K_M, vendored quantized_qwen3 with `LayerHook`.
Qwen3.6 is a hybrid Gated DeltaNet architecture not implemented in
candle, and switching the runtime path is out of Phase 4 scope.

The Qwen3-Next runtime question (cvec on hybrid + candle port options + when
to migrate) is captured in `decisions/0009-qwen3-next-runtime-options.md`.
That ADR is open and gated on F27 (Python eval) + F28 (llama.cpp cvec
spike) before it can ratify a new runtime path.

## References

- Implementation plan §0.2, §0.3, §3.1, §3.3, §4.2, §4.3, §9.3
- `decisions/0002-spike-validation.md` (records spike outcome)
- `decisions/0007-inference-throughput-routes.md` (Qwen3-32B throughput options)
- `decisions/0009-qwen3-next-runtime-options.md` (forward-looking — Qwen3.6 hybrid runtime)
