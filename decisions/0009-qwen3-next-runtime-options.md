# 0009 — Qwen3-Next runtime path: cvec + hybrid-arch port options

**Date:** 2026-05-05
**Status:** Open. Decision deferred until F27 (Python eval on Qwen3.6) lands.
**Phase:** Forward-looking — Phase 4.x continues on the Qwen3-32B path
established in ADR 0001 and validated in ADR 0002. This ADR governs the
*next* runtime decision, triggered when production wants Qwen3-Next-class
weights.

## Context

ADR 0001 froze production on Qwen3-32B (dense, pure transformer) at Q4_K_M
in vendored candle, with `LayerHook` as the steering surface. ADR 0002
confirmed steering works at that quantization (1.17 score-swing on the toy
axis at L42, MLX side; Phase 4.3 hard gate re-runs this on the candle
runtime). That decision is intact and is not what this ADR revisits.

Two things have shifted since 2026-05-03 that make a future-state look
warranted:

1. **A real Qwen3.6-27B / Qwen3-Next checkpoint is now on disk** and
   `tools/extract_vectors/` extracted a fear-greed vector against it at fp16
   (this morning). The "Qwen3.6-27B is aspirational" footnote in ADR 0001
   §Context line 17 is no longer true — Qwen3.6 dropped 2026-04-16.
2. **Qwen3-Next/3.5/3.6 is a hybrid architecture**: 3 of every 4 layers are
   Gated DeltaNet (linear attention with a fast-weight memory + α/β gates
   + delta rule), every 4th layer is conventional softmax attention. The
   architecture string is `qwen35` and is not implemented in
   `candle-transformers::models::quantized_qwen3` (which targets pure-Qwen3).

Two empirical questions follow from those shifts. They cleanly separate by
runtime:

| Q | What it tests | Where it runs | Status |
|---|---|---|---|
| Q1 | *Do steering vectors meaningfully shift Qwen3.6 outputs at all?* | Python `transformers` / repeng with PR #73 fix | F27 — open |
| Q2 | *Does a runtime that supports the hybrid architecture apply the cvec correctly to the residual stream of DeltaNet layers, not just attention layers?* | llama.cpp `--control-vector` against Qwen3.6 GGUF | F28 — open, conditional on Q1 PASS |

