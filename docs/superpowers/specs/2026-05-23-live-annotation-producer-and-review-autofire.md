# Live annotation producer + review auto-fire setting

Date: 2026-05-23 · **PARKED** (halt 2026-05-23T17:15Z; resume target ≤ 2026-05-25T17:15Z) — operator halted mid-dispatch; R1 schema commit lives on `origin/feat/charts-followup-live-annotation-r1-PARKED` (one commit, `5eb02ac`). R2–R6 not started. See §10 "Resume protocol" before restarting.

> **Spec author note:** Tied to two adjacent threads:
> - Chart-rework spec Track B B3 (`/charts/annotated`) currently fetches annotations from a fixture-backed backend stub. Live mode returns `annotations: []`; the surface renders an "annotation producer not configured" EmptyState.
> - The Strategy Review Agent is the natural producer surface for these annotations — operators already trigger reviews from a user decision, and the agent already inspects candle context to author its critique. Annotations are a structured by-product of that review.

## 1. Purpose

Today, B3's `?source=live` path is a placeholder. We need:

1. A real producer that turns a backtest run (or live-symbol window) into a structured `Annotation[]` stream that B3's `AIAnnotationDashboard` can render.
2. The producer should not be a standalone "annotation worker" — it should fold into the **Strategy Review Agent's** existing review-firing path so we don't pay for two LLM passes over the same candles.
3. An operator-facing setting that controls **whether the review (and therefore the annotation generation) auto-fires on eval-run completion**, or whether it always waits for an explicit user trigger.

Per user direction 2026-05-23:
- Live annotation should fit in with the strategy review agent.
- The graph is generated once a review fires (from the user decision).
- There should also be a setting for a review to auto fire or not on completion of an eval run.

## 2. Where annotations come from (today vs. tomorrow)

### Today
- `crates/xvision-engine/src/api/charts_annotated.rs::build_annotated_run_stub` returns 5 hard-coded sample annotations alongside synthesised candles.
- `build_annotated_live_stub` returns the candles + an empty `Vec<Annotation>`.
- Frontend: B3 surface (`AIAnnotationDashboard`) handles the empty case with `EmptyState`.

### Tomorrow
- A review fires (either auto-fire on eval-run completion, or operator-triggered from the eval-run detail page / strategy detail page).
- The review agent emits a `ReviewResult` (existing — track down the exact type in `xvision-engine` / `xvision-observability`) **plus** a structured `Vec<ReviewAnnotation>` shaped to overlap the chart-v2 `Annotation` type defined in `frontend/web/src/components/chart/v2/types.ts`.
- The annotations are persisted alongside the review (same DB table, same lifecycle).
- B3's `?source=run` endpoint returns those persisted annotations directly. `?source=live` continues to be a placeholder for streaming use cases that don't have a stored run.

## 3. Annotation schema (server-side, persisted)

Mirror the frontend `Annotation` type from `frontend/web/src/components/chart/v2/types.ts`:

```rust
pub struct ReviewAnnotation {
    pub idx: u32,                       // candle-array index
    pub side: AnnotationSide,           // Top | Bottom
    pub kind: AnnotationKind,           // Pattern | Flow | Risk | Reversion | Structure
    pub title: String,                  // ≤ 60 chars, headline form
    pub body: String,                   // 12–25 words, plain language
    pub conf: f32,                      // 0.0..1.0
    pub action: AnnotationAction,       // Watch | Long | Short | Caution
    pub danger: bool,                   // tints callout red on the surface
    pub ts_sec: i64,                    // unix seconds, used for the insight log timestamp
}
```

The frontend type stays the source of truth; the Rust type derives ts-rs so the wire shape can't drift.

## 4. Auto-fire-on-eval-complete setting

A new operator setting controls whether an eval-run's completion auto-triggers the strategy review:

```
review.autofire_on_eval_complete: bool   // default: ???  (see §6)
```

Lives in the existing settings surface (`/settings/general`? `/settings/providers`? — see §6). When ON, the `finalize_writer` (or whatever path finalises an eval run today) enqueues a review job. When OFF, the eval run completes and the operator hits the "Run review" button explicitly from `/eval-runs/:id`.

### Why operator-controllable, not hard-coded
- Reviews call an LLM and cost money. Some operators want every run reviewed; others only want curated reviews.
- The annotation graph is a by-product of the review, so the autofire setting governs both.

## 5. End-to-end flow (target)

