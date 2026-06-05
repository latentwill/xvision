# Optimizer Mutation Axes — Prompt, Filter & DSPy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the autooptimizer mutation axes that actually move the objective on real agent strategies — trader-**prompt** mutation, **filter**-threshold mutation, and an in-loop **DSPy** writeback — since the current 6 `risk.*` params are Sharpe-neutral (scale-invariant) no-ops.

**Architecture:** All three axes sit on one shared substrate: a per-`AgentRef` override (`prompt_override`, plus `model_override` reserved for the F25 follow-up) carried in the `Strategy` artifact and merged at slot resolution. This gives prompt mutations a *home* that changes the strategy content hash (so lineage/dedup work) **without** polluting the shared agent library. The filter axis is independent — it walks the already-typed `Filter` AST. DSPy builds on the prompt substrate: the existing flywheel's compiled `Pattern` is routed into a `prompt_override` candidate instead of only prefixing the mutator's system prompt. Which axes are live is gated by the existing `allowed_mutation_kinds` allowlist — model-selection (F25) and multi-agent become additional gated entries later.

**Tech Stack:** Rust (xvision-engine, xvision-filters), serde + ts-rs (frontend type export), SQLite (lineage/optimization stores), `xvision-memory` (DSPy flywheel store), tokio. Build through `scripts/cargo` (disk guard); test with `cargo test -p xvision-engine`.

**Base:** branch `feat/optimizer-mutation-axes` off `origin/main` (`cfc3771`). Source findings: `docs/QA/2026-06-05-autooptimizer-run7-gemini31-findings.md`; F25 design pass: `docs/QA/2026-06-04-autooptimizer-capability-gaps-findings.md:74-82`.

