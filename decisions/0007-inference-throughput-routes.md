# 0007 — Routes for Trader inference throughput on Apple Silicon

> **2026-05-10:** Project renamed `xianvec` → `xvision`. References below reflect the post-rename name; project history prior to this date used `xianvec`.

**Status:** Revised 2026-05-07 per ADR 0011. Trader-only inference throughput considerations remain valid; CV-driven steering-hook constraints have been excised.
**Owner:** TBD
**Date:** 2026-05-03 (revised 2026-05-07)

## Context

Phase 3 wired `xvision-trader::run_trader` against `xvision-inference::Qwen3Engine`
(candle 0.10 `quantized_qwen3` Q4_K_M GGUF) on M4 Max. End-to-end smoke surfaced:

| Path                                          | Throughput   |
| --------------------------------------------- | ------------ |
| `candle` Q4_K_M / Metal (smoke-qwen3)         | ~0.64 tok/s decode, ~1.2 tok/s prefill |
| `llama.cpp` Q4_K_M / Metal (community, M4)    | ~30–50 tok/s on 7B, ~16 tok/s on 32B   |
| MLX 4-bit / Apple Silicon (community, M4 Max) | ~25–40 tok/s on 32B-class             |
| OpenAI-compat HTTP (vLLM/Ollama localhost)    | ~30+ tok/s on 32B (no per-token in-process cost) |

Same model. Same machine. Same quantization. The candle path is **20–60× slower**
than the alternatives. A 600-token Trader prompt + 384-token decode ⇒ 17 min
wall on candle vs ~30 s on the alternatives.

ADR 0001 already flagged this as a known shape (candle's `quantized_qwen3` does
not surface flash-attention or fused Q4_K_M kernels), but Phase 0.2 didn't
benchmark it; the cost only became visible once Stage 2 made the Trader the
hot path.

Forward paper / live needs ≥10 tok/s end-to-end to be tolerable.

## What the runtime needs (post-ADR-0011)

The Trader needs to produce a structured JSON decision from a briefing prompt. Anything that can run a Qwen3-class chat model and return text satisfies the contract. **There is no steering-hook constraint.** OpenAI-compatible HTTP is a fully valid target.

## Options

### A. Vendor candle's `qwen3.rs` and inline `quantized_nn` bits

ADR 0001 already names this. Pull `candle_transformers::models::qwen3` and
the supporting `quantized_nn` / `utils::repeat_kv` / flash-attention pieces
into our tree. Replace the attention block with the flash-attention path; ship.

- **Pros:** stays in Rust, no FFI, expected
  throughput ~10 tok/s per community ports of the same approach.
- **Cons:** ~1 week of model-vendoring + maintenance burden; we now own the
  forward pass and have to track upstream candle bugfixes; flash-attention
  on Metal in candle is less battle-tested than on CUDA.

### B. `mlx-rs` (Rust bindings to Apple's MLX)

`oxideai/mlx-rs` — unofficial but actively maintained Rust bindings to MLX
(MSRV 1.82). MLX is the fastest framework on Apple Silicon for Q4 inference
in 2026.

- **Pros:** likely fastest of the routes on M4 Max; stays in Rust.
- **Cons:** binding maturity is the open question; production-grade error handling and Metal cache lifecycle still maturing.

### C. `llama-cpp-2` / `llama-cpp-rs` (Rust bindings to llama.cpp)

