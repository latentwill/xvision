# Cline Runtime Unification — Stage 4: Throughput Hardening — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the record pass scale to large backtests — a pool of sidecar processes for parallel record, agent reuse where safe, and batched/backpressured event emission under sustained 1000+ step load — meeting a throughput target measured during profiling.

**Architecture:** Replay is already network-free (Stage 3), so only the *record* pass is throughput-bound. The sidecar is single-active-run by design, so record parallelism means a **pool of sidecar processes** with record work sharded across them. This stage is **profiling-gated**: Task 1 measures the record pass and *sets* the target; the pool/reuse/batching tasks are scoped by measured need and may be deferred to a follow-up if Task 1 shows the single-sidecar path already suffices. **No throughput number is invented here — it is an output of Task 1.**

**Tech Stack:** Rust (`xvision-agent-client`, `xvision-eval`, `xvision-observability`), TypeScript (`xvision-agentd`), `tokio` (process pool + channels), `criterion`/custom bench harness.

**Umbrella spec:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md` (Stage 4 + "Subplan inheritance contract" + Risks table + Open questions 2, 4). **Builds on Stages 1–3.**

---

## Inherited contract gates (from umbrella §"Subplan inheritance contract")

- [ ] **Item 2 — Failure + recovery (pool level, non-negotiable; "Stage 1 through Stage 4").** A crashed sidecar in the pool is detected and replaced; the in-flight run on it fails cleanly and its recording is marked `incomplete` (Stage 2 semantics) — a pool-mate crash never corrupts another run's trajectory.
- [ ] **Item 4 — Piping + backpressure (at scale, non-negotiable).** Under sustained 1000+ step load, the record/replay event channels hold their bounds, apply backpressure (frames lossless), batch writes for throughput, and surface dropped-frame/backpressure observability. Profiling must exercise the bounds, not just the happy path.
- [ ] **Item 3 — Operational visibility (pool level).** Pool size, per-sidecar busy/idle state, restart count, and record-pass throughput are surfaced (CLI + dashboard).

Stage 4 exit (umbrella): *record-pass throughput meets a target set during Stage 3/Stage 4 profiling.* Pool may fold into Stage 3 or run as a follow-up per measured need.

---

## File Structure

- Create: `crates/xvision-agent-client/src/pool.rs` — `SidecarPool` (N processes, lease/return, health).
- Create: `crates/xvision-eval/benches/record_pass.rs` (or `crates/xvision-engine/tests/record_throughput.rs` if criterion isn't a dep) — the profiling harness.
- Create: `docs/superpowers/specs/2026-05-24-cline-record-throughput-target.md` — the measured target + decision record.
- Modify: `crates/xvision-observability/src/trajectory/channel.rs` (batched writer), `crates/xvision-observability/src/bus.rs` (tune/measure under load).
- Modify: `crates/xvision-eval/src/ab_compare.rs` (shard record across the pool).
- Modify: `crates/xvision-cli/src/commands/trajectory/` + `frontend/web/src/features/agent-runs/` (pool health).

---

### Task 1: Profiling harness — measure and SET the target (gates the rest)

**Files:**
- Create: `crates/xvision-eval/benches/record_pass.rs` (or a `#[ignore]`-by-default throughput test if `criterion` is not already a workspace dev-dep — check `Cargo.toml` first)
- Create: `docs/superpowers/specs/2026-05-24-cline-record-throughput-target.md`

- [ ] **Step 1: Write the profiling harness**

Drive a record pass over a representative cycle set (≥1000 slot-steps) against a single sidecar using a deterministic mock provider with a fixed per-call latency (so the harness measures *xvision overhead*, not provider latency). Capture: steps/sec, per-step p50/p95 latency, frame-write throughput, frame-channel max depth (did backpressure engage?), sidecar RSS, and dropped-frame count (must be 0).

- [ ] **Step 2: Run the harness and record the baseline**

Run (from a worktree with a per-stage `CARGO_TARGET_DIR`):
```bash
cargo bench -p xvision-eval --bench record_pass   # or: cargo test -p xvision-engine record_throughput -- --ignored --nocapture
```
Capture the numbers. **This run sets the target.** Write `docs/superpowers/specs/2026-05-24-cline-record-throughput-target.md` recording: the measured single-sidecar baseline, the throughput a realistic backtest needs (cycles × slots ÷ acceptable wall time — state the assumed backtest size), and therefore the **target** and **whether a pool is required**. If the single sidecar already meets need, record that Tasks 2–3 are deferred to a follow-up and proceed to Tasks 4–6 only as needed.

- [ ] **Step 3: Commit** `bench(stage4): record-pass profiling harness + measured target`.

---

### Task 2: Sidecar pool for parallel record (if Task 1 shows need)

**Files:**
- Create: `crates/xvision-agent-client/src/pool.rs`
- Modify: `crates/xvision-agent-client/src/lib.rs`, `crates/xvision-eval/src/ab_compare.rs`
- Test: inline `#[cfg(test)]` + `crates/xvision-engine/tests/sidecar_pool_record.rs`

> **Gate:** only undertake if Task 1's target requires parallelism. If deferred, note it in the throughput doc and skip to Task 4.

