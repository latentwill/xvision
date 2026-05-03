# ADR 0001 — Inference backend + extraction precision

**Date:** 2026-05-03
**Status:** Accepted
**Phase:** 0.2 / 0.3 prerequisite

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

## References

- Implementation plan §0.2, §0.3, §4.2, §4.3, §9.3
- `decisions/0002-spike-validation.md` (records spike outcome)
