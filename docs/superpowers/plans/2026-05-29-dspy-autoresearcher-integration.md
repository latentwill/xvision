# DSPy × Autoresearcher Integration Implementation Plan

> **For agentic workers:** REQUIRED: Use `superpowers:subagent-driven-development` (if subagents available) or `superpowers:executing-plans` to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make xvision’s autoresearcher consume `xvision-dspy` as a real optimizer primitive: expand the capability surface beyond the current trader/filter stubs, wire a real capability-driven GEPA/MIPRO/COPRO execution path, preserve offline isolation, and expose the resulting optimizer artifacts through the autoresearcher/dashboard/MCP surfaces.

**Architecture:** Keep `xvision-dspy` as a leaf, offline-only optimizer crate and keep `xvision-engine` free of `dspy-rs`. Expand the optimizer’s capability catalog and snapshot/provenance model first, then wire autoresearcher to use those artifacts instead of only the current memory-distillation path. The runtime keeps doing the durable storage / review / acceptance work; the optimizer stays out of the live decision path and only produces replayable artifacts.

**Tech Stack:** Rust, `dspy-rs`, SQLite, CLI + dashboard routes, MCP tools, markdown docs, and the existing xvision optimization store / autoresearch surface.

---

## Why this plan exists

The current implementation is only partially aligned with the research doc.

### What already exists
- `crates/xvision-dspy/` is a real leaf crate with deterministic CI support (`DummyLM`), snapshot serialization, and optimizer-store provenance.
- The CLI and engine already know how to persist optimization runs, candidates, snapshots, and lineage without putting `dspy-rs` into the runtime crate graph.
- Autoresearcher already has a minimal offline surface (`crates/xvision-engine/src/api/autoresearch.rs`, dashboard flywheel routes, MCP read tools) for memory → pattern distillation.

### What the research doc still asks for
- GEPA-style reflective mutation with rich feedback, not just a plain instruction string.
- Capability-first optimization across the full agent surface, not just `trader` / `filter`.
- `decision_grader`, `intern`, and `chat_authoring` as first-class optimizer targets.
- Autoresearcher as a consumer of the optimizer primitive, not a separate memory-only workflow.
- Metric families for strategy agents *and* chat-rail / artifact-producing agents.

### What should *not* be reinvented
- Do **not** add `dspy-rs` to `xvision-engine`, dashboard runtime crates, or the slim deploy image.
- Do **not** replace the current memory-distillation API; it stays as a corpus-building / staging helper.
- Do **not** create a second approval system or hide optimizer decisions behind a new parallel orchestration layer.
- Do **not** let the optimizer run inside a live decision cycle.

---

## Gap matrix

- **Already built:** offline optimizer crate, deterministic test model, optimizer store, CLI, read-only dashboard surfaces.
- **Partially built:** capability-first design exists in concept, but only `trader` and `filter` have real signatures; `decision_grader`, `intern`, and `chat_authoring` are stubs.
- **Still open:** real provider-backed bridge, rich reflective feedback, holdout discipline, autoresearcher consumption of optimizer artifacts, chat-rail metric family.
- **Out of scope:** live trading-path changes, chain / marketplace coupling, hidden orchestration, any runtime `dspy-rs` dependency.

---

## Files to touch

### DSPy optimizer core
- Modify: `crates/xvision-dspy/src/lib.rs`
- Modify: `crates/xvision-dspy/src/capability.rs`
- Modify: `crates/xvision-dspy/src/signatures.rs`
- Modify: `crates/xvision-dspy/src/adapter.rs`
- Modify: `crates/xvision-dspy/src/snapshot.rs`
- Modify: `crates/xvision-dspy/src/error.rs`
- Test: `crates/xvision-dspy/tests/foundation.rs`

### Engine / CLI persistence and orchestration
- Modify: `crates/xvision-engine/src/optimization/mod.rs`
- Modify: `crates/xvision-engine/src/api/optimize.rs`
- Modify: `crates/xvision-cli/src/commands/optimize.rs`
- Test: `crates/xvision-engine/src/optimization/tests.rs`
- Test: `crates/xvision-cli/tests/optimize.rs` or the existing CLI test harness if that is where optimize coverage already lives

### Autoresearcher integration surface
- Modify: `crates/xvision-engine/src/api/autoresearch.rs`
- Modify: `crates/xvision-dashboard/src/routes/flywheel.rs`
- Modify: `crates/xvision-mcp/src/tools.rs`
- Modify: `crates/xvision-dashboard/src/routes/optimizations.rs` if optimizer review needs to expose the new autoresearch-backed artifacts

### Docs / discoverability
- Modify: `crates/xvision-dashboard/wiki/optimizer.md`
- Modify: `crates/xvision-dashboard/wiki/xvision.md`
- Modify: `docs/superpowers/plans/2026-05-29-dspy-autoresearcher-integration.md` (this file, as the source of truth for the implementation plan)

---

## Task 1: Expand the optimizer capability surface to cover the research doc’s full set

