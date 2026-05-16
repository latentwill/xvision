---
track: qa9-cli-eval-model-resolution
worktree: /root/deploy/xvision/.worktrees/qa9-cli-eval-model-resolution
branch: qa9-cli-eval-model-resolution
phase: implemented-static-verified
last_updated: 2026-05-14T08:45:31Z
owner: codex
---

# Status

Picked up the QA9 CLI/HTTP eval model-resolution board item from
`team/execution-board-2026-05-13.md`.

## Implemented

- Resolve attached agent slots before queued eval launch validation.
- Pass resolved agent slots into the queued background executor instead of an
  empty slot list, so attached OpenRouter/DeepSeek agents actually execute.
- Preflight provider/model pairs against the provider's enabled model list
  before creating the queued run row.
- Added focused regression coverage for rejecting the legacy Anthropic model on
  OpenRouter, accepting the configured DeepSeek model, and preferring attached
  agent slots over legacy strategy slots.
- Added API eval-run coverage that rejects the bad model before inserting a
  queued run row.

## Verification

- Passed: `git diff --check`
- Not available locally: `rustfmt` is not installed on this host.
- Not run locally: `cargo test -p xvision-engine api::eval::tests::eval_provider_model_validation_rejects_legacy_requirement_as_model`
- Not run locally: `cargo test -p xvision-engine api::eval::tests::eval_provider_model_validation_accepts_enabled_agent_model`
- Not run locally: `cargo test -p xvision-engine api::eval::tests::eval_runtime_slots_prefer_attached_agents_over_legacy_slots`
- Not run locally: `cargo test -p xvision-engine --test api_eval_run run_rejects_openrouter_legacy_anthropic_model_before_queueing`

Cargo tests are not run on this deploy host per repository guardrails.
