# Cline Runtime Unification — Stage 3: Replay + Unify Eval — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make backtests and A/B compares deterministic and faithful by replaying recorded trajectories through the same Cline runtime that drives live — retiring the in-memory `BriefingCache`, with a parity-gated removal of the `LlmDispatch` path and a documented emergency rollback.

**Architecture:** A *replay model* (generalization of `mock-provider.ts`'s `buildMockModel()`) loads a recorded trajectory's frames and feeds them back to the Cline `Agent` as its `AgentModel`. The Agent re-runs its loop, makes the same tool calls, and produces the same decision with zero network cost. A/B pairing falls out of the `TrajectoryKey` fingerprint: arms sharing slot identity (provider+model+prompt) share one recording (preserving today's shared-intern-briefing behavior); arms differing on a slot get per-arm recordings. `BriefingCache` and the deterministic intern backends are deleted. The `LlmDispatch` flag is removed from the routine path only after a parity gate proves Cline-record == LlmDispatch decisions; an env-gated emergency rollback remains per the off-ramp contract.

**Tech Stack:** TypeScript/Vitest (`xvision-agentd`), Rust (`xvision-eval`, `xvision-engine`, `xvision-observability`, `xvision-cli`).

**Umbrella spec:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md` (Stage 3 + "Subplan inheritance contract"). **Builds on Stages 1–2.**

---

## Inherited contract gates (from umbrella §"Subplan inheritance contract")

- [ ] **Item 1 — Replay determinism (replay half, non-negotiable).** Stage 3 replays full step state — raw frames, tool payloads, tool results/errors, retry/cancel, timestamps, budgets/counters — re-executing the loop (not memoizing the final decision). A replay re-run is bit-stable.
- [ ] **Item 2 — Live-vs-replay divergence (the piece deferred from Stage 1, non-negotiable).** When a replayed Agent would make a different tool call / take a different branch than the recorded trajectory, detect it, abort, set `recovery_reason = replay_divergence`, and surface it. Never silently hide divergence.
- [ ] **Item 4 — Piping + backpressure (replay side, non-negotiable).** Specify the replay frame-feed schema, bounds, and the reconstitution rule for an exhausted/missing frame (→ recording marked corrupt, abort — no silent partial replay).
- [ ] **Item 6 — Migration/off-ramp (must-have).** A parity gate proves Cline-record == `LlmDispatch` before the flag is removed; an env-gated emergency rollback to `LlmDispatch` remains, with blast-radius limited to opt-in and documented.
- [ ] **Item 8 — A/B pairing (must-have).** Define and preserve A/B semantics under trajectories: shared-slot recordings are reused across arms; arm-specific slots are per-arm. Current shared-intern-briefing behavior is preserved exactly.
- [ ] **Item 10 — CLI affordances (Stage 3 half, must-have).** Record/replay mode selection per run (`xvn ab-compare --record` / `--replay <recording_id>`).

Stage 3 exit (umbrella): *an A/B compare over recorded cycles is bit-stable across re-runs and exercises the identical control flow as live; old cache deleted.*

---

## File Structure

- Create (Node): `xvision-agentd/src/session/replay-model.ts` — `buildReplayModel(frames)` (generalizes `mock-provider.ts`).
- Modify (Node): `xvision-agentd/src/session/build-agent.ts` (replay branch), `src/methods/session.ts` (replay load RPC).
- Modify: `crates/xvision-agent-client/src/protocol.rs` + `client.rs` — `replay.load` / `StartRunParams { replay_recording_id }`.
- Modify: `crates/xvision-engine/src/agent/execute_cline.rs` — replay branch + divergence detection.
- Modify: `crates/xvision-eval/src/ab_compare.rs`, `src/baselines/trader_arm.rs` — route through Cline-replay; delete `BriefingCache` usage.
- Delete: `crates/xvision-intern/src/cache.rs` (and its module decl) after cutover.
- Create: `crates/xvision-engine/tests/cline_parity_gate.rs` — the item-6 parity proof.
- Modify: `crates/xvision-cli/src/commands/...` (ab-compare `--record`/`--replay`), `crates/xvision-core/src/config.rs` (emergency rollback env).
- Modify: `frontend/web/src/features/agent-runs/*` — surface `replay_hit_ratio`, divergence `recovery_reason` (fields declared in Stage 1).

---

### Task 1: Replay model (generalize mock-provider) — Node

**Files:**
- Create: `xvision-agentd/src/session/replay-model.ts`
- Test: `xvision-agentd/test/session/replay-model.test.ts`

- [ ] **Step 1: Failing Vitest** — given a recorded frame sequence (one step: `Request`, `TextDelta×2`, `ToolCallDelta`, `Usage`, `Finish`), `buildReplayModel(frames).stream(request)` yields exactly the recorded `AgentModelEvent`s in order, with no network call.

```typescript
import { describe, it, expect } from "vitest"
import { buildReplayModel } from "../../src/session/replay-model.js"

describe("replay model", () => {
  it("replays recorded frames as AgentModelEvents in order", async () => {
    const frames = [
      { kind: "Request", tsMs: 1, messages: [], tools: [], systemPrompt: "x" },
      { kind: "TextDelta", tsMs: 2, text: "he" },
      { kind: "ToolCallDelta", tsMs: 3, toolName: "submit_decision", input: { action: "buy" } },
      { kind: "Usage", tsMs: 4, inputTokens: 10, outputTokens: 2, cacheReadTokens: 0, cacheWriteTokens: 0, totalCost: 0 },
      { kind: "Finish", tsMs: 5, reason: "stop" },
    ]
    const model = buildReplayModel(frames as any)
    const out: string[] = []
    for await (const ev of await model.stream({ messages: [] } as any)) out.push(ev.type)
    expect(out).toEqual(["text-delta", "tool-call-delta", "usage", "finish"])
  })
})
```

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — `buildReplayModel(frames)` returns an `AgentModel` whose `stream()` converts non-`Request` frames back into `AgentModelEvent`s and yields them. It maintains a per-step cursor exactly like `mock-provider.ts` advances per `stream()` call, but reads from recorded frames instead of a script. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): replay model from recorded frames (item 1)`.

---

### Task 2: Replay-load bridge (Rust store → sidecar)

**Files:**
- Modify: `crates/xvision-agent-client/src/protocol.rs` (`StartRunParams { replay_recording_id: Option<String> }`, `ReplayLoadParams { frames: Vec<serde_json::Value> }`)
- Modify: `crates/xvision-agent-client/src/client.rs` (`replay_load`), `xvision-agentd/src/methods/session.ts` (`session.replay_load`), `src/session/build-agent.ts` (use replay model when frames loaded)
- Test: `xvision-agentd/test/session/replay-load.test.ts`

- [ ] **Step 1: Failing test** — `session.replay_load` accepts frames, then `session.step` runs the agent against `buildReplayModel(frames)` and returns the recorded decision with `usage` summed from `Usage` frames (no provider call).

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — Rust reads frames from `TrajectoryStore.read_frames` (Stage 2), sends them via `replay_load`; `build-agent.ts` branches: if a replay frame set is loaded for the run, construct `new Agent({ model: buildReplayModel(frames), systemPrompt, tools })` (same shape as the mock-provider branch). **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): replay-load bridge store→sidecar`.

---

### Task 3: Replay branch in `execute_slot_cline` + bit-stability (item 1)

**Files:**
- Modify: `crates/xvision-engine/src/agent/execute_cline.rs`
- Test: `crates/xvision-engine/tests/cline_replay_bitstable.rs`

- [ ] **Step 1: Failing test** — record a slot once; replay it twice; assert both replays produce a **byte-identical** `LlmResponse` (same decision JSON, same token counts) and that no provider HTTP call occurs (assert via a no-network guard / mock provider that panics if called during replay).

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — `execute_slot_cline` takes a `TrajectoryMode { Record, Replay { recording_id } }`. In `Replay`, call `read_frames` → `replay_load` → `step`; the Agent re-runs its loop driven by replayed frames. Mark the run's `trajectory_mode = "replay"` and increment `replay_hit_ratio` numerator. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): replay branch — bit-stable re-runs (item 1)`.

---

### Task 4: Replay frame feed — bounds + reconstitution (item 4 replay side)

**Files:**
- Modify: `xvision-agentd/src/session/replay-model.ts`, `crates/xvision-engine/src/agent/execute_cline.rs`
- Test: `xvision-agentd/test/session/replay-exhaustion.test.ts` + `crates/xvision-engine/tests/cline_replay_corrupt.rs`

- [ ] **Step 1: Failing tests** — (a) if the Agent requests more turns than recorded frames provide (frame exhaustion), the replay model raises a typed `ReplayExhausted` error rather than hanging or fabricating output; (b) Rust maps that to the recording being marked `corrupt` with `recovery_reason = replay_frames_exhausted` and the cycle failing.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — define the replay feed schema (the recorded frame list, ordered, finite); bound it to the recorded length; on exhaustion throw `ReplayExhausted`. Reconstitution rule: a missing/exhausted frame **never** triggers a live provider call — the recording is corrupt and the run aborts. Document this alongside Stage 2's lossless-frame rule. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): bounded replay feed + reconstitution rule (item 4)`.

---

### Task 5: Live-vs-replay divergence detection (item 2)

**Files:**
- Modify: `crates/xvision-engine/src/agent/execute_cline.rs`, `xvision-agentd/src/session/replay-model.ts`
- Test: `crates/xvision-engine/tests/cline_replay_divergence.rs`

- [ ] **Step 1: Failing test** — record a trajectory; then replay it but inject a tool whose result differs from the recorded `ToolResult` (simulating a non-deterministic tool / changed environment), forcing the Agent down a different branch than recorded. Assert: detected, run aborts, `recovery_reason = replay_divergence`, and the divergence point (slot, step, frame index) is reported.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — divergence detection must compare the runtime's actual control flow against the recorded transcript, not just replayed model output against itself. During replay:
  - compare each actual tool call Cline requests against the next recorded `ToolCallDelta`;
  - execute the real tool or deterministic replay tool according to the selected replay policy, then compare the actual tool output/error against the recorded `ToolResult`;
  - compare the next model `Request` frame Cline constructs (messages/tools/system prompt) against the recorded `Request` for that step before yielding the next recorded model events.
  On any mismatch, raise `ReplayDivergence { recording_id, slot, step, frame_index, expected, actual }`; Rust marks `recovery_reason = replay_divergence` and surfaces it (UI field from Stage 1). This avoids the false-green case where a replay model simply yields the recorded `ToolCallDelta` and therefore "matches" itself while changed tool results or message reconstitution drift silently. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): replay divergence detection (item 2 deferred piece)`.

---

### Task 6: A/B pairing under trajectories (item 8)

**Files:**
- Modify: `crates/xvision-eval/src/ab_compare.rs`, `src/baselines/trader_arm.rs`
- Test: `crates/xvision-eval/src/ab_compare.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Failing test** — two `Trader` arms with the **same** intern provider/model but **different** trader models, over one cycle:
  - assert the intern slot resolves to **one** recording (shared `TrajectoryKey.fingerprint()` with `arm_scope = None` when provider/model/prompt match) → preserves today's shared-intern-briefing pairing;
  - assert the trader slot resolves to **two** recordings (one per arm, because the model differs) → arm-specific.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — derive each slot's `TrajectoryKey` from `(cycle_id, slot_role, provider, model, prompt_hashes, simulation_id, arm_scope)`. `arm_scope` is `None` for a shared slot and `Some(arm_id)` for a per-arm slot. Concretely: compute the candidate key with `arm_scope = None`; if two arms produce the same slot identity, they share that recording. If the slot identity differs, use `arm_scope = Some(arm_id)` so each arm records/replays independently. `RecordingId` remains separate and is never part of the fingerprint. Document the three modes (shared-briefing / shared-slot / per-arm per-slot) in the function doc and state which xvision uses (shared-slot, fingerprint-driven). **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): fingerprint-driven A/B pairing preserves shared-briefing (item 8)`.

---

### Task 7: Parity gate (item 6) — Cline-record == LlmDispatch

**Files:**
- Create: `crates/xvision-engine/tests/cline_parity_gate.rs`

- [ ] **Step 1: Write the parity test** — over a fixed set of recorded cycles, run each slot through (a) `LlmDispatch` and (b) Cline-record against the **same** provider/model and assert the resulting `TraderDecision`s are equal within the documented tolerance (exact for structured fields like `action`; numeric fields within an explicit epsilon). The test is the gate: it must be green before Task 8 removes the routine flag.

- [ ] **Step 2: Run — observe.** If decisions differ, the divergence is a real bug in the Cline path — fix it (do not loosen the tolerance to pass). Document the tolerance + rationale in the test header.

- [ ] **Step 3: Commit** `test(stage3): LlmDispatch vs Cline parity gate (item 6)`.

---

### Task 8: CLI record/replay mode (item 10, Stage 3 half)

**Files:**
- Modify: the `AbCompare` command args + `crates/xvision-eval` entry; `crates/xvision-cli/src/lib.rs`
- Test: CLI test following the `ab-compare` convention

- [ ] **Step 1: Failing test** — `xvn ab-compare --record` records trajectories for the run; `xvn ab-compare --replay <recording_id>` (or `--replay` reusing the latest recording for the cycle set) replays with no network and is bit-stable; default (neither flag) preserves prior behavior during the transition.

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — add a `--record` / `--replay <id>` mutually-exclusive flag pair mapping to `TrajectoryMode`. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): ab-compare --record/--replay mode select (item 10)`.

---

### Task 9: Retire `BriefingCache` + deterministic intern backends (cutover)

**Files:**
- Modify: `crates/xvision-eval/src/ab_compare.rs` (remove `Arc<BriefingCache>` construction + clones), `src/baselines/trader_arm.rs` (remove `cache.get/insert`; use replay)
- Delete: `crates/xvision-intern/src/cache.rs` + its `mod cache;` declaration
- Test: update affected eval tests; add a guard test

- [ ] **Step 1: Failing/guard test** — assert `ab_compare` compiles and runs A/B with no reference to `BriefingCache` (a grep guard `! git grep -q BriefingCache -- crates/` after deletion), and that the existing A/B determinism tests still pass using replay instead of the cache.

- [ ] **Step 2: Run — FAIL** (cache still referenced). **Step 3: Implement** — replace the cache read/write in `trader_arm.rs` with a trajectory record-on-first-pass / replay-on-rerun; delete `cache.rs`; remove the deterministic-intern-backend special-casing now that determinism comes from replay (item: "retire the in-memory `BriefingCache` and the deterministic intern backends"). **Step 4: Run — PASS;** grep guard green. **Step 5: Commit** `refactor(stage3): delete BriefingCache; determinism via replay`.

---

### Task 10: Remove the routine `LlmDispatch` flag; keep emergency off-ramp (item 6)

**Files:**
- Modify: `crates/xvision-core/src/config.rs`, `crates/xvision-engine/src/agent/pipeline.rs`, `crates/xvision-engine/src/api/eval.rs`
- Test: `crates/xvision-engine/tests/llm_dispatch_offramp.rs`

> **Gate:** Task 7's parity test must be green before starting this task.

- [ ] **Step 1: Failing test** — assert that the routine runtime is Cline (no per-config `runtime` selection needed for normal operation), AND that setting the documented emergency env var (`XVN_EMERGENCY_LLM_DISPATCH=1`) still routes through `LlmDispatch` for incident rollback, with a loud `warn!` log naming the blast radius (this process only, opt-in).

- [ ] **Step 2: Run — FAIL.** **Step 3: Implement** — drop `AgentRuntime` from the normal config surface so Cline is the unconditional path; keep `LlmDispatch` reachable **only** via `XVN_EMERGENCY_LLM_DISPATCH=1`, documented in `MANUAL.md` as an emergency rollback with a stated removal-after-bake-in date. This satisfies both the umbrella ("remove the flag") and item 6 ("keep an off-ramp"): the routine flag is gone; the emergency path is explicit, logged, and time-boxed. **Step 4: Run — PASS.** **Step 5: Commit** `feat(stage3): Cline is the unconditional runtime; LlmDispatch behind emergency env off-ramp (item 6)`.

---

### Task 11: Surface replay metrics in the dashboard (item 3 completion)

**Files:**
- Modify: `frontend/web/src/features/agent-runs/RunStatusStrip.tsx`, `SpanInspector.tsx`, `frontend/web/src/api/types-agent-runs.ts`
- Test: frontend typecheck + a render test if the suite has one

- [ ] **Step 1:** populate the Stage-1-declared fields — `replay_hit_ratio`, `dropped_events`, `recovery_reason` — in the run summary API and render them inline (status strip + span inspector). Mode badge shows `replay`/`live`/`record`. **No popups** — inline only.
- [ ] **Step 2: Run** `cd frontend/web && npm run typecheck`; expected clean. **Step 3: Commit** `feat(stage3): surface replay-hit-ratio + divergence in agent-runs UI`.

---

### Task 12: Exit gate — bit-stable A/B over recorded cycles

**Files:**
- Test: `crates/xvision-eval/tests/ab_compare_replay_bitstable.rs` (or inline)

- [ ] **Step 1: Test** — record an A/B compare over N cycles, then replay it twice; assert the full per-arm result set (decisions + realized PnL series) is byte-stable across both replays and that the control flow matches live (same slots executed in the same order). Confirm `crates/xvision-intern/src/cache.rs` no longer exists. **Step 2: Run — PASS.** **Step 3: Commit** `test(stage3): bit-stable A/B replay exit gate`.

---

## Self-Review

- **Spec coverage (Stage 3 scope):** build replay model (Task 1 ✓), route backtest + A/B through Cline-with-replay (Tasks 2–4, 6, 8 ✓), retire `BriefingCache` + deterministic intern backends (Task 9 ✓), remove `LlmDispatch` flag (Task 10 ✓). Exit = bit-stable A/B + identical control flow + cache deleted (Task 12 ✓).
- **Item 1 (replay half) ✓** Tasks 1–3 re-execute the loop (not decision memoization) and are bit-stable.
- **Item 2 (divergence) ✓** Task 5 — the piece explicitly deferred from Stage 1.
- **Item 4 (replay side) ✓** Task 4 — bounds + reconstitution; no silent live fallback.
- **Item 6 ✓** Tasks 7 + 10 — parity gate precedes flag removal; emergency env off-ramp with blast-radius + time-box preserved (resolves the umbrella-vs-item-6 tension).
- **Item 8 ✓** Task 6 — fingerprint-driven pairing preserves shared-intern-briefing exactly; three modes documented.
- **Item 10 (Stage 3 half) ✓** Task 8.
- **Placeholder scan:** parity tolerance is an explicit, documented decision (Task 7), not a vague "within reason." Cache deletion verified by grep guard (Task 9).
- **Type consistency:** `TrajectoryMode { Record, Replay }` (Task 3) is the same type threaded through Tasks 4, 8; `recovery_reason` values (`replay_frames_exhausted`, `replay_divergence`) are used identically in Tasks 4–5 and the UI in Task 11.
- **No-cargo discipline:** all `cargo test` steps run from a worktree with a per-stage `CARGO_TARGET_DIR`.
