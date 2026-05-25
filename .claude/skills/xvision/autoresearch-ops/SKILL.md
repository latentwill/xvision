---
name: xvision-autoresearch-ops
description: Operate xvision autoresearcher distillation: stage Patterns from Observation cohorts, apply numeric gates and blind Findings, promote/demote Patterns, and preserve lineage evidence.
---

# xvision autoresearch ops

Use this skill for offline memory distillation and Pattern promotion work.
Autoresearcher commands are offline-only; do not run them inside a live trading
decision process.

## Standard Flow

```bash
xvn autoresearch run \
  --agent <agent_id> \
  --pattern-text "<candidate Pattern>" \
  --embedding-json '[...]' \
  --json

xvn autoresearch gate <run_id> \
  --metric sharpe \
  --parent-day-score <n> \
  --child-day-score <n> \
  --parent-holdout-score <n> \
  --child-holdout-score <n> \
  --gate-epsilon <n> \
  --finding-text "<blind qualitative finding>" \
  --json

xvn autoresearch promote <run_id> --json
```

## Invariants

- Never promote from a single Observation cohort.
- Numeric gate must pass independently before promotion.
- The Finding is qualitative context, not the verdict.
- Pattern `training_window_end` must come from the latest source Observation
  bar timestamp, not wall clock time.
- Demotion is soft-delete first; hard delete requires explicit operator
  confirmation through the memory janitor path.

## Evidence To Capture

- `xvn autoresearch inspect <run_id> --json`
- Gate input/output JSON, including blind Finding fields.
- Promoted Pattern row and contributing Observation ids.
- Leakage-regression output before declaring a promotion path change done.
