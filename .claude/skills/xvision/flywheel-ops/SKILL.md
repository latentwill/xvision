---
name: xvision-flywheel-ops
description: Operate and audit the xvision self-improvement flywheel across memory capture, autooptimizer, training run example pools, child training, lineage, velocity, and look-ahead-protection evidence.
---

# xvision flywheel ops

Use this skill when driving the closed loop: capture, observe, score, distill,
optimize, train, gate, activate, recall, and retire.

## When to use

Run this skill when auditing or driving the closed improvement loop:
checking flywheel status and velocity, reading lineage, managing training-run
example pools, or gathering weekly/release evidence across the full cycle.

## When NOT to use

- Distilling Patterns or running the autoresearch gate → use `xvision-autoresearch-ops`
- Listing/inspecting Observations or Patterns or running leakage probes → use `xvision-memory-ops`
- Triggering a live trade → no skill applies

## Trigger examples

- Show me the flywheel status for agent abc123
- What is the 7-day velocity for agent abc123?
- List the 20 most recent lineage entries for agent abc123
- Run the memory-demos audit for agent abc123
- Export the flywheel velocity report for the weekly review
- Run the leakage regression and the MCP tests before this release
- Check whether the demo source is frozen-snapshot or fresh-recorder
- What evidence do I need to collect for the flywheel release checklist?

## Read Surfaces

```bash
xvn flywheel status --agent <agent_id> --json
xvn flywheel velocity --agent <agent_id> --days 7 --json
xvn flywheel lineage --agent <agent_id> --limit 20 --json
```

Dashboard equivalents live in the memory/flywheel panels. MCP read equivalents:

- `xvn_flywheel_status`
- `xvn_flywheel_velocity`

## Training Run Example Discipline

```bash
xvn optimize memory-demos \
  --target-agent <agent_id> \
  --memory-agent <agent_id> \
  --demo-source frozen-snapshot \
  --untouched-split 70/15/15 \
  --json

bash scripts/audit-memory-demos.sh --target-agent <agent_id> --memory-agent <agent_id>
```

Use `fresh-recorder` only when non-reproducibility is intentional and recorded.

## Weekly / Release Evidence

```bash
bash scripts/export-flywheel-velocity.sh --agent <agent_id> --days 7
bash scripts/leakage-regression.sh
cargo test -p xvision-mcp mcp_
```

Record which evidence is automated, which is manual, and which requires
live-provider credentials.

## Gotchas

**fresh-recorder without rationale**: Use `fresh-recorder` only when
non-reproducibility is intentional and recorded. Default to `frozen-snapshot`
for reproducible demo pools.

**Wrong split for untouched period**: The untouched split must be 70/15/15.
Non-standard splits invalidate the look-ahead-protection evidence.

**Skipping the MCP test suite**: `cargo test -p xvision-mcp mcp_` is part of
the release evidence checklist. Omitting it leaves MCP surface regressions
undetected.

**Confusing velocity with lineage**: Velocity is a rate metric (patterns/day,
gate pass rate). Lineage is the audit trail of individual activations and
retirements. Use the right command for the right question.

**Running flywheel status during a live cycle**: Flywheel reads are safe any
time, but do not trigger optimize or training-run operations inside a live
trading decision process.

## Owner

flywheel-ops track (`team/contracts/flywheel-ops.md`)
