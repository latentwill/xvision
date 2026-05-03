# 0007 — Routes to fix candle Q4 throughput on Apple Silicon

**Status:** Open. Spike target for Phase 4.5 (after vector application is wired
on the existing slow path) or Phase 9 prerequisite (hard gate before forward
paper).
**Owner:** TBD
**Date:** 2026-05-03

## Context

Phase 3 wired `xianvec-trader::run_trader` against `xianvec-inference::Qwen3Engine`
(candle 0.10 `quantized_qwen3` Q4_K_M GGUF) on M4 Max. End-to-end smoke surfaced:

| Path                                          | Throughput   |
| --------------------------------------------- | ------------ |
| `candle` Q4_K_M / Metal (smoke-qwen3)         | ~0.64 tok/s decode, ~1.2 tok/s prefill |
| `llama.cpp` Q4_K_M / Metal (community, M4)    | ~30–50 tok/s on 7B, ~16 tok/s on 32B   |
| MLX 4-bit / Apple Silicon (community, M4 Max) | ~25–40 tok/s on 32B-class             |

Same model. Same machine. Same quantization. The candle path is **20–60× slower**
than the alternatives. A 600-token Trader prompt + 384-token decode ⇒ 17 min
wall on candle vs ~30 s on the alternatives.

ADR 0001 already flagged this as a known shape (candle's `quantized_qwen3` does
not surface flash-attention or fused Q4_K_M kernels), but Phase 0.2 didn't
benchmark it; the cost only became visible once Stage 2 made the Trader the
hot path.

The Phase 4.3 hard gate (vectors must steer the candle runtime equivalently to
the MLX spike) does not depend on throughput, so Phase 4 can proceed against
the slow path. But forward paper / live (Phase 9) needs ≥10 tok/s end-to-end
to be tolerable, and the headline rented-GPU run still benefits from a faster
local dev loop.

## What Phase 4+ actually needs from the runtime

The steering vectors live at a specific transformer layer's residual stream:

```rust
pub trait LayerHook: Send + Sync {
    fn apply(&self, layer_idx: usize, residual: &Tensor) -> Result<Tensor>;
}
```

Anything that can intercept the residual at a configured layer satisfies the
contract. Output-only knobs (logits processor, grammar) do **not** — they
cannot reproduce hidden-state steering.

So the throughput route must preserve a **forward-pass hook surface**, not
just produce text. That filters the option space.

## Options

### A. Vendor candle's `qwen3.rs` and inline `quantized_nn` bits

ADR 0001 already names this. Pull `candle_transformers::models::qwen3` and
the supporting `quantized_nn` / `utils::repeat_kv` / flash-attention pieces
into our tree as `xianvec-inference/src/model/qwen3_steered.rs`. Replace the
attention block with the flash-attention path; keep our hook surface; ship.

- **Pros:** stays in Rust, keeps direct hook control, no FFI, expected
  throughput ~10 tok/s per community ports of the same approach.
- **Cons:** ~1 week of model-vendoring + maintenance burden; we now own the
  forward pass and have to track upstream candle bugfixes; flash-attention
  on Metal in candle is less battle-tested than on CUDA.
- **Steering: yes**, native Rust hooks at any layer.

### B. `mlx-rs` (Rust bindings to Apple's MLX)

`oxideai/mlx-rs` — unofficial but actively maintained Rust bindings to MLX
(MSRV 1.82). MLX is the fastest framework on Apple Silicon for Q4 inference
in 2026 (Apple research: Qwen3-14B-4bit on M5 hit 4.06× TTFT and 1.19×
decode vs M4; Ollama switched its Apple Silicon backend to MLX in March 2026).

- **Pros:** likely fastest of the four routes on M4 Max; stays in Rust;
  Apple is the upstream and continues to invest.
- **Cons:** binding maturity is the open question — verify `mlx-rs` exposes
  module-level forward hooks (or its Python `mlx-lm` equivalent does and the
  binding plumbs them through). If hooks aren't surfaced, this collapses to
  option D.
- **Steering: needs validation.** The Phase 0.3 spike already steered Qwen3
  via MLX in Python by patching `nn.Module.__call__` — the hook surface is
  there in MLX core, the question is binding-level access from Rust.

### C. `llama-cpp-2` / `llama-cpp-rs` (Rust bindings to llama.cpp)

Mature, fast, most-tested path on M4. llama.cpp's Q4_K_M Metal kernels are
the implicit baseline that candle is supposed to match (and currently
doesn't).

- **Pros:** known-good throughput; well-maintained bindings; little surprise
  surface.
- **Cons:** llama.cpp's public API is decode-side (logits and KV cache); it
  does **not** expose per-layer residual hooks. Steering would have to be
  rebuilt as a logits processor or via a custom llama.cpp fork — both of
  which abandon the residual-stream contract that Phase 4 is built on.
- **Steering: no** without forking llama.cpp itself. Disqualifying for v1.

### D. Run vLLM / llama-server / Ollama locally; route Trader over OpenAI-compat

The Stage 1 `OpenAICompatIntern` already speaks this wire format. Mirror it
for Stage 2: split `xianvec-trader` into a `local-candle` impl (current
code) and an `openai-compat` impl (new). Pick at config time.

- **Pros:** zero per-token cost in our process; vLLM-metal community plugin
  exists; the wire format is already a workspace primitive.
- **Cons:** same as option C — no residual hook. Steering becomes a
  vLLM/llama-server logits-processor plugin or a sidecar service, both of
  which abandon the Phase 4 contract.
- **Steering: no** in the standard sense. vLLM has a logits-processor extension
  point but logits-side steering ≠ residual-stream steering.

## Decision matrix

| Route                  | Throughput | Steering hook | Effort   | Risk           |
| ---------------------- | ---------- | ------------- | -------- | -------------- |
| A. Vendor candle qwen3 | ~10 tok/s  | ✓ native      | ~1 week  | Maintain a fork |
| B. mlx-rs              | ~25–40 tok/s | ?           | 2–5 days spike + 3–5 days port | Binding maturity, hook surface unverified |
| C. llama-cpp-rs        | ~30 tok/s  | ✗             | 2 days   | Loses Phase 4 contract |
| D. HTTP (vLLM/etc.)    | ~30+ tok/s | ✗             | 2 days   | Loses Phase 4 contract |

C and D are eliminated by the steering-hook requirement (they remain valid
forward-paper deployment targets if we bifurcate into a "decision-only" arm
that has already absorbed steering at training/extraction time, but that is a
different project shape).

## Proposed plan

1. **Phase 4 proceeds on the slow candle path.** Hook correctness can be
   validated with tens of tokens per smoke run; throughput is irrelevant here.
2. **Phase 4.5 spike: `mlx-rs` viability** (1–2 days)
   - Run `oxideai/mlx-rs` against the same Q4 weights; record TTFT + decode
     tok/s on the same prompt as `smoke-qwen3` for an apples-to-apples
     comparison against candle's 0.64 tok/s baseline.
   - Verify whether `mlx-rs` exposes (or can be extended to expose) a
     pre-/post-block forward hook. If yes, port `Qwen3Engine` to mlx-rs and
     deprecate the candle Q4 runtime — Phase 4.3 hard gate then re-runs on
     the mlx-rs path.
   - If hook surface is absent and patching it needs upstream changes, fall
     back to option A (vendor candle qwen3).
3. **Document the spike outcome** as `decisions/0008-runtime-throughput-fix.md`
   with the chosen route and measured numbers.

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
