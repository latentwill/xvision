# SSE display-label registry handoff — autoresearcher events

> For: backend/frontend engineer working on the live cycle viewer
> Date: 2026-05-27
> Source of truth: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md` §11

## TL;DR

Add a registry mapping SSE wire event names (`mutation_proposed`,
`cycle_sealed`, etc.) to operator-readable display labels
("Experiment proposed", "Evening summary signed"). The wire names
stay — they're a protocol contract between the orchestrator and any
subscriber. The dashboard renders the display label.

Two implementations: a Rust-side helper in `xvision-dashboard` that
attaches the display label to each SSE payload, and a JS-side fallback
in `bus.js` so any client that hits the channel directly still gets
readable labels.

## Files in scope

- `crates/xvision-dashboard/src/sse.rs` — add a `display_label(event:
  &AutoresearchEvent) -> &'static str` helper, include the label in
  the SSE event metadata (either as the `event:` field of the SSE
  frame, as a `display_label` JSON field on the payload, or both)
- `crates/xvision-dashboard/static/js/bus.js` — add a wire→display
  map; render using `event.displayLabel` if present, else fall back
  to the JS-side map
- `crates/xvision-dashboard/tests/sse_smoke.rs` — extend to assert
  display labels are emitted

## Files NOT in scope

- `crates/xvision-engine/src/autoresearch/progress.rs` — the
  `AutoresearchEvent` enum definition stays (it's the wire schema)
- Any code that emits events (orchestrator, mutator, judge, etc.) —
  emitters keep emitting the same wire names
- Frontend React SPA — already has its own
  `formatEventName()` helper proposed in the frontend handoff; this
  registry is for the vanilla-JS static SPA at
  `crates/xvision-dashboard/static/`

## The mapping (from terminology lock §11)

| Wire name (stays) | Display label |
|---|---|
| `cycle_started` | Evening run started |
| `mutation_proposed` | Experiment proposed |
| `mutation_evaluating` | Testing experiment |
| `mutation_committed` | Experiment kept |
| `mutation_rejected` | Experiment dropped |
| `mutation_quarantined` | Experiment flagged for review |
| `lineage_forked` | New branch added |
| `judge_wrote_finding` | Reviewer finished notes |
| `canary_outcome` | Honesty check result |
| `diversity_updated` | Variety score updated |
| `ladder_snapshot` | Proposer scoreboard updated |
| `cycle_sealed` | Evening summary signed |
| `cycle_failed` | Evening run failed |

## Rust-side implementation

Add to `crates/xvision-dashboard/src/sse.rs`:

```rust
use xvision_engine::autoresearch::progress::AutoresearchEvent;

/// Maps an event's wire name to its operator-facing display label.
/// The wire name (returned by `event_kind`) is the SSE `event:`
/// frame identifier and the JSON `kind` field on the payload. The
/// display label is what the dashboard renders to the operator.
///
/// See: docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md §11
pub fn display_label(event: &AutoresearchEvent) -> &'static str {
    use AutoresearchEvent::*;
    match event {
        CycleStarted { .. }        => "Evening run started",
        MutationProposed { .. }    => "Experiment proposed",
        MutationEvaluating { .. }  => "Testing experiment",
        MutationCommitted { .. }   => "Experiment kept",
        MutationRejected { .. }    => "Experiment dropped",
        MutationQuarantined { .. } => "Experiment flagged for review",
        LineageForked { .. }       => "New branch added",
        JudgeWroteFinding { .. }   => "Reviewer finished notes",
        CanaryOutcome { .. }       => "Honesty check result",
        DiversityUpdated { .. }    => "Variety score updated",
        LadderSnapshot { .. }      => "Proposer scoreboard updated",
        CycleSealed { .. }         => "Evening summary signed",
        CycleFailed { .. }         => "Evening run failed",
    }
}
```

In the SSE handler, wrap the JSON payload so the display label is
attached:

```rust
let stream = BroadcastStream::new(rx).filter_map(|result| match result {
    Ok(event) => {
        let kind = event_kind(&event);          // wire name
        let label = display_label(&event);      // operator-facing
        let payload = serde_json::json!({
            "kind": kind,
            "display_label": label,
            "data": event,
        });
        let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
        Some(Ok::<Event, Infallible>(Event::default().event(kind).data(json)))
    }
    Err(_) => None,
});
```

Where `event_kind` is the existing match that maps each variant to
its snake_case wire name (already present in `sse.rs`).

## JS-side implementation

Add to `crates/xvision-dashboard/static/js/bus.js`:

```js
// Wire-to-display mapping. Mirrors crates/xvision-dashboard/src/sse.rs::display_label.
// Kept in sync via the SSE registry handoff
// (docs/design/2026-05-27-autoresearcher-sse-registry-handoff.md).
// If the server is sending display_label on the payload, prefer that;
// this map is the fallback when the field is missing.
const DISPLAY_LABELS = {
  cycle_started:         "Evening run started",
  mutation_proposed:     "Experiment proposed",
  mutation_evaluating:   "Testing experiment",
  mutation_committed:    "Experiment kept",
  mutation_rejected:     "Experiment dropped",
  mutation_quarantined:  "Experiment flagged for review",
  lineage_forked:        "New branch added",
  judge_wrote_finding:   "Reviewer finished notes",
  canary_outcome:        "Honesty check result",
  diversity_updated:     "Variety score updated",
  ladder_snapshot:       "Proposer scoreboard updated",
  cycle_sealed:          "Evening summary signed",
  cycle_failed:          "Evening run failed",
};

export function displayLabel(event) {
  if (event && event.display_label) return event.display_label;
  if (event && event.kind && DISPLAY_LABELS[event.kind]) return DISPLAY_LABELS[event.kind];
  return event && event.kind ? event.kind : "Unknown event";
}
```

Then in every place the live cycle viewer renders an event:

```js
const label = displayLabel(event);
// ...render `label` in the DOM, not event.kind
```

## Acceptance criteria

1. `display_label()` helper exists in `crates/xvision-dashboard/src/sse.rs`
   and covers all 13 event variants.
2. SSE payloads include a `display_label` field on the JSON body (or
   another agreed location; document the choice in the handler).
3. `crates/xvision-dashboard/static/js/bus.js` exports a
   `displayLabel(event)` helper with the same mapping as fallback.
4. Every render path in the static SPA that uses `event.kind` for
   display switches to `displayLabel(event)`.
5. The SSE smoke test (`sse_smoke.rs`) asserts that a known event
   payload includes the expected display label.
6. Adding a new event variant requires updating both the Rust and JS
   mappings; document this expectation in a comment at the top of each
   file (and add a CI lint if feasible — match the variant count of
   `AutoresearchEvent` against the entry count of the mapping).

## Test paths

- `crates/xvision-dashboard/tests/sse_smoke.rs` — extend
- New unit test for `display_label()` covering all 13 variants
- Manual smoke: connect to `/api/events`, trigger one event of each
  kind via the orchestrator (or via the test IPC bridge), verify the
  display label appears in the rendered dashboard

## Things to push back on

- Whether to attach the display label as an SSE-level `event:` frame
  parameter, as a JSON payload field, or both. The handoff suggests
  both, but if one is clearly more idiomatic for the rest of the
  codebase, pick that one and document.
- Whether the JS-side fallback map is worth maintaining (duplicates
  state). Alternative: always require the server to send the label.
  Recommendation here is to keep the fallback so the bus.js works
  against an older server version during a rollout.
- If the static SPA at `crates/xvision-dashboard/static/` is being
  deprecated in favor of the React SPA at `frontend/web/`, this
  handoff's JS-side work may be wasted. Confirm before doing it.

## Reference

- Terminology lock §11: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
- AR-3 plan (where the dashboard SSE event taxonomy was specified):
  `docs/superpowers/plans/2026-05-09-autoresearcher-3-dashboard.md`
- Project-wide terminology note: `/CLAUDE.md` §Terminology → "Operator-facing names (autoresearcher subsurface)"
