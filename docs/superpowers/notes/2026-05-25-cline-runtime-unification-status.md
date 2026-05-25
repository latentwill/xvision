# Cline Runtime Unification — Build Status / Forklift Handoff

**Date:** 2026-05-25
**Branch:** `feat/cline-runtime-unification` (43 commits ahead of `origin/main`, **unpushed**)
**Worktree:** `.worktrees/cline-impl`
**Spec:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`
**Plans:** `docs/superpowers/plans/2026-05-24-cline-stage{0..4}-*.md`

All five stages are implemented, integrated (zero merge conflicts), and green.
Built by a parent + 9 subagent worktrees (3 waves), each track merged back with
disjoint file ownership.

## Verification (integrated tree, post-merge)

- **Rust:** `cargo check --workspace` clean (pre-existing warnings only).
  - agent-client lib **32** · observability (trajectory + batching + run-replay-metrics) **~20** · core agent_runtime **2** · eval lib **133** + no-BriefingCache guard **2**
  - engine integration targets: `cline_execute_slot` 7 · `cline_pipeline_flag` 3 · `cline_observability_live` 3 · `cline_replay_bitstable` 5 · `cline_parity_gate` 3 · `llm_dispatch_offramp` 1 · `pool_crash_recovery` 4 · `trajectory_store_roundtrip` 5 · `trajectory_retention` 4 · `trajectory_recovery` 7 · `trajectory_full_reconstruction` 2
- **Node (`xvision-agentd`):** `tsc --noEmit` clean · `vitest` **107/107**
- **Measured throughput (Stage 4):** 5,870 → **7,197 frames/sec** with batching (target 1,667 for a 500-cycle/30s backtest; 0 dropped). See `docs/superpowers/specs/2026-05-24-cline-record-throughput-target.md`.

## What landed, by stage

- **Stage 0 — ACPX purge + license guard.** `scripts/guard-no-acpx.sh` (in `board-lint`); vitest `license-guard` locks API-key-only auth.
- **Stage 1 — live Cline path.** `provider_map` (xvision→Cline gateway ids), `AgentRuntime` flag (default **Cline**), `submit_decision` lifecycle tool (local capture, `lifecycle.completesRun`), `decision_json`/`decision_schema` on the protocol, `execute_slot_cline` (slot = a Cline `Agent` run; returns the decision as a `ContentBlock::Text` so the existing capability parser is unchanged), pipeline flag-branch, `trajectory_mode` migration **039**, failure/recovery (crash→typed error, run_id dedup).
- **Stage 2 — trajectory record.** `TrajectoryKey` (versioned fingerprint, step_index frame-level only), `TrajectoryFrame` (8 variants, Rust + `frame-types.ts` mirror), lossless backpressured `FrameChannel`, `TrajectoryStore` + migration **040**, retention (TTL/purge/compact), `xvn trajectory inspect/validate/purge/reindex`. Node: real-provider model is now built via `Llms.createGateway()` + `configureProvider()` + `createAgentModel()` and wrapped by `model-wrapper` (the old "deferred gateway plumbing" blocker is RESOLVED), `frame-recorder` + `tool-shim` ToolResult capture.
- **Stage 3 — replay + unify eval.** `replay-model.ts` (generalizes mock-provider), `session.replay_load` RPC (Node + agent-client), replay branch in `execute_slot_cline` (bit-stable; no network), divergence + frame-exhaustion → recording `corrupt` + `recovery_reason`, fingerprint-driven A/B pairing, **`BriefingCache` deleted** (determinism now via trajectory replay), `ab-compare --record/--replay`, parity gate, emergency off-ramp. Dashboard: inline `TrajectoryModeBadge` (live/record/replay + hit-ratio/dropped/recovery, no popups).
- **Stage 4 — throughput hardening.** `SidecarPool` (lease/respawn/crash-isolation), batched frame writes, profiling harness.

## Known follow-ups / honest gaps (read before forklift)

1. **Live record→sidecar wiring is NOT complete.** The record *component*
   (`TrajectoryFramePersister` in `event_sink.rs`) and the replay *core*
   (`execute_slot_cline` replay branch) are done and tested **via direct store
   seeding**. Threading a live `TrajectoryStore` + active `RecordingId` through
   `AgentClient::spawn_with_event_sink` into `api/eval.rs` so a *real* Cline run
   persists frames end-to-end is the remaining integration — it changes `spawn_*`
   signatures and touches the `RunEvent` vocabulary. Until then, production
   recording from a live sidecar run is not wired; replay works off seeded/recorded
   stores.
2. **Migration `040_trajectory_frames` is applied by the `TrajectoryStore`'s own
   pool**, not the main API-DB migrator in `api/mod.rs`. Wherever the store's pool
   is initialized in production must run 040 (folds into #1).
3. **`BriefingReplay` is in-memory** (fingerprint-keyed), not SQLite-backed — the
   backtest harness runs in-process with no SQLite handle threaded in. Determinism
   and A/B pairing are identical to the persistent store; only the backend differs.
   Swapping to the persistent store is a follow-up (also folds into #1).
4. **Eval Cline-sidecar spawn path is not unit-tested** — it needs the built
   `xvision-agentd` (`dist/`) + `XVN_AGENTD_BIN`. The slot/pipeline/replay logic it
   drives is fully covered via a pure-Node **mock sidecar** fixture
   (`tests/fixtures/mock_agentd.js`).
5. **Pool/crash tests use stand-ins** (MockSlot/MockSidecar) — real-process kill
   isn't exercised in-test (no Node binary in the Rust test env). The pool logic
   (semaphore, status, respawn, restart count) is fully covered.
6. **Provider matrix doc not written** (Stage 1 Task 1 deliverable). The mapping
   itself lives in `crates/xvision-agent-client/src/provider_map.rs` with rationale.
7. **Dashboard:** 14 pre-existing frontend test failures are unrelated to this work
   (existed on the base). The new `TrajectoryModeBadge` tests (9) pass.
8. **Pre-existing, unrelated:** the `strategy_clone_model` engine test target fails
   to compile (`PublicManifest` missing `capital_mode`/`execution_mode` from another
   in-flight track on `origin/main`). Not touched by this branch; it blocks
   `cargo test -p xvision-engine` from building *all* test targets, so use specific
   `--test <name>` targets. `cargo check --workspace` is clean.

## To run the live Cline path (production)

1. Build the sidecar: `cd xvision-agentd && pnpm install && pnpm build` (→ `dist/index.js`).
2. Set `XVN_AGENTD_BIN=/abs/path/to/xvision-agentd/dist/index.js`.
3. `agent_runtime` defaults to `cline`. Emergency rollback to the legacy path:
   `XVN_EMERGENCY_LLM_DISPATCH=1` (logs a loud warn; see MANUAL.md "Emergency rollback").

## Forklift checklist

- [ ] Review the 43 commits (`git log origin/main..feat/cline-runtime-unification`).
- [ ] Decide push/PR strategy (branch is local + unpushed).
- [ ] Before relying on live recording, wire follow-up #1 (+ #2, #3).
- [ ] `pnpm build` the sidecar and set `XVN_AGENTD_BIN` on the target.
