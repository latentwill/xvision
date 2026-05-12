# Eval per-agent metrics

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task.
> **Source:** Followup to `docs/superpowers/plans/2026-05-11-agents-page-v1.md` §Downstream impact and dependent on `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md` (must land first).

---

## Goal

Attribute eval decisions, costs, and outcomes to specific **agents**
inside a strategy. Today `BacktestResult` rolls everything up to the
strategy level — useful for "did this strategy work?" but blind to
"which agent inside it produced the alpha?".

Outputs:

- `eval_decisions` rows tag the `agent_id` that produced each decision
- `BacktestResult` gains a `per_agent_metrics: Vec<AgentMetrics>` block
- Dashboard run-detail page shows a per-agent breakdown panel
- Compare-runs handles strategies with different agent sets

## Dependencies

This plan **depends on** `2026-05-12-strategies-refactor-agent-composition.md`.
Without strategies referencing agents, there's nothing to attribute to.

Specifically: `Strategy.agents: Vec<AgentRef>` must exist; the eval
runtime needs to know which `AgentRef` is responsible for each decision.

## Architecture

```
Before:
  eval_decisions { run_id, decision_index, asset, action, conviction, ... }
                                 (no agent attribution)

  BacktestResult {
    summary: RunSummary,          (Sharpe, return, drawdown — strategy-level)
    decisions: Vec<DecisionRow>,
    equity_curve: Vec<EquityPoint>,
  }

After:
  eval_decisions { run_id, decision_index, agent_id, agent_role, ... }
                                  ^^^^^^^^^^^^^^^^^^^ new
                                  agent_id is nullable for back-compat
                                  with pre-refactor runs

  BacktestResult {
    summary: RunSummary,
    decisions: Vec<DecisionRow>,
    equity_curve: Vec<EquityPoint>,
    per_agent_metrics: Vec<AgentMetrics>,        (new)
  }

  AgentMetrics {
    agent_id: String,
    agent_role: String,                          (role in the strategy)
    decision_count: u32,
    avg_conviction: f64,
    avg_decision_latency_ms: u32,                (per-cycle wall-clock)
    avg_tokens_in: u32,
    avg_tokens_out: u32,
    cost_attributed_usd: f64,                    (per-agent LLM spend)
    win_rate: Option<f64>,                       (only when this agent is the executor;
                                                  None for analyst/risk-gate agents)
    pnl_attributed_usd: Option<f64>,             (likewise — only for the executor)
  }
```

### Why role-as-attribution

A strategy has one "executor" role (the agent whose action commits the
trade). All other roles (analyst, risk_check, etc.) contribute to the
decision but don't directly commit it.

PnL and win-rate attribute to the executor only — attributing them to
the analyst would inflate the analyst's score with the executor's work.
Decision count, conviction, latency, tokens, and cost attribute to every
agent involved (each is an independent LLM call).

Determining which role is "the executor": the `PipelineDef` knows the
terminal role for Sequential pipelines; for Single pipelines, there's
only one agent; for Graph (future), the terminal node(s).

## What changes

### Engine

- **Migration 006 (`crates/xvision-engine/migrations/006_eval_per_agent.sql`):**
  - `ALTER TABLE eval_decisions ADD COLUMN agent_id TEXT NULL` (nullable for old rows)
  - `ALTER TABLE eval_decisions ADD COLUMN agent_role TEXT NULL`
  - `ALTER TABLE eval_decisions ADD COLUMN tokens_in INTEGER NULL`
  - `ALTER TABLE eval_decisions ADD COLUMN tokens_out INTEGER NULL`
  - `ALTER TABLE eval_decisions ADD COLUMN latency_ms INTEGER NULL`
  - Index on `(run_id, agent_id)` for the per-agent rollup query

- **`xvision-engine::eval::store::RunStore`:**
  - `record_decision()` gains `agent_id`, `agent_role`, `tokens_in`, `tokens_out`, `latency_ms`
  - `aggregate_per_agent(run_id) -> Vec<AgentMetrics>` — runs the per-agent SQL rollup

- **`xvision-engine::eval::executor::*`:**
  - Each LLM call inside `agent::execute` (or wherever the per-agent dispatch lives) records its agent_id + role + tokens + latency
  - Win-rate / PnL attribution: the executor's `agent_id` is captured on each decision; the post-run aggregator joins to the trade ledger

- **`xvision-engine::api::eval::*`:**
  - `BacktestResult` (the wire type) gains `per_agent_metrics: Vec<AgentMetrics>`
  - `get_run` populates it via `RunStore::aggregate_per_agent`

### Dashboard

- `GET /api/eval/runs/:id` — response gains `per_agent_metrics`
- `GET /api/eval/runs/:id/agents/:agent_id` — (optional) detail view for one agent's decision stream

### Frontend — Run detail page

New section between Equity curve and Findings:

```
┌─ Per-agent ───────────────────────────────────────────────────┐
│                                                                │
│  agent_role     decisions   avg conv   tokens   cost      pnl │
│  analyst             47       0.62      52k    $2.18      —   │
│  trader              47       0.71      18k    $0.91   +$340  │
│  risk_check          47       0.85      12k    $0.55      —   │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

Each row links to the agent's detail page (`/agents/:id`).

### Frontend — Compare runs

When two runs use **different agent sets**, the compare page shows a
"shared roles" view (matching by `agent_role`) plus a "unique agents"
section per run. Operators can manually pair non-matching agents via a
dropdown for direct comparison.

Backward compat: old-shape runs (no agent attribution) render the
existing aggregate view only; the per-agent panel shows a "Pre-agent
refactor — per-agent metrics unavailable" empty state.

## What this enables

- **Agent leaderboards inside a workspace** — "which of my agents has the highest avg conviction in chop regimes?"
- **Cost attribution** — which agent is eating the LLM budget?
- **Drift detection** — when an agent's avg latency or tokens-per-decision spike, surface it
- **Marketplace primitive** — when ERC-8004 attestations land, per-agent metrics become the natural attestation payload (currently you'd attest the whole strategy, opaque to which agent did what)

## File structure

```
crates/xvision-engine/migrations/
└── 006_eval_per_agent.sql            # NEW

