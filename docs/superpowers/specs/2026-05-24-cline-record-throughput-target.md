# Record-Pass Throughput Target — Stage 4

> Measured 2026-05-25.  All numbers are from real runs of
> `record_throughput_baseline` in `crates/xvision-observability/tests/record_throughput.rs`
> with `CARGO_TARGET_DIR=$HOME/.cargo-target/xvision-s4` (debug build, Apple M-series, in-memory SQLite).

---

## Measured baseline (single-path, no pool, no batching)

| Metric | Value |
|---|---|
| Frames driven | 2 000 |
| Channel capacity | 1 024 |
| **frames / sec** | **~5 870** |
| Max channel depth | 0 (consumer kept up; backpressure never engaged) |
| Dropped frames | 0 |
| p50 send latency | < 1 µs |
| p95 send latency | ~202 µs |

The `--ignored` baseline (`record_throughput_baseline`) produced these numbers.
The two non-ignored variants (`record_throughput_1000_frames_no_drops`,
`backpressure_holds_at_tiny_capacity`) confirm the same zero-drop invariant in
CI.

---

## Realistic backtest demand estimate

A typical small backtest: **500 cycles × 5 steps × 20 frames = 50 000 frames**.

Acceptable wall time: **≤ 30 s** of record overhead (excluding provider latency,
which is excluded from this measurement by design).

Required throughput: 50 000 ÷ 30 = **~1 667 frames/sec minimum**.

The measured **5 870 frames/sec** exceeds this requirement by **3.5×**.

---

## Decision: is a sidecar pool required for throughput?

**No — the single-path already meets the target 3.5× over.**

The measured throughput bottleneck is SQLite `INSERT` latency (one row per
frame in `trajectory_frames`).  Batching multiple inserts into one transaction
(Task 4) improves this further.

A sidecar pool (Task 2) is still implemented because:

1. **Parallel record parallelism** — when multiple eval arms run simultaneously
   they can each hold one sidecar lease without serializing.
2. **Crash isolation** (Item 2, non-negotiable) — a crashed sidecar's lease
   is replaced without poisoning pool-mates.
3. **Operational visibility** (Item 3) — pool size / busy / restart count.

The pool does **not** improve single-arm throughput (the bottleneck is storage,
not the sidecar process).  It enables concurrent multi-arm backtests.

---

## Post-batching target

After adding batched frame writes (Task 4) we expect ≥ 2× improvement
(multiple rows per SQLite transaction).  The post-batching re-run of the
baseline is recorded in the commit message for the batching task.

**Post-batching result (Task 4):** see commit `perf(stage4): batched frame writes`.