Mature, fast, most-tested path on M4. llama.cpp's Q4_K_M Metal kernels are
the implicit baseline that candle is supposed to match (and currently
doesn't).

- **Pros:** known-good throughput; well-maintained bindings; little surprise
  surface.
- **Cons:** Native FFI; llama.cpp ABI churn between versions.

### D. Run vLLM / llama-server / Ollama locally; route Trader over OpenAI-compat

The Stage 1 `OpenAICompatIntern` already speaks this wire format. The Trader's
`TraderBackend` HTTP trait + `OpenAiCompatBackend` impl is the natural
default — the same backend abstraction Stage 1 uses.

- **Pros:** zero per-token cost in our process; the wire format is already a workspace primitive; no in-process model loading; trivial to swap backends (OpenAI, Anthropic, OpenRouter, vLLM, llama.cpp, Ollama).
- **Cons:** Adds a localhost HTTP hop for fully air-gapped runs; depends on a separate process being managed.

## Decision matrix

| Route                  | Throughput | Effort   | Risk           |
| ---------------------- | ---------- | -------- | -------------- |
| A. Vendor candle qwen3 | ~10 tok/s  | ~1 week  | Maintain a fork |
| B. mlx-rs              | ~25–40 tok/s | 2–5 days spike + 3–5 days port | Binding maturity |
| C. llama-cpp-rs        | ~30 tok/s  | 2 days   | FFI / ABI tracking |
| D. HTTP (vLLM/etc.)    | ~30+ tok/s | landed   | Localhost process management |

**Decision (post-ADR-0011): D is the default.** `TraderBackend` HTTP via
`OpenAiCompatBackend` is the default Trader path, matching Stage 1's
`InternBackend`. Local candle remains an optional fully-air-gapped fallback
under route A. Routes B and C remain advisory for someone optimizing the
local candle path further.

## Proposed plan (post-ADR-0011)

1. **Default path: HTTP (option D).** Both Intern and Trader call OpenAI-compat
   backends by default. Local vLLM/Ollama serves as the "local" path; remote
   API providers serve the "cloud" path; zero in-process model loading.
2. **Optional path: candle local inference.** Keep `xvision-trader`'s candle
   wrapper for fully-air-gapped runs. Performance follows the cheap-wins
   benchmarks below.
3. **No spike required.** ADR 0007 was originally gated on the steering-hook
   constraint that ADR 0011 retired. With that constraint gone, HTTP wins on
   throughput, ergonomics, and zero local model management.

## Build flags worth verifying first (cheap wins before any port)

Before any large port, run the existing `smoke-qwen3` under each of these and
record the deltas — some of the 100× gap may be free:

- `RUSTFLAGS="-C target-cpu=native"`
- `cargo build --release --features metal` (already on by default; confirm not
  silently fallen back to CPU — `device: metal` log line is the witness)
- Larger batch on prefill — check whether candle is processing the prompt
  one token at a time vs as a single tensor (1.2 tok/s prefill is suspicious;
  it should be 5–10× faster than decode, not the same).

Two of these fixed could push candle from 0.64 → 5+ tok/s without a port
and would change the cost-benefit math of options A and B.

## References

- [oxideai/mlx-rs — Unofficial Rust bindings to MLX](https://github.com/oxideai/mlx-rs)
- [Ollama switching to MLX on Apple Silicon (2026-03-30)](https://ollama.com/blog/mlx)
- [Apple ML Research — LLMs with MLX on M5 (2026-01)](https://machinelearning.apple.com/research/exploring-llms-mlx-m5)
- [vllm-project/vllm-metal — community Metal plugin](https://github.com/vllm-project/vllm-metal)
- [llama.cpp Apple Silicon performance discussion #4167](https://github.com/ggml-org/llama.cpp/discussions/4167)
- [llama.cpp vs MLX vs Ollama vs vLLM (2026 benchmarks)](https://contracollective.com/blog/llama-cpp-vs-mlx-ollama-vllm-apple-silicon-2026)
- [candle quantized example](https://github.com/huggingface/candle/blob/main/candle-examples/examples/quantized/main.rs)
- ADR 0001 (`decisions/0001-inference-backend.md`) — original backend choice + Phase 3 throughput addendum
- Implementation plan §3.1, §4.3, §9.x

## Cheap-wins benchmark results (2026-05-03)

**Hardware:** M4 Max, 36 GB RAM, macOS 15.6.1
**Model:** Qwen3-32B Q4_K_M GGUF, `models/qwen3-32b-q4-gguf/Qwen_Qwen3-32B-Q4_K_M.gguf`
**Prompt:** 22 tokens; **Decode budget:** 48 tokens (default smoke-qwen3 fixture)

| Experiment | Build | Decode tok/s | Prefill tok/s | Notes |
| --- | --- | --- | --- | --- |
| Baseline (ADR body) | release+metal | 0.64 | 1.2 | prior measurement, cold system |
| Exp 1 – Metal confirmed (run 1, cold shader cache) | release+metal | 3.10 | 0.81 | shader JIT included in prefill time |
| Exp 1 – Metal confirmed (run 3, warm shader cache) | release+metal | 4.92 | 1.26 | post shader-cache warmup |
| Exp 1 – Metal confirmed (run 4, warm shader cache) | release+metal | 6.35 | 0.58 | prefill time still noisy (JIT variance) |
| Exp 2 – target-cpu=native | release+metal+native | not measured | not measured | rebuild succeeds; no benchmark run yet (post-resolution) |
| Exp 3 – prefill batching | release+metal (code audit) | n/a | n/a | prompt IS a single tensor; see analysis |

**Experiment 1 — Metal device confirmation:**
`engine.rs` already logs `device: metal` at line 78 (`Qwen3Engine::pick_device`). Confirmed active on every run. Metal is not silently falling back to CPU. No code change needed for this experiment; the log line was added in a prior commit.

**Experiment 2 — `RUSTFLAGS="-C target-cpu=native"` rebuild:**
Originally reported as blocked by a `lzma-rust2 0.15.3` vs `crc 2.1.0` API incompatibility surfaced when RUSTFLAGS invalidates the build fingerprint cache. **Re-investigated post-Phase-4 implementation: the block does not reproduce.** `cargo tree -i lzma-rust2` returns "package not found" and `cargo tree -i crc` resolves to `crc 3.4.0` (via `sqlx-core`); `RUSTFLAGS="-C target-cpu=native" cargo build -p xvision-inference --bin smoke-qwen3 --features metal` succeeds clean. The likely cause of the resolution shift: the Phase 4.3 substrate work replaced a planned `zip 7.2.0` NPZ-reader dep with a hand-rolled ZIP64 parser (see `crates/xvision-inference/src/substrate.rs` doc header), and that may have pruned `lzma-rust2` from the transitive graph — though a side-by-side lockfile comparison was not performed. Whether the original report was a misdiagnosis or a real conflict the substrate work happened to repair was not pursued; the cheap-wins lane is unblocked. Actual benchmark numbers under `target-cpu=native` were not captured in this pass — carry as a follow-up so we can quantify the delta vs the warm-cache decode rate.

**Experiment 3 — Prefill batching investigation:**
`engine.rs:170`: `Tensor::new(prompt_tokens.as_slice(), &device)?.unsqueeze(0)?` — the entire prompt is sent as a single `[1, N]` tensor in one `model.forward(&input, 0)` call. This IS true batched prefill; there is no per-token prefill loop.

The suspiciously slow prefill (0.58–1.26 tok/s measured, well below the expected 5–10× decode rate) is **not** a token-at-a-time loop bug. It is instead caused by two structural issues in `candle-transformers 0.10.2/src/models/quantized_qwen3.rs`:

1. **Metal shader JIT at prefill time.** The prefill forward pass uses tensor shapes `[1, N, ...]` (N=22 for smoke prompt, N≈600 for Trader prompt). These shapes differ from the decode-step `[1, 1, ...]` shapes. Metal compiles separate kernel variants per shape family; on first process invocation the compiler runs in-band with the forward pass, adding 17–88 s of JIT overhead measured across four runs. This is Metal's default behaviour and candle makes no effort to pre-warm shaders.
2. **No flash attention.** `AttentionWeights::forward` (line 184–234 of `quantized_qwen3.rs`) uses standard `q.matmul(&k.transpose(2,3)?)` with explicit softmax — no flash-attention kernel. This is O(L²) memory and does not fuse the QK^T softmax V multiply. For L=22 this is negligible; for L=600 (Trader prompt) this adds meaningful overhead.
3. **`QMatMul::forward` dequantizes on every call.** Each of the 64 transformer layers × 4 projections dequantizes Q4_K_M weights to f16/f32 on-the-fly. On Metal this means 256 kernel dispatches per forward step, each round-tripping through the GPU command buffer.

The stable decode rate of ~5–6 tok/s on a warm shader cache is ~8–10× better than the 0.64 tok/s in the ADR body. The ADR baseline was likely measured cold, with shader JIT included.

**Findings:**
- Metal IS active; log line is already present; this is not the source of slowness.
- `target-cpu=native` rebuild succeeds in the current workspace; numeric delta vs default codegen not yet measured (follow-up).
- Prefill batching is already correct; the 1.2 tok/s baseline was shader-JIT dominated. Steady-state prefill (warm cache) tracks decode at ~0.6–1.3 tok/s for 22 tokens — still slow, primarily due to non-fused QMatMul dequantize and no flash-attention, not a loop bug.
- Warm-cache decode throughput is **5–6 tok/s** — already above the "cheap wins push ≥ 5 tok/s" threshold from the ADR body — but only after Metal shaders are compiled (first invocation still takes 10–90 s of JIT).

**Recommendation:** The ≥5 tok/s threshold is met on a warm Metal shader cache (~6 tok/s decode), but in practice each cold process start spends 10–90 s on prefill shader compilation. This means option A and option B remain relevant, but for different reasons than originally stated:
- Option A (vendor candle qwen3 + flash attention) would fix the non-fused attention and dequantize cost, and could use `candle_nn::ConcatKvCache` pre-warmed at startup.
- Option B (mlx-rs) would eliminate the JIT overhead entirely (MLX pre-compiles Metal shaders at model load).
- The `target-cpu=native` flag now builds clean; running smoke-qwen3 under it for an apples-to-apples decode/prefill measurement is a follow-up.

**Decision matrix update:** No change to the matrix rankings, but annotate option B with "MLX pre-warms Metal shaders at load; eliminates 10–90 s first-run penalty". The 5 tok/s threshold being met on warm cache does NOT eliminate the port — cold-start latency still disqualifies candle for production Trader use.

## Re-measured 2026-05-03 (post-vendor-patch + native CPU codegen)

Prompted by `target-cpu=native` proving to actually work in the current workspace and post-ADR-0011 housekeeping (the original `vendor_qwen3::forward_with_hooks` path was a CV substrate concern; that code lived in `xvision-inference` and now lives in xvision-play). Five back-to-back smoke-qwen3 runs from one shell session, same machine, same fixture (22-token prompt → 48 decode tokens), `RUSTFLAGS="-C target-cpu=native"` release build:

| Run | Decode tok/s | Prefill tok/s | Prefill wall | Notes |
| --- | --- | --- | --- | --- |
| 1 | 7.11 | 1.25 | 17.6 s | post-build, partial shader cache |
| 2 | 5.02 | 4.75 | 4.6 s | warm |
| 3 | 15.53 | 4.63 | 4.7 s | warm |
| 4 | 11.04 | 2.34 | 9.4 s | warm |
| 5 | 15.86 | 10.69 | 2.1 s | warm, hottest |

**Median warm decode: ~11 tok/s. Best: 15.86 tok/s.** Compared to the cheap-wins agent's earlier 4.92–6.35 tok/s warm-cache numbers (which did not have `target-cpu=native`), this is roughly 2× the median — the native CPU codegen is a real win on the f32 softmax / dequantize path that Metal does not own. The vendor_qwen3 wrapper itself adds overhead (a per-layer Tensor clone via IdentityHook), so the gain is purely from codegen + warmer Metal shader cache.

**Variance is large** (5.02 → 15.86, 3.2× spread across 5 runs). That's noisy enough that a single number is not safe to commit to without a controlled bench rig (consistent thermal state, CPU/GPU isolation, repeated trials with statistics). Median 11 tok/s is the headline number to use for planning, with the caveat that p95 latency under sustained load remains unmeasured.

**Cold-start unchanged:** the first invocation per shape family still pays Metal shader JIT (17.6 s on run 1's prefill, dominated by JIT). Warm-cache decode tok/s says nothing about cold-start latency.

**Findings update:**
- `target-cpu=native` is worth ~2× on Apple Silicon for this workload — material, not marginal.
- The ≥10 tok/s "forward paper minimum" from §9.x is **met on warm cache, median**, post-native-CPU.
- The ≥10 tok/s threshold is **not** met on cold start (~7 tok/s on first-shape JIT pass).
- Cold-start latency is now the dominant remaining throughput problem — not steady-state decode.

**Decision matrix update (revised):**
- **Option A** (vendor candle qwen3 + flash attention) — drops in priority. Steady-state decode is no longer the bottleneck; flash-attention would help cold start by reducing per-block compile time but doesn't eliminate the JIT phase. ~1 week of vendoring effort no longer pays back at parity with B.
- **Option B** (mlx-rs spike) — remains relevant *exclusively* for cold-start mitigation. MLX's load-time shader pre-warm is the single biggest lever on first-token latency.
- **Option C / D** (llama-cpp-rs / HTTP) — post-ADR-0011, no longer disqualified. Option D (HTTP via `TraderBackend`) is now the default Trader path; the rest of this section's analysis applies only to operators choosing the optional local-candle fallback.
- **Cold-start workaround for v1**: pre-warm the Metal shader cache during process init by running a discard 1-token forward pass on a fixed prompt shape before the first real call. This converts the 10–90 s JIT into a 10–90 s startup cost paid once, and lets sustained Trader runs operate at the warm-cache median.

**Recommendation:** keep `target-cpu=native` as the workspace's required release flag (codify in `.cargo/config.toml` so it's not optional). Defer option A. Spike option B only if cold-start latency actually blocks forward paper or if the headline GPU run on Vast.ai/RunPod (Phase 9.3) shows the same shader-JIT pattern at higher precision.
