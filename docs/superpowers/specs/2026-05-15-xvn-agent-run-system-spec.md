# xvn Agent Run System

**Date:** 2026-05-15
**Status:** Evaluated 2026-05-17 — see implementation plan
`docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`
and the Cline SDK sidecar design
`docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md`.

Three open questions resolved by operator decision (recorded in the plan):

1. **Harness:** adopt Cline SDK via a Node sidecar (`xvision-agentd`) with
   a new Rust agent client crate (`xvision-agent-client`). The original
   `crates/xvision-engine/src/agent/` directory is being **deleted** as
   part of the Cline migration; observability emits from the IPC callback
   path in the new client, not from in-process agent code.
2. **Span storage:** SQLite is the canonical local execution ledger;
   OpenTelemetry export is an optional sink derived from the same events.
   Spans use a shared skeleton table plus specialized detail tables
   (`model_calls`, `tool_calls`, `approvals`, …) — not one giant JSON
   row. OTel attributes carry hashes / counts / ids only; full prompts
   never leave SQLite.
3. **Prompt retention:** first-class three-mode policy
   (`hash_only` default | `redacted` | `full_debug`), with explicit
   config / env-var / CLI-flag precedence and a startup warning on
   `full_debug`. Full payloads are never stored implicitly.

The body of this spec is preserved as the design rationale. Where this
spec and the plan disagree, the plan wins. Where the plan and the Cline
SDK sidecar design disagree, the Cline spec wins on architecture and the
observability plan wins on data model.

## Goal

Define a local-first agent run system for xvn that records each agent run as a
structured, replayable artifact and exports:

- OpenTelemetry traces
- `xvn_run.json`
- `xvn_report.md`

The system must capture:

- agent spans
- tool calls
- approvals
- sandbox results
- model calls
- supervisor notes
- final artifact
- financial evaluation link

## Evaluation Gate

This spec is **not** an implementation plan yet.

It must be evaluated, reduced to an implementable sequence, and mapped to the
current xvision/xvn codebase before work begins. In particular, the harness
choice, the trace plumbing, and the report schema may need to adapt to existing
structure rather than replace it wholesale.

## Scope

This spec covers:

- the canonical xvn agent-run data model
- the trace/report layer that persists and renders a run
- the export artifacts consumed by downstream automation
- the UI surfaces needed to inspect a completed run
- the boundary between Rust as source of truth and an external harness

This spec does **not** commit to rebuilding the whole harness from scratch.
The first follow-up item is to build the trace/report layer and reuse or adapt
the existing agent loop where possible.

## System Principles

1. Rust remains the source of truth for strategy, eval, and financial outputs.
2. The agent harness may propose actions, call tools, and summarize results.
3. Deterministic financial evaluation must come from xvn's engine, not the
   harness.
4. Real trading remains blocked in this system until a later safety design
   explicitly permits it.
5. Run artifacts should be local-first, durable, and easy to ingest by
   autoresearcher-style consumers.

## Core Architecture

```text
xvn
├─ Rust trading engine
│  ├─ backtests
│  ├─ evals
│  ├─ market data
│  └─ financial reports
│
├─ Agent harness
│  ├─ model calls
│  ├─ tool registry
│  ├─ approvals
│  ├─ sandbox
│  ├─ supervisor loop
│  └─ structured artifacts
│
├─ Trace layer
│  ├─ OpenTelemetry spans
│  ├─ local run JSON
│  └─ markdown report
│
└─ Web UI
   ├─ strategy eval view
   ├─ agent trace timeline
   └─ final research artifact
```

The boundary is deliberate:

- xvn owns run schema, trace export, eval truth, and final artifacts
- the harness owns model/tool orchestration and context handling
- the UI reads stored run artifacts and trace metadata

## Harness Position

The harness should be treated as a pluggable adapter, not the product core.

The current recommendation is to evaluate **Cline SDK** first, because the
problem here is not “build an agent platform,” but “get reliable tool calls,
structured outputs, and run-loop primitives without inventing them from
scratch.”

