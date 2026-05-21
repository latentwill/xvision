---
track: track-plan-touches-stage3
lane: foundation
wave: filter-v1
worktree: .worktrees/track-plan-touches-stage3
branch: task/track-plan-touches-stage3
base: origin/task/track-plan-touches
status: in-progress
depends_on:
  - track-plan-touches
blocks:
  - filter-v1-frontend-types-and-panels
  - filter-v1-regression-fixtures
stacking: stacked-on:track-plan-touches
allowed_paths:
  - crates/xvision-filters/Cargo.toml
  - crates/xvision-filters/src/lib.rs
  - crates/xvision-filters/src/events.rs
  - crates/xvision-filters/src/types.rs
  - crates/xvision-filters/src/state.rs
  - crates/xvision-filters/src/runtime.rs
  - crates/xvision-filters/src/indicators.rs
  - Cargo.lock
  - team/contracts/track-plan-touches-stage3.md
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-cli/**
  - crates/xvision-dashboard/**
  - crates/xvision-mcp/**
  - crates/xvision-memory/**
  - frontend/**
  - team/board.md
  - team/board-v2.md
  - team/MANIFEST.md
  - decisions/**
interfaces_used:
  - "xvision_filters Stage 1+2 surface (Filter, FilterId, FilterEvalOutcome, ActivationDecision, ConditionResult, Transition)"
  - serde::{Serialize, Deserialize}
  - ts-rs::TS
  - chrono::{DateTime, Utc}
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build --workspace
  - cargo test -p xvision-filters
  - cargo clippy -p xvision-filters --all-targets -- -D warnings
  - cargo fmt -p xvision-filters --check
  - bash scripts/board-lint.sh
acceptance:
  - "**xvision-filters gains an `events` module** — `events.rs` defines `FilterEventV1`, `SuppressedReason`, and `FilterSummary` per the Filter v1 spec §Export shape. All three derive `Serialize + Deserialize`; types-of-interest also derive `ts_rs::TS` behind the existing `ts-export` feature gate."
  - "**Wire-shape versioning.** `FilterEventV1` carries `schema_version: u32` and exposes the matching `FilterEventV1::SCHEMA_VERSION` constant (= 1). The `_v1` suffix on the struct keeps the v1 reader path future-proof when v2 lands alongside."
  - "**Runtime → event bridge.** `FilterEventV1::from_outcome(filter_id, bar_timestamp, outcome, indicator_snapshot)` maps an `ActivationDecision` to `(triggered, suppressed_reason)`. Active::Trip → triggered=true, no suppression. Active::Hold / Inactive / Warming → triggered=false, no suppression. Cooldown / CappedForDay / SuppressedInPosition → triggered=false, matching `SuppressedReason` variant."
  - "**Indicator snapshot shape.** `indicator_snapshot: BTreeMap<String, f64>` keyed on `IndicatorRef.to_string()` (e.g. `\"ema_20\"`, `\"close\"`). Sparse — only references actually evaluated this bar appear. Constructed engine-side and passed in; the runtime crate does not materialise it."
  - "**Condition leaf indices.** `conditions_passed` and `conditions_failed` carry `u32` indices into `ConditionTree::conditions()` (the flat list the runtime already returns). This keeps the wire shape compact and the indexing identical to what `FilterEvalOutcome.conditions_passed` uses."
  - "**FilterSummary aggregator.** `FilterSummary::from_events(filter_id, events)` walks a per-filter slice and computes `bars_scanned`, `wakeups`, `suppressed_in_position`, `suppressed_cooldown`, `suppressed_daily_cap`, `llm_calls_saved = bars_scanned - wakeups`, `estimated_tokens_saved = llm_calls_saved * AVG_BRIEFING_TOKEN_COST`. Caller groups events by `filter_id` externally. Suppression buckets are ordered to match runtime precedence: in-position, cooldown, daily cap."
  - "**Token-cost constant.** `crate::AVG_BRIEFING_TOKEN_COST: u64 = 50_000` lives in `lib.rs` matching the MANUAL.md §scaling-assumption block. A v1.5 follow-up replaces this with a per-strategy measurement."
  - "**Count reconciliation.** A unit test asserts `bars_scanned - wakeups - suppressed_in_position - suppressed_cooldown - suppressed_daily_cap` is the residual `inactive + warming + hold` count — i.e., every event lands in exactly one bucket."
  - "**Serde stability.** `SuppressedReason` serialises with `#[serde(tag = \"kind\", rename_all = \"snake_case\")]` so the wire shape is `{\"kind\":\"cooldown\"}` etc. (matches the v1 spec's discriminated-union form). `suppressed_reason: None` is omitted from output via `skip_serializing_if`."
  - "**Engine independence guard.** `rg --hidden -n 'use xvision_engine' crates/xvision-filters/` → no hits. The new module imports only from the local `runtime` + `types` modules."
  - "**Stage 3 Part 1 scope.** Engine-side emission (the `ProgressEvent::FilterEvaluated` variant), `RunSummary::filter_summaries`, and the export-shape extension are deferred until after `track-plan-touches-engine` because they require the per-bar hook Stage 2 Part 2 introduces. This contract delivers the engine-independent type layer Stage 3 needs; it does not block the hook-only engine PR."
  - "**Pre-existing carry-ins fixed.** The Stage 2 Part 1 branch shipped with four issues that blocked `cargo test -p xvision-filters`, `cargo clippy -p xvision-filters --all-targets -- -D warnings`, and `cargo fmt -p xvision-filters --check`: `IndicatorName` missing `Ord` (needed by `IndicatorKey`'s `Ord` derive), `Symbol::new(\"BTC/USD\".into())` ambiguous type inference in `state.rs` tests (2 sites), `UtcDay`'s visibility leak through `FilterState::wakeup_day`, and non-fmt-compliant line breaks in `runtime.rs` + `indicators.rs`. Fixed in this PR (minimal mechanical touches) so the verification gates pass."
  - "**Tests required.**"
  - "  - `events::tests::classify_maps_all_activation_decision_variants` — every `ActivationDecision` variant has a (triggered, suppressed) mapping."
  - "  - `events::tests::summary_from_events_counts_each_bucket_once` — each suppression reason is counted exactly once."
  - "  - `events::tests::summary_reconciles_counts` — bars_scanned = wakeups + suppressed_in_position + suppressed_cooldown + suppressed_daily_cap + residual."
  - "  - `events::tests::summary_llm_calls_saved_equals_bars_minus_wakeups` — 5 wakeups out of 100 bars → llm_calls_saved = 95; estimated_tokens_saved = 95 × AVG_BRIEFING_TOKEN_COST."
  - "  - `events::tests::summary_empty_events_zero_everything` — empty input → all-zero summary."
  - "  - `events::tests::event_serde_roundtrip_preserves_shape` — JSON round-trip is identity."
  - "  - `events::tests::event_suppressed_reason_omitted_when_none` — `skip_serializing_if` works."
  - "  - `events::tests::event_suppressed_reason_serialises_with_kind_tag` — `{\"kind\":\"cooldown\"}` shape."
  - "  - `events::tests::from_outcome_threads_runtime_decision_into_event` — runtime → event mapping is correct."
  - "  - `events::tests::from_outcome_trip_sets_triggered_true_and_no_suppression` — Trip path."
  - "**Grep guards.**"
  - "  - `rg --hidden -n 'fn from_outcome' crates/xvision-filters/src/events.rs` → 1 hit."
  - "  - `rg --hidden -n 'AVG_BRIEFING_TOKEN_COST' crates/xvision-filters/` → declared in lib.rs, referenced in events.rs."
  - "  - `rg --hidden -n 'use xvision_engine\\|use xvision_core' crates/xvision-filters/` → no hits."
  - "**No changes outside listed allowed paths.** If implementation forces a touch outside `allowed_paths`, **STOP** and append a checkpoint under `# Notes`."
---

# Scope

Stage 3 Part 1 of the Filter v1 wave. Lands the **engine-independent
event + summary types** in `xvision-filters/src/events.rs`, completing
the type layer the engine-side per-bar hook (Stage 2 Part 2,
`track-plan-touches-engine`) will emit through. Bridges Stage 2 Part 1's
`FilterEvalOutcome` to the spec's `FilterEventV1` shape via the
`from_outcome` constructor.

After this contract:

- `xvision-filters` exposes `FilterEventV1`, `SuppressedReason`,
  `FilterSummary` with the wire shape defined in the spec.
- A pure aggregator (`FilterSummary::from_events`) turns a per-filter
  event slice into the summary the run export and Stage 4 panels will
  render.
- The crate still has zero engine dependencies. The engine-side
  emission, the `ProgressEvent::FilterEvaluated` variant, and the
  `RunSummary::filter_summaries` extension live in a follow-up after
  `track-plan-touches-engine` provides the per-bar hook.

# Out of scope (this PR — Stage 3 Part 1)

- Engine wiring (`xvision-engine` is in `forbidden_paths`). The
  per-bar hook, `ProgressEvent::FilterEvaluated`, the migration adding
  `strategies.activation_mode` + `filters_json`, and the
  trace-span replacement all live in `track-plan-touches-engine`.
- `RunSummary::filter_summaries` and the export-shape extension —
  same reason: they need a real per-bar emit source.
- Frontend types + panels — Stage 4.
- Regression fixtures — Stage 5.
- Live mode integration.

# Merge ordering

This PR is stacked only on `track-plan-touches` and can land without
waiting for `track-plan-touches-engine`. The engine hook PR is allowed
to land independently; the later engine event/export follow-up consumes
both the hook and these type definitions.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/track-plan-touches-stage3 status
git -C .worktrees/track-plan-touches-stage3 log --oneline -3 origin/task/track-plan-touches..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/track-plan-touches-stage3
#   - base is up to date with origin/task/track-plan-touches
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/track-plan-touches-stage3 \
  -b task/track-plan-touches-stage3 origin/task/track-plan-touches
```

# Notes

Free-form. Append checkpoints, surprises, links to PRs.

- 2026-05-21 — contract drafted alongside the Stage 3 Part 1 PR. The
  Stage 2 Part 1 contract (`track-plan-touches`) directed Stage 3's
  contract to be drafted alongside the PR rather than speculatively.
- 2026-05-21 — Pre-existing build/clippy/fmt blockers on the Stage 2
  Part 1 branch (IndicatorName Ord, Symbol::new(into) ambiguity in
  state.rs tests, UtcDay visibility leak) fixed in this PR so the
  verification gates pass. These are minimal mechanical fixes
  documented in the acceptance criteria; they do not constitute scope
  creep.
