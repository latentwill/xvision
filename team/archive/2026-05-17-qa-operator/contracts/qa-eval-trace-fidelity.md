---
track: qa-eval-trace-fidelity
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-eval-trace-fidelity
branch: task/qa-eval-trace-fidelity
base: origin/main
status: blocked
depends_on:
  - agent-run-observability-ipc-emission       # in PR #224 — provides model_call_finished + span data
  - agent-run-observability-ipc-emission-v2    # needed for prompt/completion preview (assistant_text_delta + per-iteration ModelCallStarted)
  - agent-run-observability-sse-stream         # needed to surface streaming completions live
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/TraceDock.test.tsx
  - frontend/web/src/features/agent-runs/StripDockSlot.tsx
  - frontend/web/src/features/agent-runs/StripDockSlot.test.tsx
  - frontend/web/src/features/agent-runs/SpanDetail.tsx
  - frontend/web/src/features/agent-runs/SpanDetail.test.tsx
  - frontend/web/src/api/agent-runs.ts
forbidden_paths:
  - crates/**
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
parallel_safe: false
parallel_conflicts:
  - "qa-remove-post-hoc-live-toggle: also under features/agent-runs/. Coordinate so removing the toggle doesn't undo span-rendering changes."
  - "qa-eval-running-status-streaming: edits adjacent components on the same surface. Coordinate disjoint files; rebase smaller diff onto larger."
  - "qa-trace-json-download: adds a download control to the same TraceDock. Coordinate UI region."
  - "qa-trace-error-surfacing: also edits SpanDetail / span rendering. Coordinate region — error display vs prompt/model fidelity are different rows but same component."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run trace-dock agent-runs
  - pnpm --dir frontend/web build
acceptance:
  - Each span on the trace strip displays the model id the slot
    actually invoked (sourced from the per-call event payload), not the
    strategy's default model
  - Span detail view shows the prompt that was sent and the completion
    that came back (at least a preview, sourced from the schema landed
    by `agent-run-observability-schema` #200)
  - Arrow icons on the trace strip are at least 14px (or
    `text-base`-sized in Tailwind units) and read clearly at normal
    viewport zoom
  - No regression to span navigation, span selection, or the timeline
    layout
  - No `border-white`/`border-gray-100`/`border-gray-200`/`#fff` on
    dark mode (CLAUDE.md rule)
---

# Scope

The eval trace strip currently mis-reports model and content:

- Spans label the strategy's default model (e.g. `claude-opus-4-7`)
  even when the slot invoked a different per-agent model.
- Spans show top-level labels (`plan`, `review`) but no underlying
  prompt or completion content. The schema landed by
  `agent-run-observability-schema` (#200) carries these on the event
  payload — they just aren't being read into the UI.
- Arrow icons (likely the span-next / span-prev affordance on the
  strip) are too small to read.

Fix the rendering: read per-call model from the event payload, render a
prompt + completion preview inside the span detail panel, and resize
the arrow icons.

If the per-call model id is genuinely missing from the API payload
shape rather than just unread, file a queue note to whichever Phase B
agent-run-observability contract owns the event-bus IPC emission — but
the Phase A schema should already carry it.

# Out of scope

- Removing the `POST-HOC⇄LIVE` toggle (owned by
  `qa-remove-post-hoc-live-toggle`).
- Running-state pill / streaming animation (owned by
  `qa-eval-running-status-streaming`).
- Trace JSON download (owned by `qa-trace-json-download`).
- Error display inside spans (owned by `qa-trace-error-surfacing`).
- Adding new events to the agent-run-observability bus or changing the
  emission cadence.
- Pricing display per span — this contract surfaces model id, not
  cost. Pricing fixes are in `qa-openrouter-pricing-pull`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-eval-trace-fidelity \
  -b task/qa-eval-trace-fidelity origin/main
git -C .worktrees/qa-eval-trace-fidelity status
```

# Notes

**Blocked 2026-05-17: Phase B observability IPC emission is in
progress.** Per-call model id + prompt/completion preview both need
real producer events on the bus. The trace dock is currently
rendering whatever Phase A schema can produce without a wired
emitter, which is why every span shows the strategy default model
(probably hardcoded UI fallback).

Once Phase B lands, re-evaluate. The arrow-icon resize was the only
Phase-B-independent piece of this contract; it has been absorbed into
`qa-ui-micro-fixes` so the visual nit ships immediately. Expect this
contract to collapse to "confirm Phase B payload carries
model/prompt/completion and SpanDetail renders them" once unblocked.

Implementation hints:

- The trace event payload shape lives in
  `frontend/web/src/api/types.gen/AgentRun*.ts` (regenerated from Rust).
  Grep `model` in those files to find the per-call field.
- The agent-run-observability schema (migration 018) carries prompt
  text + completion text on the relevant event variants. Read
  `crates/xvision-engine/migrations/018_agent_run_observability.sql` if
  you need to confirm field names.
- For the arrow icon resize, look for `lucide-react`'s `ChevronLeft` /
  `ChevronRight` (or `ArrowLeft` / `ArrowRight`) usage in
  `TraceDock.tsx` or `StripDockSlot.tsx`. Default Lucide size is 24;
  the strip likely overrides to 8–10. Bump to ~14–16.

# Conductor note (2026-05-17, post-Phase-B-PRs)

PR #224 (`agent-run-observability-ipc-emission`) emits
`event.model_call_finished` carrying the real provider + model id per
step, so acceptance criterion #1 (per-call model display) is unblocked
once the dock reads `model_calls.provider / model` from the export
endpoint. Acceptance #3 (arrow-icon resize) is absorbed into
`qa-ui-micro-fixes` per the board.

**Remaining blocked work**: acceptance criterion #2 (prompt + completion
preview) needs `agent-run-observability-ipc-emission-v2` to land
`event.assistant_text_delta` + per-iteration `event.model_call_started`
so prompt-hash and response-hash dereference to real text via the blob
store. The frontend can ship the model-id display immediately on top
of #224/#226, then wire the prompt/completion preview after v2.

Suggested split: this contract owns the prompt/completion preview
(after v2 lands); the model-id display is small enough to absorb into
`qa-ui-micro-fixes` if a frontend worker is already in that area.
