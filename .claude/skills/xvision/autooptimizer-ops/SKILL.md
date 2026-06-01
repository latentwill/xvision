---
name: xvision-autooptimizer-ops
description: Operate the xvision autooptimizer: distill Observations into candidate Patterns, run the gate against today and the untouched test period, record blind findings, activate or retire Patterns, and preserve the lineage evidence for audit.
---

# xvision autooptimizer ops

Use this skill for offline Pattern distillation work. AutoOptimizer
commands are offline-only; do not run them inside a live trading
decision process.

## Standard flow

```bash
xvn optimizer run \
  --agent <agent_id> \
  --pattern-text "<candidate Pattern>" \
  --embedding-json '[...]' \
  --json

xvn optimizer gate <run_id> \
  --metric sharpe \
  --baseline-today-score <n> \
  --candidate-today-score <n> \
  --baseline-untouched-score <n> \
  --candidate-untouched-score <n> \
  --min-improvement <n> \
  --finding-text "<finding written blind to the numeric scores>" \
  --json

xvn optimizer activate <run_id> --json
```

## Invariants

- Never activate from a single Observation cohort.
- The gate's numeric decision (Kept / Dropped) must pass independently
  before activation.
- The finding is qualitative context, not the verdict.
- A Pattern's training cutoff must come from the latest source
  Observation's bar timestamp, not wall clock time.
- Retiring is soft-delete first; hard delete requires explicit operator
  confirmation through the memory janitor path.

## Evidence to capture

- `xvn optimizer inspect <run_id> --json`
- Gate input/output JSON, including the blind finding fields.
- Activated Pattern row and contributing Observation ids.
- Look-ahead-protection regression output before declaring an
  activation path change done.