If Q1 fails (vectors don't transfer to hybrid), the rest of this ADR is
moot — production stays on Qwen3-32B, possibly indefinitely. If Q1 passes
but Q2 fails (hybrid runtime is wrong about cvec semantics on DeltaNet
layers), the path is harder than this ADR assumes. Only if both pass do
the runtime options below become live.

## What changed in the cost estimate

Earlier conversation framed a candle port of the hybrid architecture as
"weeks of work writing SSM blocks + tracking upstream candle." That was
anchored on writing a pure Mamba SSM forward pass from scratch. Re-grounded
against actual llama.cpp source (see citations), the picture is meaningfully
different:

- **Control vector mechanism in llama.cpp is one `ggml_add` per layer**
  (`src/llama-adapter.cpp` lines 22–28). Architecture-agnostic. Hooked into
  qwen3next via `cur = build_cvec(cur, il);` at line 164 of
  `src/models/qwen3next.cpp`, *after* the FFN residual add and before the
  next layer's input norm. Porting this hook point to candle is ~10 lines
  around `xianvec_inference::vendor_qwen3::forward_with_hooks` — already
  the same shape as our existing `LayerHook::apply()` contract.
- **The DeltaNet block itself** is `build_layer_attn_linear()` in
  `src/models/qwen3next.cpp` lines 367–550 (~184 LoC) plus the base
  algorithm in `src/models/delta-net-base.cpp` (~590 LoC across three
  variants).
- **Custom kernels** (`ggml_ssm_conv`, `ggml_gated_delta_net`) are required
  for the *fused* DeltaNet variant. The **chunked variant** (also in
  `delta-net-base.cpp`) avoids them — it composes from `cumsum`,
  `solve_tri`, and standard matmul/norm ops, all of which candle already
  has. Picking chunked over fused is a strategic shortcut that removes the
  hardest part of the port.
- **Candle status:** no in-flight qwen3-next / qwen3.5 / qwen3.6 / DeltaNet
  work in github.com/huggingface/candle as of 2026-05-05. Candle has
  Mamba/Mamba2 examples but they are different enough from DeltaNet that
  they are reference, not bootstrap.

Revised port estimate: **1–2 weeks** for inference + control vectors on the
chunked variant, CPU-or-naive-Metal, no kernel optimization. ~250–350 lines
of Rust for the DeltaNet block, ~80–100 for tensor/hparam loading, ~10 for
the cvec hook — total roughly the same shape as the original
`vendor_qwen3.rs` work (ADR 0001 §"Spike runtime split", ~1000 LOC).

## Options when Q1 + Q2 both pass

| Route | Throughput | Steering hook | Effort | Risk |
|---|---|---|---|---|
| α. Continue on Qwen3-32B; do not adopt Qwen3.6 | already met | ✓ native (vendored candle) | 0 | Forfeits Qwen3.6 capability gains |
| β. Qwen3.6 + Python `transformers` runtime | slow (2–10 tok/s on M4) | ✓ via repeng PR #73 | days | Abandons the Rust-first ADR 0001 thesis |
| γ. Qwen3.6 + llama.cpp via `llama-cpp-rs` / `llama-cpp-2` | fast (~30 tok/s on M4) | ✓ via `--control-vector`; one fixed hook point | 3–5 days | Locked into llama.cpp's hook architecture; novel hook strategies require C++ patches or fork |
| δ. Qwen3.6 + candle DeltaNet port (chunked variant) | unknown until benched (target ≥10 tok/s) | ✓ native (extends current `LayerHook`) | 1–2 weeks | Maintenance burden; flash-attention-on-Metal still unproven for DeltaNet path |

α is the no-op. β contradicts the original Rust-first thesis (ADR 0001
§Context: "Python is a build tool, Rust is the runtime"). γ and δ are the
real choice.

The discriminating factor between γ and δ is *hook flexibility*, not
throughput. llama.cpp's cvec is locked to one residual-stream point per
layer (post-FFN, pre-next-layer-norm). For the current Conviction / fear-
greed application that's exactly the point we want; γ costs nothing in
capability today. But the project's identity is "control-vector-optimized
agentic trading" — which implies the hook strategy may evolve (perpendicular
projection against a refusal direction, attention-only vs DeltaNet-only
steering, dynamic α scaling on activation magnitude). Each of those is
trivial to add inside `vendor_qwen3::forward_with_hooks` and a fork-or-patch
inside llama.cpp.

## Decision

**Defer the runtime choice until F27 (Python eval) and F28 (llama.cpp cvec
spike) are answered.** Both are bounded probes (1–3 days each) and either
one failing collapses the option space.

If both pass, the working assumption — to be ratified by a follow-up ADR
0010 once benchmarks are in hand — is **route δ (candle DeltaNet port,
chunked variant)**, on the architectural grounds that:

1. ADR 0001's Rust-first / hook-flexibility thesis still applies. xianvec's
   v1 demo is built around per-layer hook control; route γ closes that off.
2. The cost estimate is now bounded (~1–2 weeks, not "weeks-to-months").
   The earlier framing was overcautious.
3. The chunked DeltaNet variant lets the port reuse candle's existing op
   set without writing custom Metal/CUDA kernels. The optimization path
   (fused kernels, flash-attention on Metal) is incremental from there.

Route γ stays as the fallback if δ runs into kernel-correctness drift that
can't be resolved in 2–3 weeks of effort.

Route β is rejected on the same grounds ADR 0001 rejected it: the inference
binary stays Rust.

Route α is the silent default if F27 fails.

## Why not …

- **Switch the production model now.** Qwen3-32B is validated end-to-end.
  Qwen3.6 has not been spike-tested on the substantive (Conviction) axis,
  let alone through the candle runtime. A swap before F27/F28 would invert
  the dependency direction the spike → hard-gate flow was set up to enforce
  in ADRs 0001 and 0002.
