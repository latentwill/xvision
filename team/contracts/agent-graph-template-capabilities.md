---
track: agent-graph-template-capabilities
lane: leaf
wave: agent-graph-2026-05-22
worktree: .worktrees/agent-graph-template-capabilities
branch: task/agent-graph-template-capabilities
base: origin/main
status: deferred
depends_on:
  - agent-graph-capability-schema  # PR #527 — Phase A
blocks: []
stacking: declared:agent-graph-capability-schema
allowed_paths:
  - crates/xvision-engine/src/agents/templates.rs
  - crates/xvision-engine/src/strategies/templates.rs   # if a separate file holds strategy templates
  - crates/xvision-engine/tests/template_validation.rs  # NEW or extension — host the flipped test
  - crates/xvision-engine/tests/strategy_validate.rs    # only if the flipped test already lives here; otherwise no
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/model.rs           # Phase A owns the field; Phase E populates values
  - crates/xvision-engine/src/agents/capability.rs      # Phase A owns the enum
  - crates/xvision-engine/src/strategies/agent_ref.rs   # Phase A owns AgentRef.activates
  - crates/xvision-engine/src/agent/**                  # dispatcher not in scope
  - frontend/web/**                                     # Phase F owns UI
interfaces_used:
  - xvision_engine::agents::Capability (Phase A — closed enum)
  - xvision_engine::agents::AgentSlot::capabilities (Phase A — field)
  - xvision_engine::strategies::agent_ref::AgentRef::activates (Phase A — field)
  - xvision_engine::strategies::validate::validate_strategy (existing — must accept the new templates without diagnostics)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --check
  - cargo clippy --workspace --tests -- -D warnings
  - cargo test -p xvision-engine --test template_validation
  - cargo test -p xvision-engine
  - cargo build --workspace
acceptance:
  - **Every entry in `crates/xvision-engine/src/agents/templates.rs::builtin_templates()`** declares an explicit `capabilities: BTreeSet<Capability>` on each `AgentSlot`. The Phase A `serde(default)` field is no longer relied on for any builtin template — every slot states its capabilities deliberately.
  - **Capability declarations match the spec table** (`docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md` "Default-capability-set on starter templates (Phase E)" section):
    | Template | Slot → Capabilities |
    |---|---|
    | `single-trader` | `trader` → `{Trader}` |
    | `risk-checked-trader` | `trader` → `{Trader}`; `risk_check` → `{Critic}`; `executor` → `{Trader}` |
    | `momentum-trader-only` | `trader` → `{Trader}` |
    | `mean-reversion-trader` | `trader` → `{Trader}` |
    | `multi-asset-router-with-traders` | `router` → `{Router}`; `equities_trader` → `{Trader}`; `crypto_trader` → `{Trader}`; `fx_trader` → `{Trader}`; `aggregator` → `{Critic}` |
    | `regime-aware-trader` | `regime_filter` → `{Filter}`; `trader` → `{Trader}` |
    | `news-reader-plus-trader` | `news_reader` → `{Intern}`; `trader` → `{Trader}` |
    | `paper-confirmed-live-trader` | `paper_trader` → `{Trader}`; `live_executor` → `{Critic}` |
  - **Each strategy template's `agents: Vec<AgentRef>` is populated** with `AgentRef { agent_id, role, activates: Some(<primary capability>), ... }` matching the slot's capability declaration. The `activates` field is explicit (not `None`) — no implicit-capability inference path.
  - **The `validate_draft_succeeds_for_fresh_template` test flips** from expected-fail to expected-pass:
    - The `#[should_panic]` / `#[ignore]` / expected-fail annotation (whatever form it takes today) is removed in the same PR.
    - The test asserts `validate_strategy(fresh_template)` returns `Ok(_)` (or equivalent: zero diagnostics, no violations).
    - The test asserts the fresh template contains at least one AgentRef with `activates: Some(Capability::Trader)`.
    - The test asserts no AgentRef has `activates: None` (every binding is explicit).
  - **All other tests still pass**. Existing strategy fixtures that supplied legacy `trader_slot` shapes continue to work; the change is additive on the builtin templates.
  - **Validator side effects**: if the validator was emitting "missing trader" diagnostics on the fresh template prior to this PR, those diagnostics now disappear. No new validator code is added — Phase A's validator already accepts capability-declared templates.
  - **Pre-launch breaking change**: removed. Phase E does not change template names or slot names; it only adds capability declarations. Operators with strategies derived from a builtin template are unaffected (their saved `Strategy` JSON did not previously carry `activates`; it still doesn't need to, because the dispatcher falls back to the slot's first capability per Phase B Decision 1).
  - **Comment in `templates.rs`** at module top: brief note that the capability declarations are required, point at the spec by date + filename, and warn future template authors that adding a new template without capability declarations will trip the `validate_draft_succeeds_for_fresh_template` regression test.

---

# Scope

Phase E of `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`. Closes the QA carryover from PR #369: `validate_draft_succeeds_for_fresh_template` has been expected-fail since 2026-05-20 because the canonical strategy template ships no trader agent.

Phase E retrofits every starter template with explicit capability declarations matching the spec table, then flips the test back to expected-pass.

This is a leaf — no dispatcher changes, no schema changes, no UI changes. Just template values + one test annotation flip.

# Out of scope

- New starter templates. The spec's set is canonical for now.
- Renaming existing slot names. `trader` / `risk_check` / `executor` etc. survive verbatim per spec Decision 3 (role labels are display-only).
- Validator changes. Phase A's validator already accepts capability declarations; Phase E only provides data.
- Authoring path documentation (the agent editor surfacing capability checkboxes is Phase F's UI spec, deferred).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
# Wait for Phase A (#527) to merge.
git worktree add .worktrees/agent-graph-template-capabilities \
  -b task/agent-graph-template-capabilities origin/main
```

Set per-worktree target dir:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-template-capabilities"
```

# Iterative verification loop

```bash
# 1. Confirm the test is expected-fail on the pre-Phase-E base.
cargo test -p xvision-engine validate_draft_succeeds_for_fresh_template 2>&1 | tee /tmp/template-before.log
# Expect: marked expected-fail or #[ignore].

# 2. Add capability declarations to every builtin template.
# 3. Populate `Strategy.agents` for each strategy template with explicit
#    AgentRef.activates values.
# 4. Remove the expected-fail annotation from the test.
# 5. Re-run:
cargo test -p xvision-engine validate_draft_succeeds_for_fresh_template 2>&1 | tee /tmp/template-after.log
# Expect: passes.

# 6. Full suite — assert no regressions in other validator / template tests
cargo test -p xvision-engine --test template_validation
cargo test -p xvision-engine
cargo build --workspace
cargo clippy --workspace --tests -- -D warnings
```

# Notes

- The `risk-checked-trader` template gives the `executor` slot `{Trader}` (not `{Critic}`) per the spec — the executor is the post-risk trader, not a second risk gate.
- `multi-asset-router-with-traders` is the most ambitious template: 5 slots, 3 capabilities (`Router`, `Trader`, `Critic`). It depends on Router shipping in Phase B (operator Q2 resolution: ship in v1). If for any reason Router slips to Phase G, this template's `router` slot would need to be temporarily declared `{Trader}` and the template flagged as draft-only; current plan ships Router in Phase B so this contingency is not in scope.
- `paper-confirmed-live-trader`: the `live_executor` is `{Critic}` because it observes the paper trader's decision and approves/rejects before live execution. This is semantically a second risk layer; v1 ships it as Critic-observation-only (spec Decision 4). v2 may promote it to a veto-capable Critic.
- `news-reader-plus-trader`: the `news_reader` is `{Intern}` — it produces an `InternObservation` that goes into the Trader's briefing as `accumulated["news_reader_output"]`. Existing sequential-pipeline semantics.
- The `Strategy.agents: Vec<AgentRef>` populated by these templates is the post-2026-05-12-strategies-refactor shape; pre-refactor `trader_slot` / `regime_slot` / `intern_slot` fields stay `None` on all builtin templates.
- This is a low-risk, high-leverage PR — it closes the longest-standing QA carryover in the strategy-validation surface (it's been red since 2026-05-20). After Phase E lands, the "create from template" path in the dashboard is operator-usable end-to-end with no manual fixup.
