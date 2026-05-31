---
name: xvision-memory-ops
description: Operate xvision's persistent memory layer: inspect Observations and Patterns, preview recall safely, manage Pattern lifecycle, run leakage probes, and avoid F+L+T violations.
---

# xvision memory ops

Use this skill when operating the `xvision-memory` substrate from CLI, dashboard,
or MCP. Memory is trading-critical state: treat every command as an audited
operator action.

## Safety Contract

- Observations are concrete per-cycle records. They are never recalled into live
  prompts.
- Patterns are distilled or operator-attested semantic memories. Only active,
  non-forgotten Patterns can be recalled.
- Backtests must pass a scenario start. Recall must exclude Patterns whose
  `training_window_end` overlaps that scenario.
- Case-law framing is required whenever Patterns enter a prompt.

## CLI Checks

```bash
xvn memory ls --agent <agent_id> --tier pattern --json
xvn memory ls --agent <agent_id> --tier observation --json
xvn flywheel status --agent <agent_id> --json
bash scripts/leakage-regression.sh
```

For lifecycle work:

```bash
xvn memory activate <pattern_id> --json
xvn memory retire <pattern_id> --json
xvn memory undo-forget --agent <agent_id> --json
```

## MCP Read Tools

- `xvn_memory_list`
- `xvn_memory_get`
- `xvn_memory_recall`
- `xvn_flywheel_status`
- `xvn_flywheel_velocity`

MCP recall requires a caller-supplied embedding. It must be treated as preview
or agent-assist context, not as a write path.

## Evidence To Capture

- CLI transcript for list/recall/lifecycle operations.
- Leakage-regression output after changes touching recall, query, prompt
  rendering, Pattern lifecycle, or provenance.
- Before/after JSON when deleting, retiring, activating, or restoring rows.
