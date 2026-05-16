---
track: qa10-eval-openrouter-slot-resolution
worktree: /Users/edkennedy/Code/xvision/.worktrees/qa10-eval-openrouter-slot-resolution
branch: qa10-eval-openrouter-slot-resolution
phase: review
last_updated: 2026-05-16T03:30:00Z
owner: claude
---

# What I'm doing right now

Opening PR for the QA10 OpenRouter slot-resolution fix. Eval no longer
silently locks auto-created `AgentSlot`s to Anthropic by parsing the
template's legacy `model_requirement` string.

# Blocked on

nothing

# Delivered

- `xvn strategy new` gets `--provider` / `--model` flags. Seeded
  `AgentSlot`s use the override > slot fields > empty-string precedence
  ladder. The legacy `model_requirement` ("anthropic.claude-sonnet-4.6")
  is no longer parsed as a fallback — it was the source of the QA10
  failure where OpenRouter-policy strategies dispatched through
  Anthropic at runtime.
- `select_eval_provider`'s "no provider configured" error names the
  strategy id and points the operator at the new
  `xvn strategy new --provider <name> --model <id>` flags so empty
  seeded slots surface an actionable recovery path.
- Unit tests in `crates/xvision-cli/src/commands/strategy.rs` cover the
  refactored precedence and the no-longer-baked Anthropic fallback.
- Eval integration regression
  `eval_run_dispatches_through_openrouter_for_openrouter_agent_ref` in
  `crates/xvision-engine/tests/api_eval_run.rs`. Builds a strategy with
  an OpenRouter-configured AgentRef, unsets `OPENROUTER_API_KEY` to
  force a deterministic failure, and asserts the error names OpenRouter
  and never names Anthropic. Pre-launch this assertion would have
  failed because dispatch fell through to Anthropic.
- Pre-existing latent test breakage in `api_eval_run.rs` fixed in
  passing: helper now applies migration 015 so `eval_decisions.reasoning`
  exists when the executor inserts decision rows.

# Verification

```
cargo test -p xvision-cli --lib commands::strategy::tests
cargo test -p xvision-engine --test api_eval_run
```

Both clean.
