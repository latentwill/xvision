# 2026-05-10 — Providers registry in `config/default.toml` (SUPERSEDED)

**Status:** The `[intern]` TOML block was removed in 2026-06. Use `[default_llm]` instead.

---

## What changed

`config/default.toml` now declares a `[[providers]]` array. The existing
`[intern]` block keeps working — at load time, an auto-derived
`_default_intern` provider row is synthesized if no `[[providers]]` row
matches its `(provider, base_url, api_key_env)` triple.

## Do I need to do anything?

**No** — existing configs load unchanged. The new shape is purely additive.

## Why

To enable per-arm Intern + Trader model selection in `xvn ab-compare`, the
Inspector UI, and `Fork with different model →`. See spec:
[`docs/superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md`](../superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md).

## Optional cleanup

If you want to make the synthetic `_default_intern` row go away:

1. Add an explicit `[[providers]]` row that matches your `[intern]` triple:

   ```toml
   [[providers]]
   name        = "anthropic"
   kind        = "anthropic"
   base_url    = "https://api.anthropic.com"
   api_key_env = "ANTHROPIC_API_KEY"
   ```

2. Run `xvn provider list` to verify the synthetic row no longer appears.

The repo's `config/default.toml` already includes the explicit row in the v1
release commit.

## New CLI

```
xvn provider list
xvn provider show --name <name>
xvn provider check --name <name> [--probe]
xvn provider add --name <name> --kind <anthropic|openai-compat|local-candle> \
    --base-url <url> [--api-key-env <ENV>]
xvn provider remove --name <name>
```

## New `xvn ab-compare` arm-spec syntax

```
trader_arm                                     # unchanged: uses CLI defaults
trader_arm:trader=openai/gpt-4o                # override Trader slot only
trader_arm:intern=anthropic/claude-opus-4-7    # override Intern slot only
trader_arm:intern=…:trader=…                   # both
trader_arm:trader_model=gpt-4o-mini            # shorthand: keep default Trader provider, swap model
```

Auto-suffix gives each `trader_arm` a unique `BacktestResult` row name,
e.g. `trader_arm[gpt-4o]`, `trader_arm[claude-opus-4-7]`. Bare `trader_arm`
keeps its name.