- [ ] **Step 1: Failing tests** — (a) `SidecarPool::new(n)` spawns `n` sidecars; (b) `pool.lease().await` hands out a sidecar and never the same one to two concurrent leases (respecting single-active-run); (c) sharding K record jobs across `n` sidecars completes all K and each job's frames land under its own recording with no cross-contamination.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** `SidecarPool` over `tokio::sync::Semaphore` + a `Vec<AgentClient>`; `lease` acquires a permit and returns a guard that returns the client on drop. Shard `ab_compare`'s record pass across leases. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage4): sidecar pool for parallel record (item 2/4)`.

---

### Task 3: Agent reuse across runs (open question 2 — audit-first)

**Files:**
- Modify: `xvision-agentd/src/methods/session.ts`, `src/session/build-agent.ts`
- Test: `xvision-agentd/test/session/agent-reuse.test.ts`

> **Audit-first (not a placeholder):** The umbrella open question 2 asks whether a Cline `Agent` can be safely reused across `start_run` boundaries. First write a test that reuses an agent across two runs and asserts no state bleed (run 2's output is independent of run 1). If Cline's `Agent` is not safely reusable, the test will surface bleed — in that case **do not implement reuse**; record the finding in the throughput doc and keep per-run lazy build (already correct). Reuse is an optimization, never a correctness requirement.

- [ ] **Step 1: Write the reuse-safety test** (two sequential runs on one agent; assert independence).
- [ ] **Step 2: Run — observe.** If clean, implement guarded reuse (reset per-run state, keep the provider gateway warm). If bleed, stop and document. **Step 3: Commit** `feat(stage4): agent reuse where safe (or: document non-reusability)`.

---

### Task 4: Batched, backpressured event emission under load (item 4 at scale)

**Files:**
- Modify: `crates/xvision-observability/src/trajectory/channel.rs` (batch writer), `crates/xvision-observability/src/bus.rs`
- Test: `crates/xvision-engine/tests/frame_batch_backpressure.rs`

- [ ] **Step 1: Failing tests** — (a) frame writes are batched (N frames → one SQLite transaction) above a threshold, improving the Task-1 throughput metric; (b) under a producer faster than the consumer, the lossless frame channel applies backpressure (producer awaits) and **zero** frames drop; (c) the observability ring (`RunEventBus`, cap 4096) still drops *non-lifecycle* deltas under the same load and records the drop count — confirming the two channels keep their distinct lossy/lossless contracts at scale.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** a batching writer in front of `TrajectoryStore.append_frame` (buffer + flush on size/time), preserving order; verify backpressure semantics from Stage 2 hold under sustained load. **Step 4: Run — PASS;** re-run Task 1 harness and record the improved number. **Step 5: Commit** `perf(stage4): batched frame writes + backpressure under load (item 4)`.

---

### Task 5: Pool-level failure recovery (item 2 Stage 4 piece)

**Files:**
- Modify: `crates/xvision-agent-client/src/pool.rs`
- Test: `crates/xvision-engine/tests/pool_crash_recovery.rs`

- [ ] **Step 1: Failing tests** — (a) kill a leased sidecar mid-record; assert the pool detects the dead process, the in-flight run fails cleanly with its recording marked `incomplete` (Stage 2), and **other** pool members' in-flight recordings are unaffected; (b) the pool replaces the dead sidecar so subsequent leases succeed; (c) restart count is observable.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** health detection (process exit / failed health RPC) + respawn; ensure crash isolation (one sidecar's death is scoped to its own lease/recording). **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage4): pool crash isolation + respawn (item 2)`.

---

### Task 6: Pool + throughput visibility (item 3)

**Files:**
- Modify: `crates/xvision-cli/src/commands/trajectory/` (a `pool-status` / `--stats` affordance), `frontend/web/src/features/agent-runs/`
- Test: CLI test + frontend typecheck

- [ ] **Step 1:** surface pool size, busy/idle per sidecar, restart count, and record-pass steps/sec via CLI (`xvn trajectory pool-status` or extend `inspect`) and the dashboard (inline strip; **no popups**).
- [ ] **Step 2: Run — PASS;** `cd frontend/web && npm run typecheck` clean. **Step 3: Commit** `feat(stage4): pool + throughput visibility (item 3)`.

---

### Task 7: Exit gate — throughput meets the measured target

**Files:**
- Test: re-run the Task-1 harness; update the throughput doc.

- [ ] **Step 1:** Re-run `record_pass` with the pool + batching enabled at the realistic backtest size; assert measured throughput ≥ the target recorded in Task 1's doc (the assertion threshold *is* the doc's number, not an invented constant). Record the final figure and mark the target met. **Step 2: Commit** `test(stage4): record-pass meets throughput target`.

---

## Self-Review

- **Spec coverage (Stage 4 scope):** sidecar pool for parallel record (Task 2 ✓), agent reuse if Cline supports it (Task 3, audit-first ✓), event-emission batching/backpressure (Task 4 ✓), profiling under sustained 1000+ step load (Tasks 1, 7 ✓). Exit = throughput meets a profiling-set target (Tasks 1, 7 ✓), and the "may fold/defer" framing is honored via the Task-1 gate.
- **Item 2 (pool) ✓** Task 5 — crash isolation + respawn; recording marked incomplete, pool-mates unaffected.
- **Item 4 (at scale) ✓** Task 4 — batching + lossless backpressure proven under load; lossy-vs-lossless channel contracts re-verified at scale.
- **Item 3 (pool) ✓** Task 6.
- **Honesty on the number:** the throughput target is *measured* in Task 1 and *asserted against* in Task 7 — never invented. Tasks 2–3 are explicitly gated on Task 1's measured need, matching the umbrella's "may fold into Stage 3 or run as a follow-up."
- **Placeholder scan:** the only deferred values (throughput target, reuse-safety verdict) are explicit measurement/audit outputs with concrete protocols, not silent TBDs.
- **No-cargo discipline:** all `cargo bench`/`cargo test` steps run from a worktree with a per-stage `CARGO_TARGET_DIR`.