**Files:**
- Modify: `crates/xvision-dspy/src/capability.rs`
- Modify: `crates/xvision-dspy/src/signatures.rs`
- Modify: `crates/xvision-dspy/src/error.rs`
- Modify: `crates/xvision-dspy/src/lib.rs`
- Test: `crates/xvision-dspy/tests/foundation.rs`

- [ ] **Step 1: Write the failing capability tests**

Add tests that prove the current behavior is incomplete:
- `signature_for(Capability::DecisionGrader)` still returns `MissingCapabilityOptimizer`
- `signature_for(Capability::Intern)` still returns `MissingCapabilityOptimizer`
- `signature_for(Capability::ChatAuthoring)` still returns `MissingCapabilityOptimizer`
- `Capability::has_optimizer()` is still only true for the existing two capabilities

- [ ] **Step 2: Define the missing signatures as real, typed optimizer targets**

Implement the signatures the research doc calls for:
- `decision_grader` — inputs: cycle summary / rubric; outputs: score + critique
- `intern` — inputs: task + raw context; outputs: compressed digest/context summary
- `chat_authoring` — inputs: user request + app state; outputs: reply / artifact text

Keep the field types primitive where possible so the signatures remain cheap to serialize and easy to validate.

- [ ] **Step 3: Update the capability metadata and remediation text**

Make the capability enum and error messages reflect that the capability set is broader than trader/filter. The error surface should still fail fast for genuinely unsupported cases, but the “not yet implemented” path should disappear for the new first-class capabilities.

- [ ] **Step 4: Re-run the optimizer crate tests**

Run:
```bash
cargo test -p xvision-dspy
```

Expected: the capability tests pass, and the crate still stays offline-only.

---

## Task 2: Replace the stubbed live model path with a real provider bridge without breaking offline isolation

**Files:**
- Modify: `crates/xvision-dspy/src/adapter.rs`
- Modify: `crates/xvision-dspy/src/lib.rs`
- Modify: `crates/xvision-cli/src/commands/optimize.rs`
- Modify: `crates/xvision-engine/src/optimization/mod.rs`
- Test: `crates/xvision-dspy/tests/foundation.rs`
- Test: `crates/xvision-cli/tests/optimize.rs` or existing optimize command tests

- [ ] **Step 1: Write the failing bridge tests**

Add tests that show the current live path is only a stub and that the optimizer still treats provider-backed provenance as incomplete.

- [ ] **Step 2: Implement the real model bridge at the correct seam**

Follow the research-doc seam carefully:
- keep `dspy-rs` as the optimizer engine
- bridge through the model adapter layer
- keep the runtime decision path untouched
- preserve provider/model identity and token/cost provenance per call

If the live provider path is not ready in code yet, keep it feature-gated, but do not let the public API pretend that a real bridge exists when it does not.

- [ ] **Step 3: Make optimizer provenance richer and reproducible**

Ensure the optimizer run record captures:
- provider / model identity
- optimizer kind and version
- signature hash
- metric name
- RNG seed
- demo-set lineage / content hash
- reflective feedback text when available

- [ ] **Step 4: Verify the CLI still uses the deterministic default path**

Run:
```bash
cargo test -p xvision-cli optimize
```

Expected: deterministic tests still pass, and the live provider path remains opt-in.

---

## Task 3: Add holdout discipline and reflective feedback to the optimizer store

**Files:**
- Modify: `crates/xvision-engine/src/optimization/mod.rs`
- Modify: `crates/xvision-dspy/src/snapshot.rs`
- Modify: `crates/xvision-cli/src/commands/optimize.rs`
- Modify: `crates/xvision-dashboard/wiki/optimizer.md`
- Test: `crates/xvision-engine/src/optimization/tests.rs`

- [ ] **Step 1: Write failing tests for holdout enforcement**

Prove the current store still allows train-only selection to look “done” without a holdout confirmation.

- [ ] **Step 2: Persist split and feedback data explicitly**

The optimizer artifacts should carry enough information to answer:
- what was trained on
- what was held out
- what feedback the reflective optimizer saw
- what metric / rubric was optimized
- what text was accepted or rejected and why

Do not bury this in ad hoc JSON blobs if a typed field is clearer and cheaper to inspect later.

- [ ] **Step 3: Enforce holdout before accept**

Acceptance must require the holdout result to exist and be visible in the stored artifact. A run that only has train-side evidence is not a trusted child candidate.

- [ ] **Step 4: Update the optimizer wiki to match the enforced discipline**

The docs should say, plainly, that a candidate is tuned on train and confirmed on holdout before acceptance. No soft wording.

- [ ] **Step 5: Re-run the optimization-store tests**

Run:
```bash
cargo test -p xvision-engine optimization
```

Expected: store tests enforce the split discipline and still round-trip existing snapshots.

---

## Task 4: Make autoresearcher consume optimizer artifacts instead of only memory-distillation outputs

**Files:**
- Modify: `crates/xvision-engine/src/api/autoresearch.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`
- Modify: `crates/xvision-dashboard/src/routes/flywheel.rs`
- Modify: `crates/xvision-mcp/src/tools.rs`
- Modify: `crates/xvision-engine/src/optimization/mod.rs`
- Modify: `crates/xvision-dashboard/src/routes/optimizations.rs` if needed for the review surface

