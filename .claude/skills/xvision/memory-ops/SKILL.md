---
name: xvision-memory-ops
description: Operate xvision's persistent memory layer: inspect Observations and Patterns, preview recall safely, manage Pattern lifecycle, run leakage probes, and avoid F+L+T violations.
---

# xvision memory ops

Use this skill when operating the `xvision-memory` substrate from CLI, dashboard,
or MCP. Memory is trading-critical state: treat every command as an audited
operator action.

## When to use

Run this skill when operating the memory substrate: listing or inspecting
Observations and Patterns, previewing recall safely, managing Pattern
lifecycle (activate / retire / undo-forget), or running leakage probes.

## When NOT to use

- Distilling new Patterns or running the autoresearch gate → use `xvision-autoresearch-ops`
- Reading flywheel velocity, lineage, or training-run example pools → use `xvision-flywheel-ops`
- MCP recall used as a live write path (not preview) → disallowed; stop

## Trigger examples

- List all active Patterns for agent abc123
- Show me the Observations for agent abc123 from last week
- Activate Pattern p_005 for agent abc123
- Retire Pattern p_003 — check lineage first
- Run the leakage regression script
- Preview recall for this embedding without writing to the prompt
- Undo the last forget on agent abc123
- Show me the memory-ops safety contract

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

## Gotchas

**Recalling Observations into live prompts**: Observations are never recalled
into live prompts — only Patterns can be recalled. Doing otherwise violates
the safety contract.

**Missing scenario start in backtests**: Recall must exclude Patterns whose
`training_window_end` overlaps the scenario start. Failing to pass the
scenario date to the recall command leaks future data.

**MCP recall as write path**: MCP recall is preview or agent-assist context
only. Using its output as a direct prompt write path bypasses the case-law
framing requirement.

**Activating without case-law framing**: Patterns entering a prompt require
case-law framing. Activation alone does not satisfy this — the recall layer
must apply the framing at query time.

**Skipping leakage regression**: After any change to recall, query, prompt
rendering, Pattern lifecycle, or provenance, the leakage-regression script
must run. Omitting it leaves F+L+T violations undetected.

**Hard delete without evidence capture**: Before deleting or retiring any row,
capture the before/after JSON. The evidence section is not optional for
lifecycle operations.

## Owner

memory-ops track (`team/contracts/memory-ops.md`)
