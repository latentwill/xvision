# SSE registry handoff — AR-2 progress events

> For: backend engineer wiring the dashboard live cycle stream
> Date: 2026-05-29
> Source of truth: `crates/xvision-engine/src/autoresearch/progress.rs`
> Related plan: `docs/superpowers/plans/2026-05-09-autoresearcher-3-dashboard.md`
>
> **Status:** follow-up contract stub. The stream taxonomy is defined now; the dashboard-side SSE endpoint / subscriber wiring is the remaining integration step once PR #686 is merged.

## What this registry covers

This handoff covers the *AR-2 cycle progress stream*:
`xvision_engine::autoresearch::progress::{CycleProgressEvent, ProgressChannel}`.

It does **not** change the wire schema. The registry only standardizes:

- the SSE `event:` names used by the dashboard,
- the operator-facing labels shown in the live cycle viewer,
- and the subscription contract for a dashboard consumer.

## Wire names and labels

The dashboard should treat the following `CycleProgressEvent` variants as the canonical SSE registry:

| Rust variant | SSE event name | Display label |
|---|---|---|
| `CycleStarted` | `cycle_started` | Evening run started |
| `ParentSelected` | `parent_selected` | Parent selected |
| `MutationProposed` | `mutation_proposed` | Experiment proposed |
| `MutationGated` | `mutation_gated` | Experiment gated |
| `HonestyCheckRun` | `honesty_check_run` | Honesty check run |
| `JudgeFinding` | `judge_finding` | Judge finding |
| `CycleSealed` | `cycle_sealed` | Evening summary signed |

Notes:

- The SSE `event:` field must be the snake_case wire name.
- The JSON payload should carry the same `kind` value for client-side routing.
- If the payload includes a `display_label`, the dashboard should render that value and only fall back to the local registry when it is absent.

## Subscription contract

The dashboard-side contract is a plain `tokio::sync::broadcast` subscription:

- `ProgressChannel::default()` creates the shared broadcaster.
- The dashboard subscribes via `ProgressChannel::subscribe()`.
- Each received `CycleProgressEvent` is serialized once and forwarded to SSE clients.
- `RecvError::Lagged(n)` is a recoverable drop signal; the dashboard should emit a `lagged` frame and continue or reconnect.
- `RecvError::Closed` is terminal for the stream.

Minimal payload shape for SSE frames:

```json
{
  "kind": "cycle_started",
  "display_label": "Evening run started",
  "data": { /* CycleProgressEvent payload */ }
}
```

## Implementation surface for the dashboard

The eventual dashboard wiring should keep the registry in one place and fan it out to the client:

- Rust helper for `CycleProgressEvent -> event name`
- Rust helper for `CycleProgressEvent -> display label`
- SSE handler that subscribes to `ProgressChannel`
- Client-side router that keys off `kind`

The helper should be exhaustive over the enum so new variants fail loudly in code review and CI.

## Why this is a follow-up rather than the main implementation

PR #686 is the orchestrator / evening-cycle merge point. Until that lands, there is no stable dashboard route to subscribe to the live cycle stream end-to-end, so this document is the smallest concrete artifact that prepares the contract without depending on the merge.

## Acceptance criteria for the follow-up patch

1. Every `CycleProgressEvent` variant has a stable SSE event name.
2. The dashboard uses the same names in its registry and in its SSE frames.
3. The dashboard render path prefers `display_label` and falls back to the registry.
4. The subscription code handles lagged and closed receivers without panicking.
5. A smoke test proves the stream can emit at least one known AR-2 event and label it correctly.
