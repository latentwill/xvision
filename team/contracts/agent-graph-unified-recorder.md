---
track: agent-graph-unified-recorder
lane: foundation
wave: agent-graph-2026-05-22
worktree: .worktrees/agent-graph-unified-recorder
branch: task/agent-graph-unified-recorder
base: origin/main
status: deferred
depends_on:
  - agent-graph-capability-schema    # PR #527 — Phase A
  - agent-graph-capability-dispatch  # Phase B — `dispatch_capability` seam
blocks: []
stacking: declared:agent-graph-capability-dispatch
allowed_paths:
  - crates/xvision-observability/src/recorder.rs       # NEW — `Recorder` trait
  - crates/xvision-observability/src/harness_recorder.rs  # NEW — HarnessRecorder impl
  - crates/xvision-observability/src/eval_recorder.rs     # NEW — EvalRecorder impl
  - crates/xvision-observability/src/lib.rs            # re-export the trait + impls
  - crates/xvision-observability/src/events.rs         # extend AgentEvent variants if needed for capability spans
  - crates/xvision-observability/src/types.rs          # ToolCall, SupervisorNote, Approval, SandboxResult, Checkpoint, Artifact shapes
  - crates/xvision-observability/src/sqlite.rs         # row writers for the 7 tables
  - crates/xvision-engine/src/agent/dispatch_capability.rs  # threads &dyn Recorder through dispatch
  - crates/xvision-engine/src/agent/pipeline.rs        # constructs HarnessRecorder for harness path
  - crates/xvision-engine/src/eval/executor/paper.rs   # constructs EvalRecorder for eval path
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/tests/recorder_symmetry.rs   # NEW — F-11(f) regression test
  - crates/xvision-engine/tests/recorder_trait_basics.rs  # NEW — trait dispatch unit tests
