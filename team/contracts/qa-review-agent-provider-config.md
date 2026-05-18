---
track: qa-review-agent-provider-config
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-review-agent-provider-config
branch: task/qa-review-agent-provider-config
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agents/templates.rs
  - crates/xvision-engine/src/review/**
  - crates/xvision-engine/src/llm/registry.rs
  - crates/xvision-engine/tests/review_provider_fallback.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-execution/**
interfaces_used:
  - LLM provider registry (Settings → Providers)
  - Agent template registry (research-agent, review-agent profiles)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine
  - cargo clippy -p xvision-engine -- -D warnings
acceptance:
  - An eval run that triggers the review pass on a dashboard with NO
    Anthropic provider configured no longer fails with
    `agent profile 'research-agent' references provider 'anthropic'
    which is not configured in Settings → Providers`.
  - Fallback behavior (one of, picked in the status note):
    (a) Provider-aware default: the review-agent profile resolves
        against whichever provider has at least one configured model;
        if none, the review pass is skipped with a single warning
        emitted on the agent-run-observability bus.
    (b) Skip-or-warn: the review pass logs a single warning and
        produces no review output. The eval run completes successfully.
  - The chosen fallback is documented in the contract Notes section
    before the PR opens, and reflected in `team/status/qa-review-agent-provider-config.md`.
  - Regression test in `crates/xvision-engine/tests/review_provider_fallback.rs`
    asserts: (1) review pass succeeds when Anthropic is unconfigured
    if any other provider has a model; (2) review pass degrades to
    warn-and-skip when no providers are configured; (3) review pass
    works normally when Anthropic IS configured (no regression).
  - The warning surfaces in the trace dock's supervisor-notes panel
    (uses the existing `event.supervisor_note` / equivalent variant on
    the agent-run-observability bus — no new event variants).
---

# Scope

Operator hit (2026-05-18): "agent profile `research-agent` references
provider `anthropic` which is not configured in Settings → Providers."
The review-agent / research-agent template hardcodes the Anthropic
provider, so any dashboard without an Anthropic key configured cannot
run a review pass — and the failure aborts the eval rather than
degrading.

Fix is provider-resolution at runtime: either pick the first configured
provider with a matching model, or warn-and-skip. Either keeps the
eval running and is acceptable for v1; pick one and document.

V2 work (user-configurable review-agent profiles via Settings UI) is
parked on `team/board-v2.md` — out of scope here.

# Out of scope

- A user-facing review-agent settings page (V2 roadmap).
- Removing or restructuring the review pass entirely.
- New agent-run-observability event variants. Use existing
  supervisor-note / warn channels.
- Adding new providers to the registry.
- Changing the hardcoded prompts on the review/research agents
  (separate concern; this is purely the provider-resolution path).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-review-agent-provider-config status
git -C .worktrees/qa-review-agent-provider-config log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-review-agent-provider-config \
  -b task/qa-review-agent-provider-config origin/main
```

# Notes

Worker must record the chosen fallback path (provider-aware default vs.
warn-and-skip) here before opening the PR. Operator (Ed) leans
provider-aware default if there's any sane provider to pick — defer to
worker judgment based on what the registry exposes.

Append checkpoints / PR links below.
