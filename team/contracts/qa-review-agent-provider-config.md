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
  - crates/xvision-dashboard/src/routes/eval/review.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/eval/review/**
  - frontend/web/**
  - crates/xvision-execution/**
interfaces_used:
  - config::ProviderEntry / ProviderKind (xvision-core)
  - AgentProfile (xvision-engine::eval::review)
  - build_dispatch_for_profile (xvision-dashboard::routes::eval::review)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-dashboard
  - cargo clippy -p xvision-dashboard -- -D warnings
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
  - Regression tests added inline in `crates/xvision-dashboard/src/routes/eval/review.rs`
    (existing `#[cfg(test)] mod tests` block) assert: (1) review pass
    succeeds when the profile's named provider is unconfigured but
    another provider is configured (provider substitution); (2) review
    pass returns a clearer error than "anthropic not configured" when
    no providers are configured at all; (3) review pass works normally
    when the named provider IS configured (no regression — the
    existing `local-candle` provider test path covers this).
  - Substitution emits a `tracing::warn!` naming the requested provider
    and the chosen fallback so the operator can see what happened in
    server logs. No new agent-run-observability event variants.
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

**Path correction (2026-05-18):** initial contract pointed at
`crates/xvision-engine/src/review/**`, but the error originates in
`crates/xvision-dashboard/src/routes/eval/review.rs::build_dispatch_for_profile`
where the provider lookup happens. The agent_profiles table is owned
by the engine but profile→dispatch resolution is dashboard-side.
allowed_paths corrected.

**Chosen fallback:** provider-aware default. When `profile.provider`
isn't found in `cfg.providers` but at least one other provider is
configured, log a `tracing::warn!` naming the requested vs chosen
provider and proceed with the first configured provider. When
`cfg.providers` is empty, return a clearer error
("no LLM provider configured in Settings → Providers") instead of
naming a specific provider that the operator may not even know is
referenced.

Rationale: operator (Ed) leans toward "let the review run if at all
possible" per memory `feedback_alpha_root_cause`-adjacent posture
(don't silently skip operator-triggered work). Substitution is a
visible warn, not a silent swap.

Append checkpoints / PR links below.
