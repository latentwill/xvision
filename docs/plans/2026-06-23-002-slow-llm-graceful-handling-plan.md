# Plan: Graceful LLM Delay Handling (Skip, Don't Stall)

**Date:** 2026-06-23  
**Author:** Edward Kennedy / agent  
**Status:** Draft → Design settled

## Problem

On sub-15m strategies (1m, 3m, 5m candles), the LLM agent can take
1–4 minutes to produce a trading decision. Currently, the live eval runner
has a single wall-clock timeout (`--live-time-limit-secs 300`) that races the
entire decision cycle. If the model is still deliberating when the timer
fires, the run exits with code 5 — a catch-all `Upstream error` that provides
no diagnostic path and loses all work.

## Scope

Live eval only. Backtest has no wall clock and works correctly. Changes in:

- `crates/xvision-eval/src/live.rs` — tick loop logic
- `crates/xvision-eval/src/runner.rs` — dispatch wrappers
- `crates/xvision-eval/src/metrics.rs` — new skip/delay counters
- `crates/xvision-eval/src/trace.rs` — trace rendering for skips
- CLI arg parsing for new flags
- DB schema for `decisions.delayed` column

## Design: Skip → Don't Stall → Accept Late

### Core logic

```
for each bar arrival:
  if agent_running:
    record DecisionSkipped { reason: "agent_busy", elapsed_ms }
    continue → bar becomes latest context for when agent is free

  else:
    dispatch agent on latest bars (including the current bar)

when agent completes:
  accept trade unconditionally
  if bar_age > stale_data_max_age_ms:   // if set
    flag decision.delayed = true
  else:
    flag decision.delayed = false
```

### What this means in practice

| Tick | Bar | Agent state | Action | Trace |
|------|-----|-------------|--------|-------|
| 1 | Spike bar | idle | Dispatch agent | model_call span begins |
| 2 | Chop bar | running | Skip dispatch | `DecisionSkipped(agent_busy, 45s)` |
| 3 | Chop bar | running | Skip dispatch | `DecisionSkipped(agent_busy, 105s)` |
| 4 | Chop bar | running | Skip dispatch | `DecisionSkipped(agent_busy, 165s)` |
| 5 | Chop bar | done | Accept trade | `TradeAccepted(delayed: true, bar_ref: bar_1)` |

The spike bar gets its decision. The chop bars get skipped. The trade is
flagged delayed for post-hoc analysis. **No bar goes un-dispatched forever** —
as soon as the agent finishes, the latest bar context (which includes the
chop bars) is dispatched fresh.

### Key invariants

- **Agent is never cancelled.** It runs until it produces a decision or
  errors. The only exception is an optional hang belt (`--max-agent-ms`)
  for runaway agents — opt-in, not the default.
- **Tick loop never stalls.** On every bar arrival the tick advances.
  Dispatch is either "fire the agent" or "skip — agent busy."
- **All decisions are accepted.** No trade gate rejection for lateness.
  `delayed: true` is a tag for analysis, not a veto.
- **No concurrent agents.** Single `JoinHandle`. Skip means "wait for this
  one to finish, then dispatch on latest context."

### Why no rejection

The model spent real compute on that decision. If the reference bar is
cold, the model knows — it saw the historical context. Rejecting the
decision after the agent fought for it wastes compute and loses a bar
window. Let it trade, flag it as late, let post-mortem decide.

### Why no "dispatch window factor"

The tick loop advances on bar arrival from the feed. There's no timer to
race. "Dispatch timeout" was a confused concept — what it really described
("advance the clock") happens naturally on every bar tick.

## Flags & defaults

| Flag | Default | Meaning |
|------|---------|---------|
| (none — core behaviour) | always-on | If agent busy → skip dispatch, record DecisionSkipped, advance |
| `--stale-data-max-age-ms` | not set | If set, decisions on bars older than this are flagged `delayed` in the trace/metrics. **Never rejects**. Only tags. |
| `--max-agent-ms` | not set | Hang belt. If set, cancels an agent running >N ms without a decision. Opt-in only. |
| `--max-consecutive-skips` | `5` | Cascade alert threshold. Emit `Degraded` when hit. |

All opt-in guardrails default to **not set** — pure pass-through behaviour
out of the box. The only always-on change is "don't exit 5, just skip and
wait."