forbidden_paths:
  - crates/xvision-engine/migrations/**           # no new migration
  - crates/xvision-engine/src/agents/model.rs     # Phase A owns
  - crates/xvision-engine/src/strategies/agent_ref.rs  # Phase A owns
  - crates/xvision-engine/src/agent/filter_dispatch.rs  # Phase C owns
  - crates/xvision-engine/src/agent/signal_cache.rs     # Phase C owns
  - crates/xvision-engine/src/agents/templates.rs       # Phase E owns
  - frontend/web/**                               # Phase F owns UI
interfaces_used:
  - xvision_engine::agents::Capability (Phase A)
  - xvision_engine::agent::dispatch_capability::{dispatch_capability, AgentOutput} (Phase B)
  - xvision_observability::ObsEmitter (existing — span emission)
  - SQLite recorder tables: `tool_calls`, `events`, `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`, `artifacts` (already exist; harness path writes to them, eval path does not — that's the F-11(f) bug Phase D closes)
parallel_safe: false
parallel_conflicts:
  - agent-graph-capability-dispatch   # Phase B authors the seam Phase D threads &dyn Recorder through; sequential
  - agent-graph-filter-capability     # Phase C touches dispatch_capability.rs; sequential (Phase D rebases on Phase C)
verification:
  - cargo fmt --check
  - cargo clippy --workspace --tests -- -D warnings
  - cargo test -p xvision-engine --test recorder_symmetry
  - cargo test -p xvision-engine --test recorder_trait_basics
  - cargo test -p xvision-engine
  - cargo test -p xvision-observability
  - cargo build --workspace
acceptance:
  - **`Recorder` trait** defined at `crates/xvision-observability/src/recorder.rs`:
    ```rust
    pub trait Recorder: Send + Sync {
        fn record_tool_call(&self, call: ToolCall);
        fn record_event(&self, event: AgentEvent);
        fn record_supervisor_note(&self, note: SupervisorNote);
        fn record_approval(&self, approval: Approval);
        fn record_sandbox_result(&self, result: SandboxResult);
        fn record_checkpoint(&self, checkpoint: Checkpoint);
        fn record_artifact(&self, artifact: Artifact);
    }
    ```
    All 7 methods take `&self` (not `&mut`); interior mutability via the underlying SQLite connection pool.
  - **`HarnessRecorder`** at `crates/xvision-observability/src/harness_recorder.rs`. Wraps the existing OTel span emission + recorder-table writes from `xvision-observability`'s today-path. Constructor: `HarnessRecorder::new(emitter: Arc<ObsEmitter>, db: Arc<XvnDb>) -> Self`. Behavior is byte-identical to the pre-Phase-D harness emission (regression-tested).
  - **`EvalRecorder`** at `crates/xvision-observability/src/eval_recorder.rs`. Two-channel writer:
    1. Buffers events into the in-memory trace blob (existing eval trace surface).
    2. **Also** writes the same row into the corresponding `xvn.db` recorder table.
    The mirror is the F-11(f) fix: eval-driven runs now produce non-empty rows in all 7 tables. Constructor: `EvalRecorder::new(db: Arc<XvnDb>, trace_buf: Arc<Mutex<TraceBuf>>, run_id: Uuid) -> Self`.
  - **`dispatch_capability` signature change**: takes `recorder: &dyn Recorder` as a parameter, threaded down from the executor / pipeline entry points. Each capability handler in Phases B-C emits through the recorder, never directly to OTel or the trace buffer.
  - **`pipeline.rs`** (harness path) constructs `HarnessRecorder` and passes a `&dyn Recorder` reference into `dispatch_capability`. **`eval/executor/{paper,backtest}.rs`** (eval path) construct `EvalRecorder` and pass it the same way. The two paths are byte-identical from the dispatcher's perspective.
  - **Recorder-symmetry regression test** at `crates/xvision-engine/tests/recorder_symmetry.rs`:
    - Synthetic strategy with one Trader-capable AgentRef + one Filter-capable AgentRef + one Critic-capable AgentRef.
    - Run once through the harness path (capture row counts in each of the 7 tables).
    - Run once through the eval-executor path (capture row counts).
    - Assert: every table that has `> 0` rows from the harness run also has `> 0` rows from the eval run.
    - The test fails today on `main` (F-11(f) reproduction); passes after Phase D.
  - **Trait dispatch unit tests** at `crates/xvision-engine/tests/recorder_trait_basics.rs`:
    - Mock `Recorder` impl that counts method calls.
    - Synthetic dispatch_capability invocation with each capability variant; assert the right `record_*` methods fired the expected number of times per capability kind.
  - **Existing emission code in `xvision-observability` is retained** but routed through the trait — no behavior change on the harness path. The PR adds the trait + the eval implementor; it does not rewrite the harness emission code.
  - **No new migration**. The 7 recorder tables already exist on disk (added by earlier migrations 005/018/020/022 and friends). Phase D only adds writers from the eval path; it does not alter table schemas.
  - **Pre-launch breaking change**: pre-Phase-D code paths that called `ObsEmitter` directly from `dispatch_capability` (placed there as scaffolding in Phase B per Phase B's "Phase D's predecessor F43" interface declaration) are migrated to the trait. Phase D removes those direct calls; the dispatcher only knows about the trait.

---

# Scope

Phase D of `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`. Closes F-11(f) (eval-driven runs produce empty recorder tables) structurally rather than piecemeal.

The fix is not "wire up tool_call emission on the eval path" (which would be the 7th such piecemeal patch and isn't sustainable). The fix is: there is now one entry point — `dispatch_capability` — that BOTH the harness AND the eval executor invoke; that entry point takes `&dyn Recorder`; each surface supplies its own `Recorder` impl. Code paths that emit to one surface and not the other become structurally impossible.

The single regression test pins this: harness and eval surfaces produce symmetric row counts in all 7 tables.

# Out of scope

- New event kinds. Phase D ships the trait shape and the eval implementor; new event kinds (Filter span shape, Critic span shape, Router span shape) land in Phases C / E as those capabilities flesh out.
- UI changes to the trace dock or eval review surface. The frontend already reads from the recorder tables (post #524 trace-dock-emitters); Phase D just makes the rows exist on the eval path.
- Migration of the legacy harness path off `ObsEmitter`. The harness's existing OTel + DB-write path is preserved verbatim; `HarnessRecorder` is a thin wrapper over it. Future cleanup can collapse them, not in this PR.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
# Wait for Phase A (#527) AND Phase B (TBD PR) to merge into origin/main.
# Phase C may or may not be merged; if not, rebase coordination is needed
# for the shared `dispatch_capability.rs` file.
git worktree add .worktrees/agent-graph-unified-recorder \
  -b task/agent-graph-unified-recorder origin/main
```

Set per-worktree target dir:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-unified-recorder"
```

# Iterative verification loop

```bash
# 1. Confirm F-11(f) reproduces on the pre-Phase-D base
cargo test -p xvision-engine --test recorder_symmetry 2>&1 | tee /tmp/symmetry-before.log
# Expect: the assertion fails on at least one of the 7 tables.

# 2. Author the trait, the two implementors, thread `&dyn Recorder`.
# 3. Re-run:
cargo test -p xvision-engine --test recorder_symmetry 2>&1 | tee /tmp/symmetry-after.log
# Expect: passes.

# 4. Full suites
cargo test -p xvision-engine
cargo test -p xvision-observability
cargo build --workspace
cargo clippy --workspace --tests -- -D warnings
```

# Notes

- Phase B's contract notes "F43 emitters" — that's PR #524 (`trace-dock-emitters`, merged 2026-05-22). Phase B can call into `ObsEmitter` directly as scaffolding for emissions; Phase D's job is to remove those direct calls and route everything through `&dyn Recorder`. The trait method signatures mirror the existing `ObsEmitter` methods on a one-to-one basis to keep the migration mechanical.
- The 7 recorder tables: `tool_calls`, `events`, `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`, `artifacts`. Migration provenance: 018 (agent-run-observability) added the core set; 020/022 extended.
- The PR is conceptually large but should be mechanically modest — the existing harness emission already writes to these tables. Phase D mostly threads a trait through and adds the eval-side implementor.
- The trait being `&self` (not `&mut self`) matters: the dispatcher passes the recorder down to multiple async per-capability handlers, sometimes in parallel (Filter granularity may evaluate two filters concurrently). The SQLite write path is internally synchronized via the pool's busy_timeout (post #522).
