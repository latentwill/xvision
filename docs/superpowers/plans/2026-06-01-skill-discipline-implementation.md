# Skill discipline adoption — implementation plan

> Spec: `docs/superpowers/specs/2026-06-01-skill-discipline-adoption-design.md`
> Branch: `feat/skill-discipline-adoption`
> Date: 2026-06-01

## Checklist

- [ ] Harden `autoresearch-ops/SKILL.md` — add When to use, When NOT to use, Trigger examples, Gotchas, Owner
- [ ] Harden `memory-ops/SKILL.md` — same sections
- [ ] Harden `flywheel-ops/SKILL.md` — same sections
- [ ] Create `.claude/skills/xvision/skill-fixtures.json` — ≥3 positive + ≥1 ambiguity fixture per skill
- [ ] Create `scripts/skill-router-benchmark.sh` — structural validation (no LLM)
- [ ] `git add -f` all docs/superpowers files; commit

## Step 1 — harden autoresearch-ops

File: `.claude/skills/xvision/autoresearch-ops/SKILL.md`

Insert after the intro paragraph:

```
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
```

Append before end of file:

```
## Gotchas
**Single-cohort activation**: Never activate from one Observation cohort — the
invariants section states this. The gate must pass first.

**Wall-clock training cutoff**: The cutoff must come from the latest source
Observation's bar timestamp, not the current date. Using wall clock time
introduces look-ahead contamination.

**Finding written after scores visible**: The blind finding must be composed
before the numeric gate scores are read. Running `xvn autoresearch run` and
then writing the finding retrospectively breaks the audit trail.

**Hard delete without janitor path**: Retiring is soft-delete. Hard delete
requires the explicit memory janitor confirmation flow — skipping it destroys
lineage evidence.

**Wrong skill for inventory inspection**: `xvn autoresearch inspect` shows one
run. Use `xvision-memory-ops` to list all Patterns or Observations across an
agent.

## Owner
autoresearch-ops track (`team/contracts/autoresearch-ops.md`)
```

## Step 2 — harden memory-ops

File: `.claude/skills/xvision/memory-ops/SKILL.md`

Insert after the intro paragraph:

```
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
```

Append before end of file:

```
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
```

## Step 3 — harden flywheel-ops

File: `.claude/skills/xvision/flywheel-ops/SKILL.md`

Insert after the intro paragraph:

```
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
```

Append before end of file:

```
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
```

## Step 4 — create fixtures

File: `.claude/skills/xvision/skill-fixtures.json`

See fixtures spec in the design doc. Must have ≥3 positive + ≥1 ambiguity
fixture per skill and cover the most dangerous disambiguation cases.

## Step 5 — create benchmark script

File: `scripts/skill-router-benchmark.sh`

Pure bash. Parses fixture JSON with jq. Validates structure only — no LLM.
Hard cap MAX_FIXTURES=100.

## Verification

```bash
bash scripts/skill-router-benchmark.sh
# Expected: all PASS, exit 0
```