```
operator → start eval run (xvn eval run --strategy …)
       ↓
eval run completes (executor writes final state)
       ↓
       ├── if review.autofire_on_eval_complete=true → enqueue review job
       └── if false → operator clicks "Run review" on /eval-runs/:id later
       ↓
review job:
   - loads candles + decisions for the run
   - calls the review LLM with a prompt that asks for:
     • a textual critique (existing — already implemented)
     • a structured Vec<ReviewAnnotation> (NEW)
   - persists both atoms in the same transaction (existing review table + new annotations table or new column)
       ↓
B3 surface:
   - /charts/annotated?source=run&run_id=<id> reads the persisted annotations
   - "no annotations yet" EmptyState when the review hasn't fired yet
   - "annotation producer not configured" EmptyState only when the review LLM call is misconfigured
```

## 6. Locked decisions (operator review 2026-05-23)

The first batch of open questions is resolved. The remaining 3 are
research items the implementing subagent will answer by reading the
code (see §6-deferred).

### 6.1 Annotation persistence → **JSON column on the existing review table.**
Single transaction, fast read of all annotations per run. Migration
adds an `annotations TEXT NOT NULL DEFAULT '[]'` (SQLite JSON-as-text)
or `annotations JSONB NOT NULL DEFAULT '[]'::jsonb` (Postgres if that
ever applies) to the review table. Legacy rows backfill to `[]`.

### 6.2 Autofire default → **per-eval attribute, picked at eval creation.**
NOT a strategy-level attribute and NOT a global setting. The eval-run
creation form gains:
- An `auto_fire_review: bool` checkbox.
- A `review_model: { provider, model }` picker that's required when
  `auto_fire_review = true` and optional otherwise (the operator can
  still pick a model later when manually firing).
Manual fire is always available from `/eval-runs/:id` regardless of
`auto_fire_review`; the manual path uses the eval's stored
`review_model` if set, otherwise prompts for one.

### 6.3 Settings page → **N/A — no global settings toggle.**
The eval-creation form is the only configuration surface. No new
settings page or section.

### 6.6 Live (no-run) mode → **show most-recent stored review for the symbol's most-recent run.**
No on-demand LLM call. The `/charts/annotated?source=live&symbol=BTC/USDT`
handler resolves to: latest run for the symbol → latest review for
that run → its annotations. EmptyState when nothing's there yet.

### 6-deferred (implementation-time research, no operator decision needed)

- **6.4 Prompt extension ownership.** The implementing subagent will
  grep the existing review prompt code (likely
  `crates/xvision-engine/src/agents/templates.rs` or `eval/review/…`),
  add the structured-annotation instruction block + JSON schema
  there, and document the change in the implementation PR.
- **6.5 Cardinality cap.** Default to `max_annotations_per_review = 8`
  (configurable in the eval-creation form alongside `review_model`).
  Lift to operator setting later only if it becomes a frequent knob.
- **6.7 Existing review-agent structured output.** The implementing
  subagent will check whether the review agent already emits any
  structure today (vs. plain text). If yes, the annotation field
  joins the existing structured response; if no, the response shape
  becomes structured at the same time annotations land.

## 7. Implementation milestones (R1 → R6)

Ready to dispatch after §6.4/§6.5/§6.7 are answered by code-reading.

### R1 — Schema + persistence
- DB migration: `reviews.annotations TEXT NOT NULL DEFAULT '[]'`.
  Allocate the next number via `team/MANIFEST.md`. Down migration
  drops the column.
- Rust type: `ReviewAnnotation` with ts-rs export so frontend type
  parity is automatic.
- Add `EvalRun.auto_fire_review: bool` + `EvalRun.review_model:
  Option<ProviderModelPair>` + `EvalRun.max_annotations_per_review:
  Option<u32>` (default 8). Either as new columns or in an existing
  JSON config blob — whichever matches the existing eval-run shape.