**Terminology guardrail (CLAUDE.md):** developer-surface = `autooptimizer`/`AutoOptimizer*`; never collapse to bare `optimizer` (that's the unrelated DSPy subsystem). Operator-surface = "Optimizer" / "Experiment" / "Experiment writer". New operator-facing concepts need a row in `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`.

---

## Pre-flight (do once, before Task 1)

- [ ] **P0.1: Confirm worktree + base.** You should be in `/Users/edkennedy/Code/xvision/.worktrees/optimizer-mutation-axes` on branch `feat/optimizer-mutation-axes`. Run `git log --oneline -1` — expect `cfc3771 docs(qa): run-7 optimizer findings`.
- [ ] **P0.2: Set a per-track cargo target** so parallel agents don't collide on the shared dir:
  ```bash
  export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-mutation-axes"
  ```
- [ ] **P0.3: Establish the build baseline.** `origin/main` is NOT build-gated and may already be red. Run:
  ```bash
  scripts/cargo test -p xvision-engine --no-run 2>&1 | tail -40
  ```
  **Known pre-existing breakage to expect:** `crates/xvision-engine/src/autooptimizer/mutator.rs` tests call `build_user_payload(...)` with **6** arguments (e.g. line ~804 `build_user_payload("prog", &kinds, &keys, None, 7, Some(ctx))`) but the function signature (line ~499) takes **7** params (the 7th is `avoid_count: usize`). If the baseline fails to compile on this, fix the three test call sites first by passing `0` for `avoid_count` (a `param`-less arity fix — see Task 12, which folds this in), commit as `fix(optimizer): repair build_user_payload test arity (pre-existing main breakage)`, and re-establish a green baseline before starting Task 1. Record the exact baseline state (green, or green-after-arity-fix) in the commit message so reviewers know what was inherited vs introduced.

---

## File Structure

New / modified files, grouped by responsibility:

**Substrate (Phase 0):**
- Modify `crates/xvision-engine/src/strategies/agent_ref.rs` — add `prompt_override`, `model_override` to `AgentRef`.
- Modify `crates/xvision-engine/src/agent/pipeline.rs` (`resolve_agent_slots_for_strategy`, ~line 996) — merge overrides onto the resolved slot.
- Regenerated: `frontend/web/src/api/types.gen/AgentRef.ts` (ts-rs export; do not hand-edit).

**Prompt axis (Phase 1):**
- Modify `crates/xvision-engine/src/autooptimizer/mutator.rs` — apply `Prose` edits to the trader `AgentRef.prompt_override`; flip `applicable_mutation_kinds` to allow `prose` when a prompt home exists.
- Modify `crates/xvision-engine/src/autooptimizer/validator.rs` — validate prose edits (non-empty `after`, role resolves to an agent).
- Modify `crates/xvision-engine/src/autooptimizer/inversion.rs` — carry prose through inversion as a real change.
- Modify `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md` — document the `prose` experiment shape.

**Filter axis (Phase 2):**
- Modify `crates/xvision-engine/src/autooptimizer/mutator.rs` — `MutationKind::Filter`, `FilterEdit`, AST-walk apply, `filter` applicability.
- Modify `crates/xvision-engine/src/autooptimizer/validator.rs` — validate filter edits against the live AST.
- Modify `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md` — document the `filter` experiment shape + enumerated tunable filter paths.
- Reference only (read, don't modify): `crates/xvision-filters/src/types.rs` (the `Filter`/`ConditionTree`/`Operand`/`Operator` AST).

**DSPy wiring (Phase 3):**
- Modify `crates/xvision-cli/src/commands/autooptimizer.rs` (~line 1345) — construct `DspyContext` when `dspy_enabled`.
- Modify `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs` (~line 181) — same.
- Modify `crates/xvision-engine/src/autooptimizer/cycle.rs` — emit a prose candidate from a freshly compiled `Pattern` (in-loop writeback).

**Bundled fixes (Phase 4):**
- Modify `crates/xvision-engine/src/autooptimizer/mutator.rs` — F32 retry-seed rotation.
- Modify `crates/xvision-engine/src/autooptimizer/parent_policy.rs` — real `delta_sharpe` score instead of the hash-proxy stub.
- Modify `crates/xvision-engine/src/autooptimizer/cycle.rs` — skip honesty check on `no_candidate`.

**Config (cross-cutting):**
- Modify `crates/xvision-engine/src/autooptimizer/config.rs` — add `"filter"` to `default_allowed_mutation_kinds`.

---

## PHASE 0 — Substrate: per-`AgentRef` override

The prompt axis (Phase 1) and DSPy writeback (Phase 3) both need a place to store a mutated trader prompt that (a) changes the `Strategy` content hash so lineage/dedup work, and (b) is honored at eval time. The team's F25 design pass already concluded this is a per-`AgentRef` optional override merged at resolution (`capability-gaps-findings.md:79`). We add `prompt_override` now and `model_override` as a reserved field (wired at resolution, consumed by the deferred F25 model-swap axis).

### Task 1: Add override fields to `AgentRef`

**Files:**
- Modify: `crates/xvision-engine/src/strategies/agent_ref.rs:57-72`
- Test: same file, `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests.** Add to the test module in `agent_ref.rs`:

```rust
    #[test]
    fn agent_ref_overrides_default_to_none_and_omit_from_wire() {
        // A ref with no overrides must serialize WITHOUT the override keys, so
        // existing strategy JSON and content hashes are byte-stable.
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(!s.contains("prompt_override"), "absent override must be omitted: {s}");
        assert!(!s.contains("model_override"), "absent override must be omitted: {s}");
    }

    #[test]
    fn agent_ref_overrides_round_trip_when_present() {
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: Some("You are a disciplined momentum trader...".into()),
            model_override: Some("openrouter/google/gemini-3.1-flash-lite".into()),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AgentRef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn agent_ref_legacy_json_without_overrides_parses() {
        // Strategies written before this field exists must still load.
        let r: AgentRef = serde_json::from_value(json!({
            "agent_id": "01HZAGENT", "role": "trader"
        })).unwrap();
        assert_eq!(r.prompt_override, None);
        assert_eq!(r.model_override, None);
    }
```

- [ ] **Step 2: Run to verify failure.** `scripts/cargo test -p xvision-engine agent_ref:: 2>&1 | tail -20` — expect compile error: missing fields `prompt_override`, `model_override`.

- [ ] **Step 3: Add the fields.** In `agent_ref.rs`, after the `activates` field (line 71), inside `struct AgentRef`:

```rust
    /// Optional per-strategy override of the referenced agent's trader-slot
    /// system prompt. `None` (the default) = use the shared agent library
    /// prompt verbatim. `Some(p)` makes THIS strategy run with prompt `p`
    /// without mutating the shared `Agent` record — so the override lands in
    /// the `Strategy` content hash (proper lineage) and never leaks into other
    /// strategies that reference the same agent. This is the "home" that makes
    /// `prose` optimizer mutations reachable (run-7 finding; F25 design pass).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub prompt_override: Option<String>,
    /// Optional per-strategy override of the referenced agent's trader-slot
    /// `(provider/)model`. Same rationale as `prompt_override`. Reserved for the
    /// deferred F25 model-swap mutation axis; honored at resolution today so the
    /// axis is a pure mutator/validator add later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub model_override: Option<String>,
```

- [ ] **Step 4: Fix every other `AgentRef { .. }` literal in the codebase.** Adding non-`Default` struct fields breaks existing literals. Find them:
  ```bash
  grep -rn "AgentRef {" crates/ | grep -v "agent_ref.rs"
  ```
  For each, add `prompt_override: None, model_override: None,` (or `..Default::default()` only if `AgentRef` derives `Default` — it does NOT today, so use explicit `None`s). Expect hits in test fixtures and authoring code.

- [ ] **Step 5: Run to verify pass.** `scripts/cargo test -p xvision-engine agent_ref:: 2>&1 | tail -20` — expect PASS.

- [ ] **Step 6: Commit.**
  ```bash
  git add crates/xvision-engine/src/strategies/agent_ref.rs
  git add -A   # picks up any literal fixes in other files
  git commit -m "feat(optimizer): add prompt_override/model_override to AgentRef (mutation substrate)"
  ```

### Task 2: Honor overrides at slot resolution

`resolve_agent_slots_for_strategy` (`agent/pipeline.rs:996`) reads `(provider, model, system_prompt)` straight off the resolved `Agent`'s slot. The override must win when present.

**Files:**
- Read first (whole fn): `crates/xvision-engine/src/agent/pipeline.rs` around line 996.
- Modify: same function.
- Test: `crates/xvision-engine/src/agents/store.rs` test module (there's an existing `resolve_agent_slots_for_strategy_binds_attached_trader` test ~line 1199 to mirror).

- [ ] **Step 1: Read the resolver.** `Read crates/xvision-engine/src/agent/pipeline.rs` from ~960 to the end of `resolve_agent_slots_for_strategy`. Identify the struct it returns (the explorer notes a resolved slot carrying `.slot.model` and `.system_prompt`) and where it copies `system_prompt`/`model` off `agent.slots`. Note the exact field names — the test below references `system_prompt` and `slot.model`, but confirm against the real return type before writing.

- [ ] **Step 2: Write the failing test.** In `crates/xvision-engine/src/agents/store.rs` tests, mirroring the existing resolve test, add one where the strategy's trader `AgentRef` carries overrides and assert they win:

```rust
    #[tokio::test]
    async fn resolve_applies_agent_ref_prompt_and_model_overrides() {
        // Build a strategy whose trader AgentRef carries prompt_override +
        // model_override; the resolved slot must reflect the OVERRIDES, not the
        // shared agent library values.
        // (Reuse the same pool/agent/strategy setup as
        // `resolve_agent_slots_for_strategy_binds_attached_trader`; then set
        // strategy.agents[0].prompt_override / model_override before resolving.)
        // ... setup omitted: copy the sibling test's fixture ...
        strategy.agents[0].prompt_override = Some("OVERRIDDEN PROMPT".to_string());
        strategy.agents[0].model_override = Some("overridden-model".to_string());

        let slots = crate::agent::pipeline::resolve_agent_slots_for_strategy(&pool, &strategy)
            .await
            .unwrap();
        assert_eq!(slots[0].system_prompt, "OVERRIDDEN PROMPT");
        assert_eq!(slots[0].slot.model.as_deref(), Some("overridden-model"));
    }
```
  (Adjust field access to the real return type confirmed in Step 1.)

- [ ] **Step 3: Run to verify failure.** `scripts/cargo test -p xvision-engine resolve_applies_agent_ref 2>&1 | tail -20` — expect FAIL (override ignored).

- [ ] **Step 4: Merge overrides in the resolver.** In `resolve_agent_slots_for_strategy`, after the slot is resolved from the agent record and the owning `AgentRef` is in scope, apply:

```rust
    // Per-AgentRef overrides win over the shared library slot. Honor both here
    // so prompt/model mutations live in the Strategy artifact (content hash +
    // lineage) without touching the shared Agent record.
    if let Some(p) = agent_ref.prompt_override.as_ref() {
        resolved.system_prompt = p.clone();
    }
    if let Some(m) = agent_ref.model_override.as_ref() {
        resolved.slot.model = Some(m.clone());
    }
```
  Adjust `resolved.system_prompt` / `resolved.slot.model` to the real field paths from Step 1. If `prompt_override` wins, also recompute `prompt_version` if the resolved slot exposes one (it's a digest of the prompt — set it empty to force recompute, matching the DSPy-accept path in `dashboard/src/routes/optimizations.rs:279` which sets `prompt_version = String::new()`).

- [ ] **Step 5: Run to verify pass.** `scripts/cargo test -p xvision-engine resolve_applies_agent_ref 2>&1 | tail -20` — expect PASS. Then run the whole resolver test group to confirm no regression: `scripts/cargo test -p xvision-engine resolve_agent_slots 2>&1 | tail -20`.

- [ ] **Step 6: Commit.**
  ```bash
  git add crates/xvision-engine/src/agent/pipeline.rs crates/xvision-engine/src/agents/store.rs
  git commit -m "feat(optimizer): honor AgentRef prompt/model overrides at slot resolution"
  ```

### Task 3: Regenerate frontend types

**Files:**
- Regenerated: `frontend/web/src/api/types.gen/AgentRef.ts`

- [ ] **Step 1: Run ts-rs export.** The repo exports TS via the `ts-export` feature. Find the export command:
  ```bash
  grep -rn "ts-export\|ts_rs\|export_bindings\|cargo test.*export" Cargo.toml crates/xvision-engine/Cargo.toml scripts/ | head
  ```
  Run the project's standard binding-export (commonly `scripts/cargo test -p xvision-engine --features ts-export export_bindings` or a dedicated script). Confirm `frontend/web/src/api/types.gen/AgentRef.ts` now contains `prompt_override?: string` and `model_override?: string`.

- [ ] **Step 2: Commit.**
  ```bash
  git add frontend/web/src/api/types.gen/AgentRef.ts
  git commit -m "chore(types): regenerate AgentRef bindings for prompt/model overrides"
  ```

---

## PHASE 1 — Prompt axis (reach `prose`)

`MutationKind::Prose` and `ProseEdit { agent_role, before, after }` already exist (`mutator.rs:101,107`). Today `apply_to` deliberately does NOT apply prose (comment at `mutator.rs:243-248`) and `applicable_mutation_kinds` returns `false` for it (`mutator.rs:215`) — because before Phase 0 there was no home. Now there is.

### Task 4: Apply `ProseEdit` to the trader `AgentRef.prompt_override`

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` (`MutationDiff::apply_to`, lines 249-279; the doc comment at 243-248)
- Test: same file test module

- [ ] **Step 1: Write the failing test.** Add to `mutator.rs` tests (the fixture strategy at line 625 already has `agents: [{agent_id, role: "trader"}]`):

```rust
    #[test]
    fn apply_to_writes_prose_into_trader_prompt_override() {
        let base = fixture_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit {
                agent_role: "trader".into(),
                before: String::new(),
                after: "Trade only with-trend; size down in chop.".into(),
            }],
            params: vec![],
            tools: ToolDiff { added: vec![], removed: vec![] },
            rationale: "test".into(),
        };
        let child = diff.apply_to(&base);
        let trader = child.agents.iter().find(|a| a.canonical_role() == "trader").unwrap();
        assert_eq!(
            trader.prompt_override.as_deref(),
            Some("Trade only with-trend; size down in chop.")
        );
        // And it is a REAL change (distinct content hash), not an identity no-op.
        assert!(!is_identity_diff(&diff, &base), "prose change must alter the strategy");
    }

    #[test]
    fn apply_to_prose_for_unknown_role_is_noop() {
        // A prose edit naming a role no strategy agent plays leaves the strategy
        // unchanged (validator rejects it upstream; apply stays total/safe).
        let base = fixture_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit { agent_role: "nonexistent".into(), before: String::new(), after: "x".into() }],
            params: vec![], tools: ToolDiff { added: vec![], removed: vec![] }, rationale: "t".into(),
        };
        assert!(is_identity_diff(&diff, &base), "unknown-role prose must be a no-op");
    }
```

- [ ] **Step 2: Run to verify failure.** `scripts/cargo test -p xvision-engine apply_to_writes_prose 2>&1 | tail -20` — expect FAIL (prose still ignored).

- [ ] **Step 3: Apply prose in `apply_to`.** In `MutationDiff::apply_to` (after the tools loop, before `s` is returned at line 278), add:

```rust
        // Prose edits land on the trader AgentRef's prompt_override (Phase 0
        // substrate). Matching by canonical role keeps lineage stable: the
        // override changes the Strategy content hash WITHOUT touching the shared
        // Agent library record. An edit naming a role no agent plays is a no-op
        // (the validator rejects those upstream; apply stays total).
        for edit in &self.prose {
            let target = crate::strategies::agent_ref::canonical_role(&edit.agent_role);
            if let Some(a) = s.agents.iter_mut().find(|a| a.canonical_role() == target) {
                a.prompt_override = Some(edit.after.clone());
            }
        }
```
  Then update the doc comment at lines 243-248 — replace "Prose edits are intentionally **not** applied here…" with a note that prose now writes the trader ref's `prompt_override`, and that an unknown-role prose edit is a no-op.

- [ ] **Step 4: Update `is_empty`.** `MutationDiff::is_empty` (line 223) already counts `self.prose.is_empty()` — no change. Confirm.

- [ ] **Step 5: Run to verify pass.** `scripts/cargo test -p xvision-engine apply_to_ 2>&1 | tail -30` — expect all `apply_to_*` PASS.

- [ ] **Step 6: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs
  git commit -m "feat(optimizer): apply prose mutations to trader prompt_override"
  ```

### Task 5: Make `prose` structurally applicable when a prompt home exists

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` (`applicable_mutation_kinds`, lines 208-220; doc comment 200-207)
- Test: same file

- [ ] **Step 1: Write the failing test.** Replace the existing `applicable_kinds_drop_prose_and_keep_param` test (line 751) — its premise is now inverted — and add the new behavior:

```rust
    #[test]
    fn applicable_kinds_allow_prose_when_strategy_has_an_agent() {
        let base = fixture_strategy(); // has a trader AgentRef
        let allowed = vec!["prose".into(), "param".into(), "tool".into()];
        let kinds = applicable_mutation_kinds(&base, &allowed);
        assert!(kinds.contains(&"prose".to_string()), "prose now has a home (AgentRef override)");
        assert!(kinds.contains(&"param".to_string()), "param always applicable (risk exists)");
    }

    #[test]
    fn applicable_kinds_drop_prose_when_strategy_has_no_agents() {
        let mut base = fixture_strategy();
        base.agents.clear();
        let allowed = vec!["prose".into(), "param".into()];
        let kinds = applicable_mutation_kinds(&base, &allowed);
        assert!(!kinds.contains(&"prose".to_string()), "no agent => no prompt home => prose excluded");
    }
```

- [ ] **Step 2: Run to verify failure.** `scripts/cargo test -p xvision-engine applicable_kinds 2>&1 | tail -20` — expect FAIL (`prose => false`).

- [ ] **Step 3: Flip applicability.** In `applicable_mutation_kinds` (line 208), compute a prose-home predicate and use it:

```rust
pub fn applicable_mutation_kinds(base: &Strategy, allowed: &[String]) -> Vec<String> {
    let has_params = !tunable_param_keys(base).is_empty();
    // Prose is applicable iff the strategy has at least one agent to carry a
    // `prompt_override` (Phase 0). For agentless/pre-refactor strategies there
    // is still no home, so prose stays excluded there.
    let has_prompt_home = !base.agents.is_empty();
    let has_filter = base.filter.is_some(); // Phase 2
    allowed
        .iter()
        .filter(|k| match k.as_str() {
            "param" => has_params,
            "tool" => true,
            "prose" => has_prompt_home,
            "filter" => has_filter,
            _ => false,
        })
        .cloned()
        .collect()
}
```
  (The `"filter"` arm is added now to avoid a second edit in Phase 2; it's inert until Task 7 adds the kind.) Update the doc comment (200-207) to reflect that prose is now reachable via the AgentRef override.

- [ ] **Step 4: Run to verify pass.** `scripts/cargo test -p xvision-engine applicable_kinds 2>&1 | tail -20` — expect PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs
  git commit -m "feat(optimizer): allow prose mutations when strategy has an agent (prompt home)"
  ```

### Task 6: Validate prose edits + carry through inversion + document the shape

**Files:**
- Read first: `crates/xvision-engine/src/autooptimizer/validator.rs` (whole file — find `validate_mutation_diff`), `crates/xvision-engine/src/autooptimizer/inversion.rs` (whole file).
- Modify: both, plus `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md`.
- Test: `validator.rs` and `inversion.rs` test modules.

- [ ] **Step 1: Read the two files** to learn the existing `ValidationError` construction pattern and how inversion builds the reverse diff for params/tools.

- [ ] **Step 2: Write failing validator test.** In `validator.rs` tests, assert a `Prose` diff is rejected when `after` is empty or the `agent_role` matches no agent, and accepted otherwise:

```rust
    #[test]
    fn prose_edit_requires_nonempty_after_and_known_role() {
        let base = /* fixture strategy with a "trader" AgentRef */;
        // empty after -> error
        let empty = prose_diff("trader", "");
        assert!(validate_mutation_diff(&empty, &base).is_err());
        // unknown role -> error
        let unknown = prose_diff("ghost", "do X");
        assert!(validate_mutation_diff(&unknown, &base).is_err());
        // good -> ok
        let ok = prose_diff("trader", "Trade with-trend only.");
        assert!(validate_mutation_diff(&ok, &base).is_ok());
    }
```

- [ ] **Step 3: Run to verify failure**, then implement in `validate_mutation_diff`: when `diff.kind == MutationKind::Prose`, for each `ProseEdit` require `!after.trim().is_empty()` (code `"empty_prose"`) and that `canonical_role(agent_role)` matches some `base.agents[..].canonical_role()` (code `"unknown_role"`). Mirror the existing `ValidationError` shape.

- [ ] **Step 4: Write failing inversion test + implement.** In `inversion.rs`, ensure inverting a prose diff swaps `before`/`after` (so the inversion pair is a real symmetric change, not dropped). Add the prose arm to whatever match builds the inverse; assert `invert(invert(d)) == d` for a prose diff.

- [ ] **Step 5: Document the prose shape** in `prompts/autooptimizer/mutator-v1.md`: add a `prose` experiment example showing `{ "kind": "prose", "prose": [{ "agent_role": "trader", "before": "<current prompt excerpt>", "after": "<full revised prompt>" }], ... }` and state that `after` is the COMPLETE replacement prompt for that role. Keep operator-surface language ("Experiment writer").

- [ ] **Step 6: Run the autooptimizer test group.** `scripts/cargo test -p xvision-engine autooptimizer:: 2>&1 | tail -30` — expect PASS.

- [ ] **Step 7: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/validator.rs crates/xvision-engine/src/autooptimizer/inversion.rs crates/xvision-engine/prompts/autooptimizer/mutator-v1.md
  git commit -m "feat(optimizer): validate + invert prose mutations; document prose experiment shape"
  ```

---

## PHASE 2 — Filter axis (`MutationKind::Filter`)

The filter is a fully-typed AST: `Strategy.filter: Option<Filter>` (`strategies/mod.rs:153`), with `Filter.conditions: ConditionTree` (`All|Any` of `Condition { lhs, op, rhs }`), `Operand::Numeric(f64)` / `Range(f64,f64)`, parameterized `Operator` variants (`AboveFor(u32)`, `WithinPct(f64)`, `SlopeGt(u32)`, `ZscoreGt(u32)`, …), and scalar fields `cooldown_bars: u32`, `max_wakeups_per_day: Option<u32>` (`xvision-filters/src/types.rs:966-989`). All numeric thresholds are programmatically addressable. We expose them as dotted paths and mutate them like params.

### Task 7: Define `MutationKind::Filter` + `FilterEdit` + tunable-path enumeration

**Files:**
- Read first: `crates/xvision-filters/src/types.rs:640-1000` (the AST: `Operator`, `Operand`, `Condition`, `ConditionTree`, `Filter`).
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs`
- Test: same file

- [ ] **Step 1: Read the AST** to confirm exact variant names and arities for `Operator` and `Operand` (the plan references `Operand::Numeric`, `Operator::WithinPct(f64)`, etc. — verify before coding the walk).

- [ ] **Step 2: Add the type.** In `mutator.rs`, extend the enum (line 100) and add an edit struct:

```rust
pub enum MutationKind {
    Prose,
    Param,
    Tool,
    Filter,
}

/// One incremental change to a numeric threshold inside the strategy's typed
/// `Filter` AST, addressed by a stable dotted path (see `filter_tunable_paths`).
/// e.g. `path = "conditions.0.rhs.numeric"`, `before = 25.0`, `after = 28.0`,
/// or `path = "cooldown_bars"`, `before = 3`, `after = 6`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterEdit {
    pub path: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}
```
  Add `pub filter: Vec<FilterEdit>` to `MutationDiff` (line 127) with `#[serde(default)]`, and `filter: Vec::new()` to `empty_mutation()` (line 135). Update `MutationDiff::is_empty` to also check `self.filter.is_empty()`.

- [ ] **Step 3: Implement `filter_tunable_paths`.** Add a function that walks a `&Filter` and returns `Vec<(String /*path*/, serde_json::Value /*current*/)>` for every mutatable numeric node: each `Condition`'s `rhs`/`lhs` `Numeric`/`Range`, each parameterized `Operator` arg, plus `cooldown_bars` and `max_wakeups_per_day`. Use stable indices (`conditions.<i>.rhs.numeric`, `conditions.<i>.op.within_pct`, etc.). Write it next to `tunable_param_keys`. Mirror its return style.

- [ ] **Step 4: Write failing tests** for `filter_tunable_paths` (build a small `Filter` with one ADX>25 condition + cooldown_bars=3; assert the path list contains the rhs-numeric path with value `25.0` and `cooldown_bars` with `3`) and for the enum/serde round-trip of a `Filter`-kind `MutationDiff`.

- [ ] **Step 5: Run to verify failure**, implement, **run to verify pass**: `scripts/cargo test -p xvision-engine filter_tunable 2>&1 | tail -20`.

- [ ] **Step 6: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs
  git commit -m "feat(optimizer): add Filter mutation kind, FilterEdit, and filter_tunable_paths"
  ```

### Task 8: Apply `FilterEdit` to the AST + validate

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` (`apply_to`), `crates/xvision-engine/src/autooptimizer/validator.rs`
- Test: both

- [ ] **Step 1: Write the failing apply test.** Build a strategy with a `Filter` (ADX>25), diff `path="conditions.0.rhs.numeric"`, `after=28.0`; assert the applied child's filter has `28.0` at that node and `!is_identity_diff`. Add a no-op case (path not present → unchanged → identity).

- [ ] **Step 2: Run to verify failure.** Expect FAIL.

- [ ] **Step 3: Implement apply.** In `MutationDiff::apply_to`, after the prose loop, when `s.filter` is `Some(f)`, for each `FilterEdit` resolve `path` against a mutable view of `f` and set `after`. Implement a `set_filter_value(filter: &mut Filter, path: &str, value: &serde_json::Value) -> bool` (returns whether it applied) that matches the path forms produced by `filter_tunable_paths` — keep the two functions adjacent and symmetric so a produced path always resolves. An unresolved path leaves the filter unchanged (no panic).

- [ ] **Step 4: Validate filter edits.** In `validate_mutation_diff`, for `MutationKind::Filter`: require `base.filter.is_some()` (code `"no_filter"`), require each `path` to be one `filter_tunable_paths(base.filter)` actually produces (code `"unknown_filter_path"`), and require `after` to be the right JSON type for that node (number; for `max_wakeups_per_day` allow null). Write a failing test for an unknown path and a wrong-type value, then implement.

- [ ] **Step 5: Run the autooptimizer group.** `scripts/cargo test -p xvision-engine autooptimizer:: 2>&1 | tail -30` — expect PASS.

- [ ] **Step 6: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs crates/xvision-engine/src/autooptimizer/validator.rs
  git commit -m "feat(optimizer): apply + validate filter-threshold mutations against the typed AST"
  ```

### Task 9: Surface filter kind to the experiment writer + default-enable it

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` (`build_user_payload` — enumerate filter paths like param keys), `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md`, `crates/xvision-engine/src/autooptimizer/config.rs` (default allowlist).
- Test: `mutator.rs`, `config.rs`

- [ ] **Step 1: Enumerate filter paths in the prompt.** In `build_user_payload`, when `filter` is an allowed kind and `base.filter.is_some()`, add a section listing the `filter_tunable_paths` (path + current value) exactly like `keys_section` does for params, so the writer proposes a valid `path`. This requires threading the filter paths into `build_user_payload` (add a `filter_paths: &[(String, serde_json::Value)]` arg, computed once in `propose` like `param_keys`). Write/adjust a `build_user_payload` test asserting the filter section appears. **Note:** this is the same function with the known pre-existing arity issue — keep all test call sites in sync with the final arg count (see Pre-flight P0.3 / Task 12).

- [ ] **Step 2: Document the filter shape** in `mutator-v1.md`: `{ "kind": "filter", "filter": [{ "path": "conditions.0.rhs.numeric", "before": 25, "after": 28 }], ... }`, noting paths must come from the enumerated list and changes should be incremental (a clear direction + magnitude).

- [ ] **Step 3: Default-enable filter.** In `config.rs:67`, change `default_allowed_mutation_kinds` to `vec!["prose".into(), "param".into(), "tool".into(), "filter".into()]`. Add a `config.rs` test asserting `"filter"` is in the default allowlist. (Existing `autooptimizer.toml` files without the key get the new default via `#[serde(default = ...)]`; files that pin the list keep their pin — document this in the commit body.)

- [ ] **Step 4: Run.** `scripts/cargo test -p xvision-engine autooptimizer:: 2>&1 | tail -30` — expect PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs crates/xvision-engine/prompts/autooptimizer/mutator-v1.md crates/xvision-engine/src/autooptimizer/config.rs
  git commit -m "feat(optimizer): expose filter paths to the experiment writer; enable filter axis by default"
  ```

---

## PHASE 3 — DSPy in-loop writeback

`handle_cycle_dspy` writes judge findings as Observations and, at the cohort threshold, calls `bridge.compile(...)` to produce a `Pattern` instruction (`dspy_flywheel.rs:112`). Today the compiled instruction is only ever read back as a **mutator system-prompt prefix** (`query_dsr_prefix` → `cycle.rs:107`), and the whole path is inert because both run-cycle entry points pass `dspy_ctx = None` and `dspy_enabled` defaults `false`. Phase 3 (a) actually constructs the `DspyContext`, and (b) routes a freshly compiled `Pattern` into a `prompt_override` candidate — the in-loop writeback the run-7 doc asks for.

### Task 10: Construct `DspyContext` in the run-cycle entry points

**Files:**
- Read first: `crates/xvision-engine/src/autooptimizer/dspy_bridge.rs` (the `DspyBridge` trait + any production impl), `crates/xvision-cli/src/commands/autooptimizer.rs` ~1320-1390, `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs` ~160-190.
- Modify: the CLI and dashboard run-cycle paths.
- Test: a CLI-level or engine-level test that, with `dspy_enabled=true`, a `DspyContext` is threaded (the existing `dspy_flywheel.rs` tests already cover the flywheel mechanics).

- [ ] **Step 1: Identify the production `DspyBridge`.** `grep -rn "impl DspyBridge" crates/` — determine whether a real bridge exists or only the test `RecordingBridge`. If only a test bridge exists, the in-loop compile cannot run for real; in that case scope Task 10 to "construct `DspyContext` with the available bridge behind `dspy_enabled`, and if no production bridge exists, log a clear `dspy_enabled but no bridge available` warning and leave `ctx = None`." Record which case applies in the commit body. Do NOT invent an LLM bridge here — that is its own track.

- [ ] **Step 2: Build the context behind the flag.** Where the CLI currently passes `None` (`autooptimizer.rs:~1345`), construct:
  ```rust
  let dspy_ctx = if config.dspy_enabled {
      // MemoryStore for the run (reuse the engine's memory store handle if the
      // run already opens one; else open the autooptimizer namespace store).
      Some(DspyContext {
          store: memory_store.clone(),
          bridge: dspy_bridge.clone(),
          namespace: crate::autooptimizer::mutator::MUTATIONS_NS.to_string(),
      })
  } else {
      None
  };
  ```
  and pass `dspy_ctx.as_ref()` to the cycle. Mirror in the dashboard route. Use the existing `MUTATIONS_NS` constant for namespace consistency with the mutator's memory layer.

- [ ] **Step 3: Run** the engine + CLI test groups touching the optimizer; ensure no regression. `scripts/cargo test -p xvision-engine autooptimizer:: 2>&1 | tail -20`.

- [ ] **Step 4: Commit.**
  ```bash
  git add crates/xvision-cli/src/commands/autooptimizer.rs crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs
  git commit -m "feat(optimizer): construct DspyContext in run-cycle paths behind dspy_enabled"
  ```

### Task 11: Route a compiled `Pattern` into a `prompt_override` candidate (in-loop writeback)

**Files:**
- Read first: `crates/xvision-engine/src/autooptimizer/cycle.rs` (the section around `handle_cycle_dspy` at line ~594 and the candidate-gating loop).
- Modify: `cycle.rs`, and add a helper to `dspy_flywheel.rs` to fetch the newest `Pattern` text.
- Test: `cycle.rs` or `dspy_flywheel.rs`.

- [ ] **Step 1: Add a "latest pattern" reader.** In `dspy_flywheel.rs`, add `pub async fn latest_pattern(store, namespace) -> anyhow::Result<Option<String>>` returning the most recently compiled `Pattern` instruction (query `Tier::Pattern`). Write a failing test mirroring `dspy_enabled_triggers_compile_on_threshold` that asserts `latest_pattern` returns the compiled instruction; implement.

- [ ] **Step 2: Emit a prose candidate from the pattern.** In `cycle.rs`, after `handle_cycle_dspy` runs and the flywheel may have compiled a new `Pattern`, when `dspy_ctx` is `Some` and a new pattern exists, construct a `MutationDiff { kind: Prose, prose: vec![ProseEdit { agent_role: <trader role>, before: <current prompt>, after: <pattern instruction> }], .. }`, apply it via the canonical `apply_to`, and gate the resulting candidate through the SAME gate/inversion/lineage path as a mutator-proposed candidate. This makes the DSPy-optimized prompt compete on the real backtest objective rather than only nudging the mutator's system prompt. Guard against re-emitting an identical candidate using the existing `candidate_already_tried`/avoid-set machinery.

- [ ] **Step 3: Write an integration-style test** (engine-level) that, given a seeded `MemoryStore` with enough Observations to compile a `Pattern` and a trader strategy, the cycle produces a prose candidate whose applied prompt equals the pattern instruction. Use the in-memory store + a `RecordingBridge`-style fake (see `dspy_flywheel.rs` tests for the pattern).

- [ ] **Step 4: Run.** `scripts/cargo test -p xvision-engine autooptimizer::cycle 2>&1 | tail -30` — expect PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/cycle.rs crates/xvision-engine/src/autooptimizer/dspy_flywheel.rs
  git commit -m "feat(optimizer): route compiled DSPy Pattern into a prompt_override candidate (in-loop)"
  ```

---

## PHASE 4 — Bundled run-7 fixes

### Task 12: F32 — rotate the exploration seed across retries

The retry loop reuses one `exploration_seed` for every attempt, so `focus = param_keys[(seed) % len]` never rotates and `already_tried` is unescapable within a single `propose()` call (`mutator.rs:346-355`, `:548`).

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs`
- Test: same file

- [ ] **Step 1: First, repair the pre-existing test arity** if not already done in Pre-flight: the `build_user_payload` test call sites (~804, 817, 827) must match the real signature's arg count. After Task 9 added `filter_paths`, the signature grew again — update ALL call sites to the final arity in this commit so the module compiles.

- [ ] **Step 2: Write the failing test.** Assert that successive attempts focus on different params. Since `build_user_payload` derives focus from the seed, test at that level:

```rust
    #[test]
    fn retry_rotates_focus_param_across_attempts() {
        let keys: Vec<String> = (0..4).map(|i| format!("risk.k{i}")).collect();
        // Simulate the per-attempt seed the propose loop now uses.
        let base_seed = 9u64;
        let p0 = build_user_payload_focus(base_seed.wrapping_add(0), &keys);
        let p1 = build_user_payload_focus(base_seed.wrapping_add(1), &keys);
        assert_ne!(p0, p1, "attempt 0 and attempt 1 must focus different params");
    }
```
  (Extract a tiny pure helper `build_user_payload_focus(seed, keys) -> &str` returning the chosen focus key, or assert on the full payload string difference — whichever matches the existing test style.)

- [ ] **Step 3: Run to verify failure**, then fix the loop. In `Mutator::propose`, change the `build_user_payload` call (line 347) to pass a per-attempt seed:

```rust
        for attempt in 0..max_attempts {
            // F32 (run-7): rotate the exploration seed per attempt so the focus
            // parameter — `param_keys[seed % len]` — actually changes across
            // retries. Without this, every retry re-derives the SAME focus and
            // `already_tried` fires until the budget is exhausted with no escape.
            let attempt_seed = exploration_seed.wrapping_add(attempt as u64);
            let user_text = build_user_payload(
                &program_md,
                &kinds,
                &param_keys,
                /* filter_paths (from Task 9), */
                last_errors.as_deref(),
                attempt_seed,
                memory_context,
                avoid.len(),
            );
            // temperature also jitters per attempt — desirable:
            // temperature: Some(exploration_temperature(attempt_seed)),
            ...
```
  Update the `temperature` line (368) to `exploration_temperature(attempt_seed)` too.

- [ ] **Step 4: Run to verify pass.** `scripts/cargo test -p xvision-engine mutator:: 2>&1 | tail -30` — expect PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/mutator.rs
  git commit -m "fix(optimizer): F32 rotate exploration seed per retry so focus param changes (+repair build_user_payload test arity)"
  ```

### Task 13: Parent selection — rank by real `delta_sharpe`, not the hash-proxy stub

`score_node` derives a pseudo-random score from `bundle_hash` bytes (`parent_policy.rs:50-57`), so improved children don't reliably outrank the root. Wire the real per-node outcome metric.

**Files:**
- Read first: `crates/xvision-engine/src/autooptimizer/cycle.rs` (`record_outcome` and where `delta_sharpe` per child is persisted), `crates/xvision-engine/src/autooptimizer/lineage.rs` (node schema — is there an `outcomes`/metric column or a sibling table keyed by `bundle_hash`?).
- Modify: `parent_policy.rs` (+ `lineage.rs` if a reader method is needed).
- Test: `parent_policy.rs`

- [ ] **Step 1: Locate the metric source.** Find where a node's gated `delta_sharpe` (or objective score) is stored keyed by `bundle_hash` (the explorer noted `record_outcome(pool, child_hash, delta_sharpe)`). Determine the read path. If scores live in a table the `LineageStore` can read, add `LineageStore::node_score(bundle_hash, objective) -> Option<f64>`.

- [ ] **Step 2: Write the failing test.** Construct a lineage with a root and an improved child (higher stored `delta_sharpe`), both active leaves; assert `select_parents(TopK{k:1,..})` returns the CHILD, not the root. With the stub scorer this is pseudo-random / wrong.

- [ ] **Step 3: Run to verify failure**, then replace `score_node` to read the stored metric for `node.bundle_hash` (fall back to `0.0`/`diversity_score` only when no score exists). Thread the score source into `select_parents` (it currently only has the `LineageStore`, which is the right place for the reader). Keep the function signature stable for callers in `cycle.rs`.

- [ ] **Step 4: Run.** `scripts/cargo test -p xvision-engine parent_policy 2>&1 | tail -20` — expect PASS.

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/parent_policy.rs crates/xvision-engine/src/autooptimizer/lineage.rs
  git commit -m "fix(optimizer): rank parents by real delta_sharpe so improved children outrank the root"
  ```

### Task 14: Skip the honesty check on `no_candidate` cycles

When the mutator exhausts retries and emits `no_candidate`, the cycle still runs the honesty-check (kill-trades canary) evals — pure waste (run-7 🟡).

**Files:**
- Read first: `crates/xvision-engine/src/autooptimizer/cycle.rs` (where `no_candidate` is emitted and where the honesty check runs).
- Modify: `cycle.rs`
- Test: `cycle.rs`

- [ ] **Step 1: Write the failing test.** Drive a cycle whose mutator yields `no_candidate` (e.g. a fake dispatch that always returns an already-tried/identity diff, or `max_retries` exhausted) and assert the honesty-check eval count is `0` for that cycle (assert on whatever observable the cycle exposes — emitted events or an eval-run counter).

- [ ] **Step 2: Run to verify failure**, then early-return/skip the honesty-check block when the cycle produced no candidate. Keep the `no_candidate` event emission intact.

- [ ] **Step 3: Run.** `scripts/cargo test -p xvision-engine autooptimizer::cycle 2>&1 | tail -20` — expect PASS.

- [ ] **Step 4: Commit.**
  ```bash
  git add crates/xvision-engine/src/autooptimizer/cycle.rs
  git commit -m "fix(optimizer): skip honesty check on no_candidate cycles (no candidate to verify)"
  ```

---

## Final integration

### Task 15: Full workspace build + test + manual smoke

- [ ] **Step 1: Build the workspace.** `scripts/cargo build --workspace 2>&1 | tail -30` — expect clean. Consumer crates (`xvision-cli`, `xvision-dashboard`) must compile against the new `AgentRef` fields and `MutationKind::Filter` (any exhaustive `match MutationKind` elsewhere needs the new arm — `grep -rn "MutationKind::" crates/` to find them).
- [ ] **Step 2: Run the optimizer + strategies + agents test groups.** `scripts/cargo test -p xvision-engine autooptimizer:: strategies:: agents:: 2>&1 | tail -40` — expect PASS.
- [ ] **Step 3: Manual smoke (optional, if a dev DB is handy).** Create a tiny agent strategy with a filter, run `xvn optimizer run-cycle` (dev), and confirm the SSE/event stream now shows `prose` and/or `filter` experiments (not just `param`), and that a 2-week window produces a non-zero `delta_sharpe` on at least one accepted candidate. This is the run-7 acceptance: the optimizer can finally move the objective on an agent strategy.
- [ ] **Step 4: Commit any match-arm fixups** discovered in Step 1.
  ```bash
  git add -A
  git commit -m "chore(optimizer): exhaustive MutationKind match arms for Filter across consumers"
  ```

### Task 16: Update run-7 QA status + deferred register

- [ ] **Step 1: Append a resolution note** to `docs/QA/2026-06-05-autooptimizer-run7-gemini31-findings.md` marking: prompt axis ✅, filter axis ✅, DSPy in-loop ✅, F32 retry-seed ✅, parent-selection real-metrics ✅, honesty-skip ✅; min-window guard ⏸️ dropped (reframed as a future decision-count warning — same-scenario re-eval is now meaningful because mutations are behavioral). Note the deployed-image-lag caveat that several findings were already partly fixed on `main`.
- [ ] **Step 2: Commit (force-add — `docs/superpowers` is gitignored-but-tracked; plain `git add` no-ops there, but `docs/QA` is NOT gitignored — verify with `git check-ignore -v <path>` first).**
  ```bash
  git add docs/QA/2026-06-05-autooptimizer-run7-gemini31-findings.md
  git add -f docs/superpowers/plans/2026-06-05-optimizer-mutation-axes.md
  git commit -m "docs(qa): run-7 mutation-axis resolution status + implementation plan"
  ```

---

## Deferred items register

Recorded here so they don't get lost (per the team's "record deferred items in the plan" rule). These are intentionally OUT of scope for this wave; both become additional gated entries in `allowed_mutation_kinds` on the SAME per-`AgentRef`-override substrate this plan builds.

| Item | Why deferred | Where it lands | Substrate ready? |
|---|---|---|---|
| **F25 model-selection axis** | Operator-decision deferred (`capability-gaps-findings.md:72`); user framed it as a configurable follow-up. | `model_override` field is added in Task 1 + honored in Task 2. Remaining: `MutationKind::ModelSwap` + `model_swap` on `MutationDiff`; `apply_to` sets trader ref's `model_override`; `validator.rs` (only registered providers); `inversion.rs` (real change, not symmetric noise); add `"model_swap"` to allowlist (off by default). | ✅ field + resolution done here |
| **Multi-agent mutation** | Larger scope (mutating which/how many agents, pipeline edges); user framed as a configurable follow-up. | New gated mutation kind operating on `Strategy.agents` / `PipelineDef`; off by default in `allowed_mutation_kinds`. | partial (PipelineDef exists) |
| **Min-window guard → decision-count warning** | Dropped as a calendar gate (user: same-scenario re-eval is fine once mutations are behavioral). The honest signal is decision count, which the filter axis now tunes. | Optional: emit a "<N decisions over this window → gating unreliable" warning in `cycle.rs` once filter tuning is exercised. | n/a |

---

## Self-Review

**Spec coverage** (run-7 "Suggested next pass" + user directive):
1. Prompt axis → Phase 0 (substrate) + Phase 1 (Tasks 4–6). ✅
2. Filter mutation kind → Phase 2 (Tasks 7–9). ✅
3. DSPy in-loop → Phase 3 (Tasks 10–11). ✅
4. F32 retry-seed rotation → Task 12. ✅
5. Parent selection by gate score → Task 13. ✅
6. Skip honesty check on no_candidate → Task 14. ✅
7. Model-selection + multi-agent as configurable follow-ups → Deferred register (substrate prepared in Tasks 1–2). ✅
8. Min-window guard → dropped per operator decision; reframed in Deferred register. ✅

**Placeholder scan:** Two tasks (10, 13) intentionally branch on a codebase fact that must be read first (production `DspyBridge` existence; metric-storage location) rather than inventing an interface — each says exactly what to read and how to scope if absent. The deep AST-walk code (Tasks 7–8) is specified as symmetric `filter_tunable_paths`/`set_filter_value` functions with concrete path forms and tests, with a "read the AST first" step to confirm variant arities. This is deliberate: writing byte-exact code against files not yet read would risk wrong variant names. All interface additions (fields, enum arm, struct) are concrete and complete.

**Type consistency:** `prompt_override`/`model_override: Option<String>` used identically in Tasks 1, 2, 4, 11, and the deferred register. `MutationKind::Filter` + `FilterEdit { path, before, after }` + `MutationDiff.filter: Vec<FilterEdit>` consistent across Tasks 7–9. `canonical_role` (from `agent_ref.rs:22`) used for role matching in Tasks 4 and 6. `MUTATIONS_NS` reused for the DSPy namespace in Tasks 10–11.

**Build-gating caveat:** `origin/main` is not build-gated; Pre-flight P0.3 + Task 12 Step 1 handle the inherited `build_user_payload` arity breakage explicitly so it isn't mistaken for a regression introduced here.
