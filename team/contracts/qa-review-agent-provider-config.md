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
  - A POST to `/api/eval/runs/:id/review` on a dashboard with NO
    Anthropic provider configured no longer fails with the cryptic
    `agent profile 'research-agent' references provider 'anthropic'
    which is not configured in Settings → Providers`. The replacement
    behavior is one of (a) or (b) below — the chosen path is
    documented in the contract Notes before the PR opens.
  - Resolution behavior (exactly one of, no ambiguity):
    (a) **Same-kind substitution + skip-with-remediation otherwise.**
        Exact provider-name match wins. If the named provider isn't
        configured but a configured provider has the same
        `ProviderKind` (so the seeded model id remains valid on the
        wire), substitute it with a `tracing::warn!` naming the
        substitution. If neither match is available, return a clearer
        `ApiError::Validation` listing the configured providers and
        what kind of provider would resolve the review. NEVER
        cross-kind substitute — dispatching `claude-sonnet-4-6` to an
        OpenAI-compatible endpoint would 404.
    (b) **Pure skip-or-warn.** Any miss returns the clearer
        `ApiError::Validation`; no substitution at all.
  - Regression tests added inline in `crates/xvision-dashboard/src/routes/eval/review.rs`
    (existing `#[cfg(test)] mod tests` block) assert: (1) same-kind
    substitution path is taken when the named provider is missing but
    a same-kind provider IS configured (asserts the resolver did NOT
    skip); (2) cross-kind substitution is REFUSED (skip-with-error
    listing the configured providers); (3) review pass returns
    skip-with-remediation when no providers are configured; (4) the
    kind-inference helper unit-test covers known + unknown names; (5)
    existing `post_review_persists_inconclusive_when_local_candle_returns_stub`
    covers the no-regression case where the named provider IS
    configured exactly.
  - Substitution (when it fires) emits a `tracing::warn!` naming the
    requested provider, the chosen substitute, and the matched kind.
    No new agent-run-observability event variants.
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

**Chosen path:** (a) — same-kind substitution + skip-with-remediation
otherwise. Initial implementation (PR #256 first revision) used
"first configured provider" which would have dispatched a
`claude-sonnet-4-6` model id to whatever non-Anthropic provider the
operator had — a wire-format failure. Switched to kind-aware
substitution after code review (the local-candle test masked the bug
because mock dispatch ignores the model). Cross-kind substitution is
explicitly refused; the resolver returns a remediation message
listing the configured providers and naming the requested provider so
the operator knows what to add.

The kind-inference helper hardcodes the convention that
`provider="anthropic"` → `ProviderKind::Anthropic`,
`provider="openrouter"`/`"openai"`/`"openai-compat"` →
`ProviderKind::OpenaiCompat`, `provider="local-candle"` →
`ProviderKind::LocalCandle`. Unknown names return `None` (no
substitution attempted) so we don't quietly guess.

Rationale: operator (Ed) leans toward "let the review run if at all
possible" per memory `feedback_alpha_root_cause`-adjacent posture
(no silent skips). Same-kind substitution preserves that intent
without breaking the model id at the wire layer.

Append checkpoints / PR links below.
