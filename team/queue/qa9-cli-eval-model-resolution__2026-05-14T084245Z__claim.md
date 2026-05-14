# Claim: qa9-cli-eval-model-resolution

Worktree: `.worktrees/qa9-cli-eval-model-resolution`

Branch: `qa9-cli-eval-model-resolution`

Owner: codex

## Scope

Fix the QA9 eval model-resolution bug where `POST /api/eval/runs` can queue
successfully, then fail in the background because the queued executor uses a
legacy strategy slot model such as `anthropic.claude-sonnet-4.6` while the
configured runtime provider/model is OpenRouter/DeepSeek.

## Verification plan

- `git diff --check`
- `cargo test -p xvision-engine api::eval::tests::eval_provider_model_validation_rejects_legacy_requirement_as_model`
- `cargo test -p xvision-engine api::eval::tests::eval_provider_model_validation_accepts_enabled_agent_model`
- `cargo test -p xvision-engine api::eval::tests::eval_runtime_slots_prefer_attached_agents_over_legacy_slots`
- `cargo test -p xvision-engine --test api_eval_run run_rejects_openrouter_legacy_anthropic_model_before_queueing`

Cargo verification is CI/non-deploy only on this host per repository
guardrails.
