# Strategy filters

Strategy filters are deterministic gates saved on a strategy. They decide
whether the strategy should dispatch on a bar before any LLM agent spends
tokens. Empty filter state means every bar.

## Dashboard

Open `/strategies/:id` and use the **Filter** card.

- Choose `toml` or `json`.
- Save with **Save filter**.
- Use **Clear filter** to return to every-bar dispatch.
- Use **Check eval readiness** after saving when you want backend validation.

The strategy inspector also edits asset universe and cadence in the Manifest
card. Those fields drive multi-asset and timeframe validation; they are not
prompt-only settings.

## QA checks

For filter functionality QA, verify the run exercised the XVN filter system,
not just prompt wording:

- The strategy Filter card says `Filter artifact attached`.
- The eval detail has non-empty filter summaries/events when the filter should
  fire or block.
- Decision provenance is checked: high `noop_skip` or early-stop counts mean
  many rows were synthesized rather than direct model decisions.
- For filter-specific QA, disable or account for skip optimizations when they
  would hide whether the filter changed behavior.

## CLI

Current CLI authoring still supports agent-gated filters:

```bash
xvn agent create \
  --name regime-filter-v1 \
  --capability filter \
  --provider openrouter \
  --model anthropic/claude-sonnet-4.5 \
  --system-prompt @prompts/regime-filter.txt

xvn strategy add-filter <strategy_id> \
  --filter-agent <agent_id> \
  --gates trader \
  --when '{"signal":"regime_filter","field":"regime","op":"eq","value":"high_vol"}'
```

For new operator QA, prefer the dashboard's strategy-level Filter card unless
you specifically need to test `AgentRef` and `PipelineEdge` wiring.

## Related commands

```bash
xvn strategy create --name "BTC 4h filter test" --prompt @prompt.md \
  --provider openrouter --model anthropic/claude-sonnet-4.5 \
  --asset BTC/USD --timeframe 4h

xvn strategy validate <strategy_id> --scenario <scenario_id> --json
xvn strategy clone <strategy_id> --name "variant" --provider openrouter --model <model>
xvn eval run --strategy <strategy_id> --scenario <scenario_id> --mode backtest
xvn eval run --strategy <strategy_id> --scenario <scenario_id> --mode paper
xvn eval run --strategy <strategy_id> --scenario <scenario_id> --mode live
```

Provider/model must be explicit on the strategy's attached agent. Workspace
defaults are not assumed for eval launch.
