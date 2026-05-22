---
track: agent-firing-filter-cli-verbs
lane: leaf
wave: agent-firing-filter-operator-surface-2026-05-22
worktree: .worktrees/agent-firing-filter-cli-verbs
branch: task/agent-firing-filter-cli-verbs
base: origin/main
status: deferred
depends_on:
  - agent-graph-capability-schema    # PR #527 — MERGED
  - agent-graph-capability-dispatch  # Phase B — pending
blocks: []
stacking: declared:agent-graph-capability-dispatch
allowed_paths:
  - crates/xvision-cli/src/commands/agent.rs
  - crates/xvision-cli/src/commands/strategy.rs
  - crates/xvision-cli/src/lib.rs                            # register new subcommands
  - crates/xvision-engine/src/strategies/validate.rs         # add the soft-warning emission
  - crates/xvision-engine/src/api/strategy.rs                # pipe `acknowledge_no_filter` flag if it lives here
  - crates/xvision-cli/tests/agent_create.rs                 # NEW
  - crates/xvision-cli/tests/strategy_add_filter.rs          # NEW
  - crates/xvision-cli/tests/strategy_validate_warnings.rs   # NEW
forbidden_paths:
  - frontend/web/**
  - crates/xvision-engine/migrations/**         # no migration in Phase 2
  - crates/xvision-engine/src/agents/model.rs   # Phase A owns
  - crates/xvision-engine/src/strategies/agent_ref.rs  # Phase A owns
  - crates/xvision-engine/src/agent/**          # Phase B owns dispatcher
interfaces_used:
  - xvision_engine::agents::Capability (Phase A)
  - xvision_engine::agents::AgentSlot { capabilities, ... } (Phase A)
  - xvision_engine::strategies::agent_ref::AgentRef { activates } (Phase A)
  - xvision_engine::strategies::agent_ref::PipelineEdge { condition } (Phase A)
  - xvision_engine::agent::dispatch_capability::EdgePredicate (Phase B)
  - xvision_engine::strategies::validate::validate_strategy (extends warnings array)
parallel_safe: false
parallel_conflicts:
  - agent-graph-template-capabilities  # both may touch validate.rs warning surface; order so Phase E lands first
verification:
  - cargo fmt --check
  - cargo clippy --workspace --tests -- -D warnings
  - cargo test -p xvision-cli --test agent_create
  - cargo test -p xvision-cli --test strategy_add_filter
  - cargo test -p xvision-cli --test strategy_validate_warnings
  - cargo test -p xvision-engine
  - cargo build --workspace
acceptance:
  - **`xvn agent create` exists** with signature: `--name <n> --capability <trader|filter|critic|intern|router> --provider <p> --model <m> --system-prompt <path-or-string> [--skills <id>...] [--temperature <f>] [--max-tokens <u32>]`. Parses `--system-prompt` as a path if it starts with `@`, otherwise as a literal string. Round-trips: agent created via CLI is identical (modulo ULID) to one created via the SPA.
  - **`xvn strategy add-filter <strategy_id>` exists** with signature: `--filter-agent <agent_id> --gates <role> --when <json-predicate>`. Behavior: appends the filter agent to the strategy's `agents` list with `activates: Some(Capability::Filter)`, adds a `PipelineEdge { from_role: <filter_role>, to_role: <gates>, condition: Some(<parsed predicate>) }`. Errors if `--filter-agent` doesn't exist, isn't Filter-capable, or `--gates` doesn't match any existing AgentRef role.
  - **`xvn strategy remove-filter <strategy_id> --role <filter_role>` exists.** Removes the AgentRef with the matching role AND every PipelineEdge with that role as `from_role`. Idempotent — removing a non-existent role is a no-op warning, not an error.
  - **`--when` argument** takes a JSON literal matching the `EdgePredicate` shape from Phase B (`{"signal":"<name>","field":"<f>","op":"<eq|ne|gt|lt|in>","value":<v>}`). Validates against the predicate enum's serde shape; rejects with a clear error on parse failure.
  - **`xvn strategy validate` emits a soft-warning** when any `AgentRef` with `activates ∈ {Trader, Critic}` has no incoming `PipelineEdge` from a Filter AgentRef. Warning text: `warning: strategy '<name>' has a Trader agent with no upstream Filter — it will dispatch on every bar. Consider adding a Filter to reduce LLM cost. (See: xvn agent create --capability filter)`. Exit code stays 0. If the strategy carries `acknowledge_no_filter: true`, the warning is suppressed.
  - **`xvn strategy create` and `xvn strategy edit`** pass `--no-filter-warning` through to set `acknowledge_no_filter: true` on the saved Strategy JSON.
  - **No new migration.** `acknowledge_no_filter` lives as an optional JSON field on the Strategy blob (the strategies table stores strategy config as JSON per the existing convention). Add `Strategy::acknowledge_no_filter: bool` with `#[serde(default)]` and `#[serde(skip_serializing_if = "std::ops::Not::not")]` so the field is absent on disk for default-false strategies.
  - **Integration tests cover happy paths and three failure modes** per verb: missing required arg, malformed `--when`, non-Filter agent passed to `--filter-agent`.

# Scope

Phase 2 of `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`. CLI surface for creating Filter-capable agents, wiring them into strategies, and surfacing the no-Filter soft-warning.

The CLI is the headless / scripting surface. Operators authoring non-trivial predicates use the SPA composer in Phase 3; the CLI takes JSON literals only.

# Out of scope

- SPA composer for predicates. That's Phase 3.
- Inline Filter agent authoring from the strategy editor (`scope_strategy_id`). That's Phase 3.
- `--when-file` flag for file-form predicates. Deferred until operator demand surfaces.
- A DSL parser at the CLI. Not in this spec.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
# Wait for Phase B (agent-graph-capability-dispatch) to merge — EdgePredicate
# must exist on main before this contract opens.
git worktree add .worktrees/agent-firing-filter-cli-verbs \
  -b task/agent-firing-filter-cli-verbs origin/main
cd .worktrees/agent-firing-filter-cli-verbs
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-firing-filter-cli"
```

# Iterative verification loop

```bash
# 1. Build with new subcommands registered.
cargo build -p xvision-cli

# 2. Smoke-test the new verbs against a scratch DB.
target/debug/xvn agent create --name test-filter --capability filter \
  --provider anthropic --model claude-haiku-4-5 \
  --system-prompt "You are a regime filter. Output JSON: {regime: 'high_vol' | 'low_vol'}."
target/debug/xvn strategy add-filter <id> --filter-agent <agent_id> \
  --gates trader \
  --when '{"signal":"regime_filter","field":"regime","op":"eq","value":"high_vol"}'
target/debug/xvn strategy validate <id>

# 3. Run the test triad.
cargo test -p xvision-cli --test agent_create
cargo test -p xvision-cli --test strategy_add_filter
cargo test -p xvision-cli --test strategy_validate_warnings
```

# Risks

- **`acknowledge_no_filter` placement.** If `Strategy` is stored as JSON and the engine's strategy loader is strict (`deny_unknown_fields`), an additive field needs careful `#[serde(default)]` placement. Verify on a saved-then-loaded round-trip before committing.
- **Soft-warning surface duplication.** The SPA already reads `warnings: Vec<String>` from `validate_strategy()`. Confirm that emitting the warning here automatically threads through to the SPA validation panel — Phase 3 should not need to re-author the warning logic.
- **Filter-agent validity check.** Phase B's `dispatch_capability` may not enforce that a Filter-capable agent actually has `Capability::Filter` in its slot's capabilities set. The CLI must do the check at `add-filter` time to avoid letting operators wire non-Filter agents as gates.