## Trace visibility

The trace already has `model_call` spans. We need to add:

### DecisionSkipped events

```
DecisionSkipped {
  reason: "agent_busy",
  bar_timestamp: ISO8601,
  bar_count: 42,
  current_agent_elapsed_ms: 85000,
  agent_id: "claude-sonnet-4"
}
```

Rendered in `xvn eval show`:

```
  Bar @ 2026-06-23T10:00:00Z  ─ SKIP (agent busy — 1m25s elapsed)
  Bar @ 2026-06-23T10:01:00Z  ─ SKIP (agent busy — 2m25s elapsed)
  Bar @ 2026-06-23T10:02:00Z  ─ SKIP (agent busy — 3m25s elapsed)
  Bar @ 2026-06-23T10:03:00Z  ─ DECISION (delayed: true, ref bar: 10:00)
```

### Trace spans

- Add `DecisionSkipped` span type to the trace schema
- Span includes: reason, elapsed_ms, bar_count
- The eventual `TradeAccepted` span references the original bar's timestamp
  and includes `delayed: bool`

### MetricsSummary additions

| Field | Type | Meaning |
|-------|------|---------|
| `skipped_dispatches` | u64 | Total bars where dispatch was skipped (agent busy) |
| `delayed_decisions` | u64 | Decisions accepted but flagged `delayed: true` |
| `forced_cancels` | u64 | (only if `--max-agent-ms` is set) Agents force-cancelled |

## Units of work

### U1: Skip-dispatch + delayed-flag pipeline

**Files:** `live.rs`, `runner.rs`, DB migration

- Remove the single-wall-clock-hard-fail from live eval
- Add `agent_running: bool` + `last_dispatch_ts: Instant` to live state
- On bar arrival with `agent_running == true`: record `DecisionSkipped`, tick forward
- On agent completion: always accept, check staleness → set `delayed` flag
- DB migration: add `decisions.delayed BOOLEAN NOT NULL DEFAULT FALSE`
- If `--max-agent-ms` set: abort agent handle on expiry, log `WARN`

**Outputs:**
- Exit 5 no longer fires on slow model. Run keeps running through skips.
- All decisions are accepted and tagged with latency info.

### U2: Trace + metrics visibility

**Files:** `trace.rs`, `metrics.rs`, `xvn eval show`

- Add `DecisionSkipped` variant to trace span enum
- Wire `DecisionSkipped` events into the trace recorder
- Add `skipped_dispatches`, `delayed_decisions`, `forced_cancels` to `MetricsSummary`
- Render skips in `xvn eval show` output
- Emit `Degraded` event on `--max-consecutive-skips` hit
- Show `delayed` decisions distinctly in summary output

**Outputs:**
- Full trace shows which bars were skipped, for how long, and whether the
  eventual decision was delayed
- Operator sees at a glance: "12 skips, 3 delayed decisions, 0 forced cancels"

### U3: Edge cases

- **Agent errors** (panic/throw inside agentd): record `DecisionSkipped { reason: "agent_error" }`, dispatch fresh on next bar
- **Backtest mode**: no behaviour changes. The "agent busy" path doesn't exist in backtest since all bars are processed sequentially
- **`--stale-data-max-age-ms` not set**: `delayed` is always `false` — pure pass-through
- **`--max-agent-ms` not set**: agent runs indefinitely until it produces a decision or errors

### Non-goals

- Changing the LLM model call itself
- Changing how agents produce decisions
- Performance optimization of the LLM call
- Backwards-incompatible CLI changes
- Any trade gate rejection

## Implementation order

1. U1 — core logic: skip-dispatch + delayed flag
2. U2 — trace rendering, metrics, eval show
3. U3 — edge cases

## Decisions (settled)

- **Agent never cancelled** by default. `--max-agent-ms` is a rare opt-in hang belt.
- **No dispatch-window-factor** — the concept was confused. Tick loop naturally advances on bar arrival.
- **All decisions accepted.** `delayed: true` is a trace/metrics tag, not a rejection.
- **No live- prefix** on flags — these are general eval settings.
- **DB column:** `decisions.delayed BOOLEAN NOT NULL DEFAULT FALSE`
- **Trace must show both skips and eventual delayed decision** — the chain of "busy → busy → busy → accepted (delayed)" tells the full story.
