---
name: xvision-flywheel-ops
description: Operate and audit the xvision self-improvement flywheel across memory capture, autoresearch, optimizer demo pools, child minting, lineage, velocity, and leakage evidence.
---

# xvision flywheel ops

Use this skill when driving the closed loop: capture, observe, score, distill,
optimize, mint, gate, promote, recall, and demote.

## Read Surfaces

```bash
xvn flywheel status --agent <agent_id> --json
xvn flywheel velocity --agent <agent_id> --days 7 --json
xvn flywheel lineage --agent <agent_id> --limit 20 --json
```

Dashboard equivalents live in the memory/flywheel panels. MCP read equivalents:

- `xvn_flywheel_status`
- `xvn_flywheel_velocity`

## Optimizer Demo Discipline

```bash
xvn optimize memory-demos \
  --target-agent <agent_id> \
  --memory-agent <agent_id> \
  --demo-source frozen-snapshot \
  --holdout-split 70/15/15 \
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