### R2 — Autofire wiring on eval-run completion
- Where the eval-run finalises (search for `finalize_writer` /
  `mark_complete`): if `auto_fire_review == true`, enqueue a review
  job using the run's `review_model`. If `review_model` is None and
  `auto_fire_review` is true, log a warning and skip (operator
  misconfiguration — don't crash the eval).

### R3 — Review prompt + parser extension
- Add the structured-annotation instruction block + JSON schema to
  the review prompt (location TBD, see §6.4-deferred).
- Parser extracts `Vec<ReviewAnnotation>` from the LLM response,
  validates each item against `idx ∈ [0, candle_count)`, clamps
  `conf` to `[0, 1]`, defaults `danger = false`, fills `ts_sec` from
  the indexed candle's timestamp.
- Persist via R1's schema. Annotations are an atom of the review:
  either both land or both fail.

### R4 — Real /api/v2/charts/annotated builder
- Replace `build_annotated_run_stub` with a real builder that reads
  the persisted `reviews.annotations` for the run_id. If no review
  exists yet, return the run's candles with `annotations: []` and an
  `EmptyState` reason of `"review not yet run"` carried in a new
  optional `note` field (avoid breaking the existing wire shape;
  field added as `note?: string`).
- Update `?source=live`: implement §6.6 — latest run for symbol →
  latest review for that run → annotations.

### R5 — Eval-creation form: auto-fire + review model picker
- Frontend: in `frontend/web/src/routes/eval-runs.tsx` (or wherever
  the eval-launch flow lives), add the two fields per §6.2.
- "Run review" button on `/eval-runs/:id`: always present when the
  run is complete and no review exists yet; the button uses the
  run's stored `review_model` or prompts when it's None.

### R6 — Reflect autofire state in eval-run UI
- Eval-run row in the list view shows a small "auto" pill when
  `auto_fire_review = true`.
- Eval-run detail page shows the chosen `review_model` and the
  current review status (none / queued / running / done / failed).
- B3 `/charts/annotated` shows a "Run review" CTA in its EmptyState
  when annotations are absent and the run hasn't been reviewed yet.

## 8. Out of scope

- Streaming annotations as candles arrive in real-time. The "live" wording for B3's `?source=live` is about *unbound-by-a-stored-run*, not *streaming*. True streaming is a separate spec.
- Multi-reviewer aggregation. One review per run, one annotation set per review.
- Annotation editing / approval workflow. Annotations are LLM output; operators see them but can't edit them in v1.

## 9. Sources

- B3 surface + payload type: `frontend/web/src/components/chart/v2/surfaces/AIAnnotationDashboard.tsx`, `frontend/web/src/components/chart/v2/types.ts`.
- B3 backend stub: `crates/xvision-engine/src/api/charts_annotated.rs`.
- Spec where the "producer is out of scope" caveat lives: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §9.
- User direction 2026-05-23: live annotation should fit into the strategy review agent; graph generated once a review fires from the user decision; setting for review to auto-fire on eval-run completion.

## 10. Resume protocol (added at halt 2026-05-23T17:15Z)

The first dispatch attempt was halted by the operator with **R1 partially landed** and R2–R6 not started. Pick up here in ≤ 48h.

### What's on disk at halt time

| Where | What |
|---|---|
| `origin/feat/charts-followup-live-annotation-r1-PARKED` | One commit `5eb02ac feat(charts): R1 — review-annotation schema foundation`. Migration `035_review_annotations_and_eval_autofire.sql` + `_down.sql`, `ReviewAnnotation` Rust type with ts-rs export, `EvalRun` field additions, supporting tests. **Not reviewed; resume agent must verify before stacking on it.** |
| `.claude/worktrees/charts-followup-live-annotation-r1` (local) | The worktree the killed agent was working in. Stash `halt-2026-05-23` holds the agent's `cargo fmt` sprawl across ~50 unrelated files — **do NOT include those in the resume PR**; they are not the agent's logic changes. Recover the stash only if you actually want the formatting normalization (separate PR). |
| `.claude/worktrees/charts-followup-live-annotation` (local) | Older empty worktree from the very first dispatch attempt. Safe to remove (`git worktree remove --force`). |

### Resume steps

1. **Review the R1 PARKED branch first.** Open
   `feat/charts-followup-live-annotation-r1-PARKED` on GitHub; confirm
   the migration shape, the `ReviewAnnotation` enums (Pattern/Flow/Risk/
   Reversion/Structure; top/bottom; Watch/Long/Short/Caution), and the
   `EvalRun` fields (`auto_fire_review: bool`, `review_model:
   Option<ProviderModelPair>`, `max_annotations_per_review:
   Option<u32>`). If anything's off, rewrite R1 before fanning out.
2. **If R1 is good**, open the PR for it
   (`gh pr create --title "feat(charts): R1 of live-annotation producer — schema foundation" --base feat/charts-followup-b-rollout-b5`)
   so the parallel agents have a stable base to stack on.
3. **Fan out R2 / R3 / R4 / R5+R6 as 4 parallel subagents** off the R1
   branch. The prompts from the halt session (in the parent agent's
   transcript) include task scopes, allowed paths, and acceptance
   criteria — reuse them directly.
4. **Don't redo §6.** The 4 locked operator decisions stand:
   JSON column on reviews table; per-eval autofire selectable at eval
   creation; no global settings page; `?source=live` resolves to
   most-recent stored review for the symbol's most-recent run.

### Why halted

Operator direction 2026-05-23T17:15Z: "Halt process here. Mark as
incomplete and stash for later." Resume window: 48h (target ≤
2026-05-25T17:15Z). No technical blocker — the halt was a
prioritization choice, not a problem with the work.
