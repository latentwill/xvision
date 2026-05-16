# qa10-eval-openrouter-slot-resolution claim

Claimed: 2026-05-16T02:41:02Z
Owner: claude
Worktree: `.worktrees/qa10-eval-openrouter-slot-resolution`
Branch: `qa10-eval-openrouter-slot-resolution`

Scope:

- Stop `xvn strategy new` from baking `provider="anthropic"` into the
  auto-created `AgentSlot` by parsing the template's legacy
  `model_requirement` string. The legacy field describes the policy
  constraint, not the user's provider choice.
- Add explicit `--provider` / `--model` flags to `xvn strategy new` so
  CLI users can configure provider/model at create time.
- Add an eval regression that proves an OpenRouter-configured strategy
  dispatches through `openrouter` (and never touches the Anthropic
  dispatch path) for `eval::run` in paper mode.
- Make `select_eval_provider`'s "no provider configured" surface
  actionable when a seeded AgentSlot has empty provider/model.
