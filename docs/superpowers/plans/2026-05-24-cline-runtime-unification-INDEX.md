# Cline Runtime Unification — Plan Index & Inheritance Ledger

**Date:** 2026-05-24
**Umbrella design:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`

Five stage plans decompose the umbrella. Each opens with an **"Inherited contract gates"** block listing only the "Subplan inheritance contract" items that bind it, as hard acceptance gates. Contracts-proper are authored by the conductor at execution time (per the umbrella); these are the spec→**plan** layer.

| Stage | Plan file | Scope |
|---|---|---|
| 0 | `2026-05-24-cline-stage0-acpx-purge.md` | ACPX purge + API-key-only license guard |
| 1 | `2026-05-24-cline-stage1-live-path.md` | Deferred Wave 3 — live/forward-paper through the Cline sidecar |
| 2 | `2026-05-24-cline-stage2-trajectory-record.md` | Frame-level trajectory persistence (versioned store) |
| 3 | `2026-05-24-cline-stage3-replay-unify-eval.md` | Replay model; unify eval; retire `BriefingCache` + `LlmDispatch` flag |
| 4 | `2026-05-24-cline-stage4-throughput-hardening.md` | Profiling-gated record-pass scaling (sidecar pool, batching) |

Supporting deliverables produced by the plans: `2026-05-24-cline-provider-matrix.md` (Stage 1, item 5), `2026-05-24-cline-record-throughput-target.md` (Stage 4, measured target).

## Inheritance-contract coverage ledger

Every one of the 10 "Subplan inheritance contract" items is owned by ≥1 stage. No item is unassigned.

| # | Contract item (type) | Owning stage(s) → task(s) |
|---|---|---|
| 1 | Replay determinism (non-negotiable) | **S2** (record half — T2 frame schema, T4 store, T5 capture) + **S3** (replay half — T1–T3 bit-stable) |
| 2 | Failure + recovery (non-negotiable) | **S1** T8 (crash/idempotency/partial) · **S2** T8 (record side) · **S3** T5 (live-vs-replay divergence) · **S4** T5 (pool crash isolation) |
| 3 | Operational visibility (non-negotiable) | **S1** T7 (begins; mode=live) · **S3** T11 (replay metrics) · **S4** T6 (pool health) |
| 4 | Piping + backpressure (non-negotiable) | **S2** T3 (lossless record channel) · **S3** T4 (replay feed + reconstitution) · **S4** T4 (batching at scale) |
| 5 | Provider matrix + compatibility | **S1** T1 (matrix doc + typed abort on gap) |
| 6 | Migration/off-ramp | **S3** T7 (parity gate) + T10 (flag removal behind emergency env off-ramp) |
| 7 | Trajectory identity | **S2** T1 (versioned, collision-resistant key) |
| 8 | A/B pairing | **S3** T6 (fingerprint-driven; preserves shared-intern-briefing) |
| 9 | Retention | **S2** T6 (TTL/compaction/purge + documented cache cutover) |
| 10 | CLI affordances | **S2** T7 (inspect/validate/purge/reindex) + **S3** T8 (record/replay mode select) |

## Cross-stage decisions worth flagging to the conductor

1. **A slot = a Cline `Agent` run, not a `LlmDispatch::complete` swap** (S1). Wrapping Cline inside `LlmDispatch` would nest two tool loops. `execute_slot_cline` returns the same `LlmResponse` shape so downstream parsing is unchanged.
2. **Migration 018 already provides the persistence substrate.** `checkpoints`/`model_calls`/`tool_calls` + the content-addressed blob store (`*_payload_ref`, `retention_mode`) pre-exist; Stage 2 *adds* a versioned `trajectory_frames` table rather than greenfielding.
3. **`model-wrapper.ts` is mock-only today.** Real-provider frame capture is net-new in S2 T5 (the umbrella's "Current state" implied it was closer to done than it is).
4. **FOLLOWUPS.md F21 is already gone** — S0 verifies absence rather than fabricating an edit (umbrella scope list was slightly stale).
5. **Item-6 vs umbrella tension resolved** (S3 T10): the *routine* `LlmDispatch` flag is removed after a parity gate, but a time-boxed, logged, opt-in `XVN_EMERGENCY_LLM_DISPATCH` env off-ramp remains — satisfying both "remove the flag" and "keep an off-ramp."
6. **Stage 4's throughput target is measured, not invented** (S4 T1 sets it; T7 asserts against it). Pool/reuse work is gated on measured need, honoring the umbrella's "may fold into Stage 3 or run as a follow-up."

## Sequencing

Stage 0 is independent and may land first. Stages 1→3 are sequential. Stage 4 is profiling-gated and may fold into Stage 3 or run as a follow-up.