- **Wait for candle upstream to add Qwen3-Next.** The repo has no PRs in
  flight as of 2026-05-05. We could file an issue and wait, but the path
  blocks Phase 4+ work indefinitely. Cheaper to port a chunked DeltaNet
  ourselves and contribute back if the implementation is clean. (See F31.)
- **Fork llama.cpp and patch its cvec hook to emit per-layer events to a
  Rust callback.** This is option γ-prime — keep llama.cpp's runtime,
  inject our hook surface via FFI callbacks. Possible but technically
  worse than δ: we now own a llama.cpp fork *and* an FFI bridge, rather
  than ~600 LoC of pure Rust. The LoC count is similar; the
  cross-language debugging surface is much worse.
- **Run extraction on Qwen3.6 and inference on Qwen3-32B (cross-model
  transfer).** Steering vectors are known to transfer poorly across model
  families (Mitra §A.4; Subramani 2022). Within-family transfer (e.g.
  Qwen3-32B-bf16 vector applied to Qwen3-32B-Q4) works because the residual
  stream basis is preserved by quantization noise. Across architectures
  (pure transformer ↔ hybrid DeltaNet) the basis is not preserved — the
  DeltaNet recurrence adds dimensions to the hidden state's effective
  geometry that pure-transformer extraction never sees. Risky and not worth
  the experimental contamination.

## Consequences

- Phase 4.x continues on the Qwen3-32B path. No pivot, no churn. ADR 0002's
  PASS still applies; the Phase 4.3 hard gate (F1) still gates Phase 9.3.
- F27 and F28 land in the Python eval / llama.cpp spike lanes — neither
  blocks production work. They run in parallel to Phase 4.x.
- If F27 fails: this ADR closes as "Qwen3-32B remains the production model
  through v1." The Qwen3.6 weights on disk become an artifact of
  exploration, not a production dependency.
- If F27 + F28 both pass: ADR 0010 lands with measured benchmarks for δ
  (port) vs γ (FFI), the choice between them is made on data, and a 1–2-week
  port lane opens after Phase 9 headline (the headline run is the higher-
  priority deliverable).