That recommendation is provisional:

- the spec must be compared against existing xvision architecture
- the harness may need to adapt to current agent and CLI surfaces
- the final choice must account for tracing, report export, and downstream
  autoresearcher ingestion

## Main Data Model

```rust
AgentRun {
  run_id: String,
  objective: String,
  strategy_id: Option<String>,
  started_at: DateTime,
  finished_at: Option<DateTime>,
  status: RunStatus,

  model_calls: Vec<ModelCall>,
  tool_calls: Vec<ToolCall>,
  approvals: Vec<ApprovalEvent>,
  sandbox_results: Vec<SandboxResult>,
  supervisor_notes: Vec<SupervisorNote>,
  spans: Vec<RunSpan>,

  financial_eval_id: Option<String>,
  final_artifact: Option<FinalArtifact>,
  otel_trace_id: Option<String>,
}
```

### Span Model

Every meaningful action becomes a span.

```rust
RunSpan {
  span_id: String,
  parent_span_id: Option<String>,
  name: String,
  kind: SpanKind,
  started_at: DateTime,
  finished_at: DateTime,
  status: SpanStatus,
  attributes: JsonValue,
}
```

Span kinds:

- `agent.run`
- `agent.plan`
- `model.call`
- `tool.call`
- `approval.request`
- `approval.response`
- `sandbox.exec`
- `supervisor.review`
- `financial.eval`
- `artifact.write`

Use Rust `tracing` plus OpenTelemetry as the underlying spine. The
`tracing-opentelemetry` bridge should connect local spans to OTel-compatible
trace export.

### Tool Call Model

```rust
ToolCall {
  tool_call_id: String,
  tool_name: String,
  input_json: JsonValue,
  output_json: Option<JsonValue>,
  error: Option<String>,
  started_at: DateTime,
  finished_at: Option<DateTime>,
  approval_required: bool,
  approval_id: Option<String>,
}
```

Every tool definition should include:

- `name`
- `description`
- `input_schema`
- `output_schema`
- `risk_level`
- `requires_approval`
- `timeout_ms`

For strategy tools, the policy must be strict:

- `run_backtest`
- `compare_strategies`
- `inspect_trades`
- `mutate_strategy_params`
- `fetch_market_data`
- `score_strategy`
- `write_research_artifact`

### Approval Model

```rust
ApprovalEvent {
  approval_id: String,
  tool_call_id: String,
  reason: String,
  risk_level: RiskLevel,
  requested_at: DateTime,
  decision: ApprovalDecision,
  decided_at: Option<DateTime>,
  decided_by: String,
}
```

Risk levels:

- `safe_read`
- `expensive_compute`
- `file_write`
- `network_call`
- `strategy_mutation`
- `real_trade_blocked`

Real trading is blocked in v1. The system stays eval/backtest-only unless a
later safety design explicitly expands the execution policy.

### Sandbox Result Model

```rust
SandboxResult {
  sandbox_id: String,
  command: String,
  cwd: String,
  stdout: String,
  stderr: String,
  exit_code: i32,
  duration_ms: u64,
  files_changed: Vec<String>,
}
```

Sandboxing for v1 can be simple:

- Docker container
- temporary working directory
- read-only strategy files unless approved
- no live exchange keys mounted
- resource limits

### Model Call Model

```rust
ModelCall {
  model_call_id: String,
  provider: String,
  model: String,
  input_tokens: Option<u64>,
  output_tokens: Option<u64>,
  cost_usd: Option<f64>,
  prompt_hash: String,
  response_text: Option<String>,
  response_json: Option<JsonValue>,
  tool_calls_requested: Vec<String>,
}
```

The system should avoid storing giant raw prompts by default. Hashes are the
canonical long-lived record; full payloads should only be retained in an
explicit debug mode.

### Supervisor Note Model

```rust
SupervisorNote {
  note_id: String,
  role: String,
  content: String,
  severity: Severity,
  created_at: DateTime,
}
```

