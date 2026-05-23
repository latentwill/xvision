# Live annotation producer + review auto-fire setting

Date: 2026-05-23 · **DRAFT** — open questions in §6 need answers before this becomes an executable plan.

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

## 6. Open questions (answer before this becomes a plan)

These are the questions I owe you before I can write the executable plan + open a PR.

### 6.1 Where does the annotation persistence live?
- (a) **New column on the existing review table**: store `Vec<ReviewAnnotation>` as JSON-blob alongside the rest of the review.
- (b) **New separate `review_annotations` table** with FK to the review row, one row per annotation. Better for querying / filtering by `kind`, worse for migration cost.
- (c) **Reuse an existing observability surface** (e.g. `xvision-observability` has structured events). Likely the cleanest, but I'd need to look at its schema.

### 6.2 Default of `review.autofire_on_eval_complete`?
- (a) **OFF by default** — explicit operator action keeps LLM costs in check.
- (b) **ON by default** — matches the "every run gets a review" mental model and the annotation graph is always available.
- (c) **Tied to a per-strategy attribute** (some strategies auto-review, some don't).

### 6.3 Where in settings does the toggle live?
- (a) `/settings/general` — a new "Review behaviour" section.
- (b) `/settings/providers` — the review LLM provider already lives there.
- (c) New `/settings/review` page.

### 6.4 What's the prompt extension?
The review LLM is already prompted with the run's candle + decision context. The annotation extension needs:
- A new instruction block asking for the structured `Annotation[]` array.
- A JSON schema in the prompt (or function-calling spec) so the LLM emits valid `idx`, `side`, `kind`, …, fields.

Open: who owns this prompt? Is there a single review-prompt file (likely under `crates/xvision-engine/src/agents/templates.rs` or similar), or is it cobbled together at call time?

### 6.5 Cardinality cap?
Per-run, how many annotations max? The handoff sample had 5. A cap is operator-visible (more annotations = more LLM tokens = more $). Suggest:
- 8 max per run, configurable via `review.max_annotations_per_run`.

### 6.6 Live (no-run) mode — keep or kill?
B3 currently supports `?source=live&symbol=BTC/USDT`. Without a stored run, the producer has nowhere to persist annotations. Options:
- (a) Drop the live source entirely — annotations only exist alongside reviews.
- (b) Live mode runs an ephemeral review-style LLM call on demand and serves the annotations transiently (no persistence). Adds infra; useful for "I just want to see live commentary" use cases.
- (c) Live mode returns the most recent stored review for the symbol's most recent run (no on-demand LLM call). Lowest-cost option that keeps the route useful.

### 6.7 Existing strategy review agent — does it already emit structured output?
I need to grep for the existing review code path (`crates/xvision-engine/src/review/…` or wherever it lives) and confirm whether it's text-only today or already emits any structure. The structural shape of the extension depends on that answer.

## 7. Implementation milestones (placeholder — fill in once §6 is answered)

Tentative — exact ordering depends on §6.1 and §6.7.

1. **R1**: extend the review-agent prompt + parser to emit `Vec<ReviewAnnotation>` alongside the existing critique. Persist (per §6.1).
2. **R2**: add `review.autofire_on_eval_complete` setting (per §6.3) + wire `finalize_writer` to enqueue the review job when true.
3. **R3**: replace `build_annotated_run_stub` with a real builder that reads persisted annotations by run_id.
4. **R4**: live mode decision (per §6.6) — either delete the live route or implement the chosen option.
5. **R5**: add a `Run review` button on `/eval-runs/:id` for the manual path; reflect autofire state in the UI.
6. **R6**: per-strategy override on autofire (per §6.2 option c) if §6.2 picks (c).

## 8. Out of scope

- Streaming annotations as candles arrive in real-time. The "live" wording for B3's `?source=live` is about *unbound-by-a-stored-run*, not *streaming*. True streaming is a separate spec.
- Multi-reviewer aggregation. One review per run, one annotation set per review.
- Annotation editing / approval workflow. Annotations are LLM output; operators see them but can't edit them in v1.

## 9. Sources

- B3 surface + payload type: `frontend/web/src/components/chart/v2/surfaces/AIAnnotationDashboard.tsx`, `frontend/web/src/components/chart/v2/types.ts`.
- B3 backend stub: `crates/xvision-engine/src/api/charts_annotated.rs`.
- Spec where the "producer is out of scope" caveat lives: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §9.
- User direction 2026-05-23: live annotation should fit into the strategy review agent; graph generated once a review fires from the user decision; setting for review to auto-fire on eval-run completion.