- [ ] **Step 1: Write the failing autoresearch integration tests**

Add tests that show the current autoresearch surface still only:
- selects Observation rows
- promotes them into a Pattern
- records an `autoresearch_runs` row

and does **not** yet connect to optimizer runs, snapshots, signatures, capability choice, or reflective feedback.

- [ ] **Step 2: Split “memory distillation” from “optimizer-backed autoresearch”**

Keep the current memory-distillation flow as a corpus-building stage, but add a real optimizer-backed flow that:
- chooses a capability explicitly
- chooses an optimizer explicitly (`mipro`, `gepa`, `copro`)
- binds a metric explicitly
- persists the resulting optimization run / snapshot / lineage link

The new autoresearch path should consume `xvision-dspy` as a primitive instead of duplicating the optimizer loop in `xvision-engine`.

- [ ] **Step 3: Store links between autoresearch runs and optimizer artifacts**

The autoresearch run DTOs should be able to point at:
- optimizer run id
- snapshot id
- capability / metric family used
- split / holdout state
- reviewer-visible provenance

That makes the autoresearch surface an operator view over the optimizer output, not a separate universe.

- [ ] **Step 4: Expose the new flow through dashboard and MCP read paths**

The dashboard and MCP should be able to list and inspect these autoresearch-backed optimizer artifacts without parsing raw JSON by hand.

- [ ] **Step 5: Re-run the autoresearch API tests**

Run the relevant dashboard / engine integration tests for autoresearch routes.

Expected: the new optimizer-backed path is visible and the legacy memory-distillation path still works.

---

## Task 5: Make chat-rail and artifact-quality optimization first-class

**Files:**
- Modify: `crates/xvision-dspy/src/signatures.rs`
- Modify: `crates/xvision-engine/src/api/autoresearch.rs`
- Modify: `crates/xvision-engine/src/optimization/mod.rs`
- Modify: `crates/xvision-dashboard/wiki/optimizer.md`
- Modify: `crates/xvision-dashboard/wiki/xvision.md`

- [ ] **Step 1: Write failing coverage for chat-rail-oriented metrics**

Prove that the current optimizer path still treats chat-rail / artifact-producing work as second-class or implied instead of explicit.

- [ ] **Step 2: Add explicit metric families for artifact-producing agents**

The research doc says chat-rail agents are not just “reply generators”; they produce downstream artifacts. The plan should therefore expose metrics for:
- tool-use accuracy
- schema / format adherence
- instruction following
- token budget adherence
- latency
- judge-graded helpfulness
- downstream artifact quality

- [ ] **Step 3: Make `intern` a real compression step, not a placeholder**

`intern` should be the cheap context-compression primitive the research doc implies, not an empty named role. That lets autoresearch and chat-rail tuning share the same optimizer vocabulary.

- [ ] **Step 4: Rebuild the xvision wiki index around the new optimizer/autoresearch split**

The wiki should point readers at:
- the optimizer docs for capability + holdout discipline
- the autoresearch plan for the V3 loop
- the new DSPy/autoresearch integration plan for the bridge between them

---

## Task 6: Verify the workspace still honors the offline isolation invariant

**Files:**
- Modify: `crates/xvision-dspy/src/lib.rs`
- Modify: `crates/xvision-engine/src/optimization/mod.rs`
- Modify: `crates/xvision-dashboard/wiki/optimizer.md`
- Test: any existing `cargo tree` / invariant checks already used in the optimizer docs

- [ ] **Step 1: Write the invariant checks explicitly**

Make sure the workspace still proves the same thing the optimizer wiki already promises:
- `xvision-engine` stays free of `dspy-rs`
- `xvision-dashboard` stays free of `dspy-rs`
- `xvision-dspy` remains the only place that owns the heavy transitive optimizer dependency tree

- [ ] **Step 2: Re-run the dependency checks**

Run:
```bash
cargo tree -p xvision-engine | grep -iE "dspy-rs|xvision-dspy" || echo "engine clean"
cargo tree -p xvision-dashboard | grep -i dspy || echo clean
```

Expected: both checks stay clean.

---

## Acceptance criteria

- `xvision-dspy` has real signatures for `decision_grader`, `intern`, and `chat_authoring`.
- Autoresearcher can consume optimizer artifacts instead of only running memory distillation.
- Holdout discipline is enforced before acceptance.
- The optimizer path still stays offline-only by default.
- The dashboard / MCP / wiki surfaces explain the new flow clearly and do not imply a live runtime dependency on `dspy-rs`.

---

## Notes for implementers

- Prefer small, reviewable steps. Don’t try to land the entire optimizer + autoresearch bridge in one giant commit.
- Keep the optimizer output schema stable and replayable.
- If a field is needed to explain why an optimizer result won, it belongs in the persisted artifact, not in a transient log line.
- If a path would cause `xvision-engine` to import `dspy-rs`, stop and redesign the seam.
