---
name: xvision-autooptimizer-ops
description: Operate the xvision autooptimizer: distill Observations into candidate Patterns, run the gate against today and the untouched test period, record blind findings, activate or retire Patterns, and preserve the lineage evidence for audit.
---

# xvision autooptimizer ops

Use this skill for offline Pattern distillation work. AutoOptimizer
commands are offline-only; do not run them inside a live trading
decision process.

## When to use

Run this skill when you are offline, outside a live trading decision, and need
to distill candidate Patterns from Observations, run the gate, inspect a run,
activate a passing run, or retire a Pattern.

## When NOT to use

- Inspecting Pattern or Observation inventory → use `xvision-memory-ops`
- Reading flywheel velocity or overall health → use `xvision-flywheel-ops`
- Any task involving a live trading cycle or real-time data → no skill applies

## Trigger examples

- Distill a new Pattern from last week's Observations for agent abc123
- Run the autoresearch gate on run_id xyz with a Sharpe baseline of 0.8
- Activate run xyz after the gate passed
- Retire Pattern p_001 — soft delete only
- Inspect autoresearch run xyz and show me the gate output JSON
- Write a blind finding for this autoresearch run before I see the scores
- Check whether Pattern p_002's training cutoff is correct

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

## Gotchas

**Single-cohort activation**: Never activate from one Observation cohort — the
invariants section states this. The gate must pass first.

**Wall-clock training cutoff**: The cutoff must come from the latest source
Observation's bar timestamp, not the current date. Using wall clock time
introduces look-ahead contamination.

**Finding written after scores visible**: The blind finding must be composed
before the numeric gate scores are read. Running `xvn optimizer run` and
then writing the finding retrospectively breaks the audit trail.

**Hard delete without janitor path**: Retiring is soft-delete. Hard delete
requires the explicit memory janitor confirmation flow — skipping it destroys
lineage evidence.

**Wrong skill for inventory inspection**: `xvn optimizer inspect` shows one
run. Use `xvision-memory-ops` to list all Patterns or Observations across an
agent.

## Owner

autoresearch-ops track (`team/contracts/autoresearch-ops.md`)
