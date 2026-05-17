# Intake — 2026-05-17 — agent run observability (promoted to v1)

The xvision team is hitting too many agent failures we can't introspect — bad
tool calls, opaque model-call costs, no per-run timeline of what each agent
actually did. The "observability feature" sitting in V2-territory is being
promoted to v1 because debugging the agent pipeline is now blocking forward
work.

## Source spec

- `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md` (464 lines,
  status: **Draft for evaluation**)

The spec defines an `AgentRun` data model with structured spans, model calls,
tool calls, approvals, sandbox results, supervisor notes, plus OTel export and
a Run Detail UI with an agent timeline. Spec snippet:

> The first follow-up item is intentionally narrow:
> 1. Build the trace/report layer only.
>
> Subsequent items, after the trace/report layer exists, should cover:
> - harness adapter interface
> - tool registry normalization
> - approval/sandbox policy wiring
> - autoresearcher ingestion contract

## Current fit

What already exists:

- `crates/xvision-engine/migrations/002_eval.sql` — `eval_runs`,
  `eval_decisions`, `eval_equity_samples`, `eval_findings`. Per-decision rows
  capture action / conviction / justification / fills, **not** per-model-call
  prompts, tokens, costs, or tool calls.
- `crates/xvision-engine/migrations/001_api_audit.sql` — coarse-grained
  request/response audit on the dashboard API (not agent-internal).
- `crates/xvision-engine/migrations/013_cli_jobs.sql` — remote CLI job rows
  and output chunks.
- Eval review wave (#186/#188/#190, merged 2026-05-16) — **post-hoc**
  analytical reviews of completed eval runs. Different surface; does not
  capture inside-the-run agent traces.

What is missing (every item is greenfield):

- `agent_runs`, `model_calls`, `tool_calls`, `approvals`, `sandbox_results`,
  `supervisor_notes`, `run_spans` tables (or one merged schema).
- OTel span emission in `crates/xvision-engine/src/agent/**` and
  `agent/llm.rs`.
- `xvn_run.json` and `xvn_report.md` export.
- Run Detail UI with agent timeline, separate from existing
  `/eval-runs/:id`.

## Why the spec is NOT ready for implementation

The spec explicitly self-gates:

> ## Evaluation Gate
> This spec is **not** an implementation plan yet.
> It must be evaluated, reduced to an implementable sequence, and mapped to the
> current xvision/xvn codebase before work begins.

And there is no corresponding plan file in `docs/superpowers/plans/`. The
spec also leaves three provisional decisions open:

1. **Harness choice.** Spec leans toward "evaluate Cline SDK first" but is
   explicit that this is provisional and must be checked against the existing
   xvision agent code (`crates/xvision-engine/src/agent/`).
2. **Span storage shape.** Spec describes spans both as Rust `tracing` +
   OTel export AND as durable rows. The split between in-process spans (OTel
   collector) vs. SQLite-persisted rows needs a decision before migrations
   are written.
3. **Prompt retention.** Spec says "avoid storing giant raw prompts by
   default; hashes are the canonical long-lived record; full payloads only in
   explicit debug mode." That toggle (`XVN_AGENT_RUN_FULL_PAYLOAD=1`?) needs
   a config surface before the schema is locked.

## Raw items → tracks

This wave is **not yet decomposed into leaf contracts**. The first track is
foundational planning — once the plan exists, the conductor decomposes leaves.

| Raw item | Track | Lane |
|---|---|---|
| Evaluate spec against current codebase, write the trace/report-layer implementation plan, leave open questions resolved or explicitly deferred | `agent-run-observability-foundation` | foundation |
| Implement `agent_runs` + `run_spans` + `model_calls` schema and Rust persistence | (TBD post-foundation) | foundation |
| OTel `tracing-opentelemetry` bridge + span emission in agent pipeline | (TBD post-foundation) | leaf |
| `xvn_run.json` / `xvn_report.md` export | (TBD post-foundation) | leaf |
| Run Detail UI + agent timeline | (TBD post-foundation) | leaf |

## Out of this intake

- The full harness adapter / tool registry normalization / approval+sandbox
  policy wiring — defer until the trace/report layer ships.
- Autoresearcher ingestion contract — depends on `xvn_run.json` schema
  stabilizing first.

## Decision: parallel with V2A?

The V2A onboarding leaves on `team/board-v2.md` are independent and shouldn't
block. The foundation track is one author writing one plan; safe to run in
parallel with V2A.