crates/xvision-engine/src/eval/
├── store.rs                          # MODIFY — record_decision args; aggregate_per_agent
├── metrics.rs                        # MODIFY — AgentMetrics struct
├── executor/                         # MODIFY — capture per-agent context on each call
└── run.rs                            # MODIFY — BacktestResult gains per_agent_metrics

crates/xvision-engine/src/api/eval.rs # MODIFY — wire response

crates/xvision-dashboard/src/routes/eval_runs.rs   # MODIFY — surface new fields

frontend/web/src/
├── routes/eval-runs-detail.tsx       # MODIFY — add per-agent panel
├── routes/eval-compare.tsx           # MODIFY — handle mixed agent sets
├── components/eval/
│   └── PerAgentPanel.tsx             # NEW

docs/superpowers/plans/
└── 2026-05-12-eval-per-agent-metrics.md   # this file
```

## Tasks

### Task 1 — Migration + store gains agent fields

- Migration 006 adds nullable columns to `eval_decisions`
- `RunStore::record_decision` signature gains agent_id / role / tokens / latency
- Backward-compat: old call sites pass `None` for these fields
- Unit test: insert with agent_id, query back, verify rollup; insert without, verify NULLs

### Task 2 — Per-agent aggregation query

- `RunStore::aggregate_per_agent(run_id) -> Vec<AgentMetrics>`
- SQL: GROUP BY agent_id, agent_role; SUM/AVG tokens, latency, decision count
- Join to trade ledger for PnL attribution (executor only — determined by `agent_role` matching the terminal role of the strategy's PipelineDef)
- Win-rate: count of profitable trades / total executor decisions where action != hold/flat
- Unit test with synthetic decisions across 3 roles

### Task 3 — Executor wires agent context

- Each LLM call inside the per-cycle pipeline records agent_id + role into the decision row
- Token usage from the LLM dispatch is recorded per-call (not summed across the cycle)
- Latency = wall-clock from prompt-send to response-received per call
- Integration test: run an eval against a 2-agent strategy, verify decisions tagged correctly

### Task 4 — BacktestResult wire shape + API

- `BacktestResult.per_agent_metrics: Vec<AgentMetrics>` (ts-export)
- `api::eval::get_run` populates from `aggregate_per_agent`
- Old runs (no agent_id) return empty vec — frontend handles
- Integration test: GET /api/eval/runs/:id returns expected shape for new + old runs

### Task 5 — Run detail PerAgentPanel

- New component renders the table shown above
- Empty state for pre-refactor runs
- Each row links to /agents/:agent_id
- Smoke test in dev

### Task 6 — Compare-runs handles mixed agent sets

- When two runs have different `agent_role` sets, render Shared and Unique columns
- Manual pairing dropdown for non-matching agents
- Smoke test with two synthetic runs

### Task 7 — Cost-per-agent surfacing in agents page

- The agents-page detail view's "Recent runs" panel gains a per-run cost summary
- Reuses `aggregate_per_agent` on a per-agent slice
- New endpoint: `GET /api/agents/:id/runs/:run_id/metrics`
- Closes the loop — the agent's own page shows where its money/decisions go

## Self-review

**Estimated effort:** 2–3 days single-engineer. Tasks 1–3 (engine) ~1.5 days;
Task 4 (API) ~0.5 day; Tasks 5–7 (frontend) ~1 day.

**Risk areas:**

- **PnL attribution accuracy** — only the executor role gets PnL/win-rate. This is correct semantically but if the strategy has no clearly-terminal role (PipelineKind::Graph with multiple terminals, future), attribution becomes ambiguous. Document the rule, surface it in the UI.
- **Backward compat with old runs** — every code path that reads `eval_decisions.agent_id` must handle NULL. Mitigated by Option<String> in the Rust struct and explicit None-handling in the aggregator.
- **Cost calculation** — depends on per-call token counts from the LLM dispatch. Currently captured as part of the audit but not always pushed into eval_decisions. Need to verify the dispatch path actually surfaces these.

**What this plan does NOT solve:**

- Per-agent leaderboards across operators / marketplace ranking (separate feature, F34)
- Real-time per-agent monitoring during a live run (the live daemon doesn't exist yet)
- Attribution for skills (a skill called by an agent doesn't get its own metrics — rolled up into the agent's tally). Skills-level attribution is a separate plan if it becomes load-bearing.

**Open questions for the operator:**

1. **Win-rate for non-binary actions.** If an agent emits an action like "long_open with size 0.4%", is a 0.3% PnL outcome a "win"? Proposal: any positive realized PnL on close = win. Threshold-based attribution is out of scope for v1.
2. **Cost attribution model.** Per-call token counts × provider pricing = cost. Pricing table lives where? Proposal: hardcode for the three major providers in `xvision-core::pricing`; update on quarterly cadence. Alternative: per-provider config in TOML, operator-maintained.
3. **Latency: prompt-to-response, or full cycle?** Proposal: prompt-to-response (the LLM dispatch latency only — excludes tool execution and downstream work). Full cycle would mix concerns from multiple agents.

**Sequencing:** runs strictly after the strategies refactor lands. No
parallel work possible because eval needs Strategy.agents to exist.