- Risk: if the candle ecosystem ships qwen3-next support before we do
  (community PRs we couldn't find today *might* land in the next 4–8 weeks),
  the port becomes redundant. Cheaper to discover late than build
  speculatively now. F31 captures the watch-task.

## Sibling option: distilled-model swap as a parallel lane (F32)

This ADR's F27–F31 lane asks *can we adopt the new architecture?* A separate
question, not in scope for this ADR's decision but worth flagging because the
constraints overlap: *should we adopt a different training method on the
architecture we already support?*

**DeepSeek-R1-Distill-Qwen-32B** sits inside xianvec's existing constraint
envelope (dense, pure transformer, candle-supported via `quantized_qwen2`,
fits 36 GB at Q4) but differs from Qwen3-32B in its training method:
distilled from R1's reasoning trace, not RL-aligned for safety. Per
Ayyub 2026 (cited under §"Empirical priors" below), distilled models
produce ~2× the per-layer logit-difference of RL-trained models for the
same vector — meaningfully more steering headroom on the same architecture.

The runtime port is also smaller than the F29 Qwen3-Next port: vendoring
`quantized_qwen2` mirrors the existing `vendor_qwen3.rs` pattern but on a
simpler attention block (no q/k norm layers Qwen3 added). ~3–5 days
versus F29's ~1–2 weeks.

This is filed as **F32 in FOLLOWUPS.md**, not as a decision in this ADR,
because:

1. The trigger is downstream: it only matters if the Qwen3-32B headline
   run shows steering headroom is the binding constraint, OR if F27 fails
   and Qwen3-32B is locked in but capability is being left on the table.
2. The two lanes (F27–F31 hybrid path, F32 distilled-model swap) are
   independent — they can run in parallel and resolve via separate ADRs
   (0010 for the hybrid runtime, 0011 if F32 ratifies a new production
   model).

If both lanes succeed, the eventual question becomes whether to combine
them (a hypothetical future "DeepSeek-R1-Distill-Qwen3-Next" doesn't exist
yet, and the distilled-on-hybrid combination has no published precedent).
That's a 2026-Q4 problem, not now.

## Empirical priors worth pinning before the spike

From existing Qwen3 steering work scanned 2026-05-05:

- Layer sweeps consistently find vectors extracted from layers >50% depth
  steer best; **RL-trained models peak deeper (70–85% depth) than distilled
  variants (50–65%)**. Qwen3.6-27B is RL-trained — favour deeper extraction
  layers (L36–L46 if 64 layers; scale to actual layer count) rather than the
  L42 used in ADR 0002 on Qwen3-32B.
- **RL-trained models show ~½× the per-layer logit-difference of distilled
  models** for the same vector. Calibration: if F27's α-sweep shows weaker
  steering than ADR 0002's 1.17 swing, that may not be a vector quality
  issue — it may be RL post-training compressing the headroom.
- repeng's `model_layer_list()` discovery walks `model.repeng_layers` →
  `model.model.layers` → `transformer.h`. Qwen3-Next's HF
  `transformers` impl uses `model.model.layers` (verified in PR #73's fix
  path). Should work without an override; F30 confirms.
- "The Rogue Scalpel" (arXiv 2509.22067) shows activation steering can
  weaken unrelated guardrails. Less directly relevant for trading than for
  general assistants, but the F27 eval should include a coherence/safety
  control prompt set, not just the disposition score swing.

## References

- ADR 0001 (`decisions/0001-inference-backend.md`) — original Qwen3-32B + candle decision
- ADR 0002 (`decisions/0002-spike-validation.md`) — MLX spike PASS on toy axis
- ADR 0007 (`decisions/0007-inference-throughput-routes.md`) — runtime throughput options matrix (this ADR mirrors that structure)
- [llama.cpp `src/models/qwen3next.cpp`](https://github.com/ggml-org/llama.cpp/blob/master/src/models/qwen3next.cpp) — hybrid architecture build, ~633 LoC
- [llama.cpp `src/models/delta-net-base.cpp`](https://github.com/ggml-org/llama.cpp/blob/master/src/models/delta-net-base.cpp) — three DeltaNet variants (chunked / autoregressive / fused), ~590 LoC
- [llama.cpp `src/llama-adapter.cpp` cvec apply](https://github.com/ggml-org/llama.cpp/blob/master/src/llama-adapter.cpp) — single `ggml_add` per layer
- [llama.cpp PR #19375 — qwen3next graph optimization](https://github.com/ggml-org/llama.cpp/pull/19375)
- [Issue #15940 — Qwen3-Next support](https://github.com/ggml-org/llama.cpp/issues/15940)
- [`vgel/repeng` PR #73 — Qwen3 attention_type fix](https://github.com/vgel/repeng/pull/73)
- [`vgel/repeng` Issue #60 — Try with RWKV / MAMBA (closed unresolved)](https://github.com/vgel/repeng/issues/60)
- [Gated Delta Networks: Improving Mamba2 with Delta Rule (arXiv 2412.06464)](https://arxiv.org/abs/2412.06464)
- [What I Learned (And Didn't) Steering Qwen3 Models — Omar Ayyub, 2026-01](https://omar.bet/2026/01/17/What-I-Learned-Steering-Qwen3-Models/)
- [The Rogue Scalpel: Activation Steering Compromises LLM Safety (arXiv 2509.22067)](https://arxiv.org/html/2509.22067)
- [`oxideai/mlx-rs`](https://github.com/oxideai/mlx-rs) — relevant if F12 (mlx-rs spike) lands first and supplies a hook surface for hybrid arch
- FOLLOWUPS.md F27 (Python eval), F28 (llama.cpp cvec spike), F29 (candle DeltaNet port — chunked), F30 (repeng layer-discovery on qwen35), F31 (watch upstream candle for qwen3-next PRs), F32 (Tier-2 model swap — DeepSeek-R1-Distill-Qwen-32B; sibling lane)