Example notes:

- "Agent overfit to last 30 days."
- "Backtest result lacks fee/slippage assumptions."
- "Mutation changed too many variables at once."

### Final Artifact

```rust
FinalArtifact {
  artifact_id: String,
  title: String,
  summary: String,
  hypothesis: String,
  evidence: Vec<EvidenceItem>,
  financial_eval_id: String,
  recommendation: String,
  next_experiments: Vec<NextExperiment>,
}
```

This artifact is the bridge into autoresearcher-style ingestion.

## Export Artifacts

### `xvn_run.json`

This is the machine-readable canonical artifact.

```json
{
  "schema_version": "xvn.agent_run.v1",
  "run_id": "run_123",
  "objective": "Improve BTC mean reversion strategy",
  "strategy_id": "btc_mean_reversion_v4",
  "status": "completed",
  "otel_trace_id": "trace_abc",
  "financial_eval_id": "eval_456",
  "model_calls": [],
  "tool_calls": [],
  "approvals": [],
  "sandbox_results": [],
  "supervisor_notes": [],
  "final_artifact": {
    "summary": "...",
    "hypothesis": "...",
    "recommendation": "...",
    "next_experiments": []
  }
}
```

It should be stable enough for autoresearcher and follow-on automation to
ingest directly.

### `xvn_report.md`

This is the human-readable version.

```md
# xvn Agent Run Report

## Objective
Improve BTC mean reversion strategy.

## Strategy
btc_mean_reversion_v4

## Agent Summary
...

## Financial Evaluation
- PnL:
- Max drawdown:
- Sharpe:
- Win rate:
- Benchmark:

## Tool Calls
| Tool | Status | Notes |
|---|---|---|

## Supervisor Notes
...

## Recommendation
...

## Next Experiments
1. ...
2. ...
```

## UI Spec

Create an Agent Run Detail page in the xvn web UI:

```text
Agent Run Detail
├─ Header
│  ├─ objective
│  ├─ status
│  ├─ strategy
│  └─ trace id
│
├─ Financial Eval
│  ├─ equity curve
│  ├─ drawdown
│  ├─ trade table
│  └─ score
│
├─ Agent Timeline
│  ├─ model calls
│  ├─ tool calls
│  ├─ approvals
│  ├─ sandbox results
│  └─ supervisor notes
│
├─ Final Artifact
│  ├─ hypothesis
│  ├─ evidence
│  ├─ recommendation
│  └─ next experiments
│
└─ Export
   ├─ Download xvn_run.json
   ├─ Download xvn_report.md
   └─ Send to autoresearcher
```

The UI should render the run artifact first and treat the trace as a drill-down
view of that run, not as the only source of truth.

## Follow-Up Sequence

This spec should be turned into an implementation plan only after evaluation.
The first follow-up item is intentionally narrow:

1. Build the trace/report layer only.

That first item should deliver:

- `AgentRun` persistence
- OTel span capture
- `xvn_run.json` export
- `xvn_report.md` export
- the run detail UI and timeline view

It should not attempt to rebuild the full harness. The harness can be adapted
later once the trace/report contract is stable.

Subsequent follow-up items, after the trace/report layer exists, should cover:

- harness adapter interface
- tool registry normalization
- approval/sandbox policy wiring
- autoresearcher ingestion contract

## Risks

1. If the trace schema is tied too tightly to one harness, the system will be
   hard to replace later.
2. If the report format is not canonical, autoresearcher ingestion will drift
   from the UI representation.
3. If financial evaluation is allowed to live inside the harness, the agent
   loop will become the source of truth by accident.
4. If the run layer tries to solve orchestration, tracing, and tool reliability
   all at once, the implementation will sprawl.

## Result

When this spec is properly evaluated and implemented in sequence:

- xvn will have a durable run record for every meaningful agent action
- the UI will show a coherent agent trace and financial outcome
- downstream automation can consume `xvn_run.json`
- and the harness can be swapped without losing the canonical run format
