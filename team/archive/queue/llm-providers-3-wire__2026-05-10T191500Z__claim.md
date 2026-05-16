---
from: llm-providers-3-wire
to: all
topic: claim
created_at: 2026-05-10T19:15:00Z
ack_required: false
---

# `llm-providers-3-wire` track claimed (Phase 3 Tasks 11–12 — wire ProviderRegistry into run_ab_compare)

Session 3 (continuing the Plan #7 thread; Phases 1, 2, and 3 Tasks 9–10 already
merged via PRs #14, #16, #20) takes the wire-up that completes Phase 3.
Worktree `.worktrees/llm-providers-3-wire`, branch
`feature/llm-providers-phase-3-wire`.

## Scope

- **T11** — Rewrite `run_ab_compare` in `xvision-eval/src/ab_compare.rs` to take
  `Arc<ProviderRegistry>` instead of the 4 separate args (intern backend,
  intern_provider, intern_model, trader backend). Per-arm `intern` / `trader`
  slot overrides resolve through the registry; `None` falls back to
  `registry.default_intern` / `default_trader`.
- **T12** — Swap `xvision-cli/src/commands/ab_compare.rs` to build a
  `ProviderRegistry` from `RuntimeConfig.providers` + CLI flag fallback rows
  and call the new `run_ab_compare`. Delete the deprecated old function in
  the same PR.

This completes Phase 3. Without it, the registry merged in PR #20 is
unused dead code; with it, `xvn ab-compare --arms 'trader_arm:trader=anthropic/claude-opus-4-7,trader_arm:trader=openai/gpt-4o'`
actually pits the two LLMs against each other.

## Files this track touches

- `crates/xvision-eval/src/ab_compare.rs` (modify `run_ab_compare` signature + body)
- `crates/xvision-cli/src/commands/ab_compare.rs` (rewrite CLI driver to build registry)

Zero overlap with active sessions:
- `eval-3c-findings` (PR #19): `crates/xvision-engine/src/eval/findings/`
- `frontend-2-home-and-health` (PR #13): `crates/xvision-engine/src/api/health.rs`
- `frontend-2-settings` (PR #18): `crates/xvision-dashboard/`, `frontend/web/`
- `frontend-2-eval-runs` (PR #21): `crates/xvision-engine/src/api/eval/`, `frontend/web/`

## Out of scope (deferred)

- **Phase 4** — `xvn provider` CLI subcommand (list / show / add / remove / check)
- **Phase 5** — UI design lock + migration note

## v1 QA testing value

After this lands, an operator can run a single `xvn ab-compare` invocation
that backtests one strategy against N different LLM providers in parallel,
sharing the same Intern HTTP client when slots match. That is the
load-bearing demo claim from the Plan #7 spec.
