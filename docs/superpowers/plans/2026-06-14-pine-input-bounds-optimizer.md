# Pine input bounds → optimizer-enforced search space + auto-generated settings UI

> Follow-up to the Pine ingestion feature (#998). REQUIRED SUB-SKILL for execution:
> superpowers:subagent-driven-development, TDD per work unit; build via `scripts/cargo`,
> all work in `.worktrees/<name>` (off a **post-#998** commit — local `main` checkout may
> lack `pine_import/`), `CARGO_TARGET_DIR` set.

**Date:** 2026-06-14 · **Plan-review gate:** iteration 1 FAILED (Feasibility, Completeness;
Scope passed) — rev 2 below corrects the optimizer-enforcement design and the missing files.

## Problem (verified against #998 surface)

WU3 produces `InputTarget { path, default, min, max, step, kind }` per Pine `input.*` knob
(`pine_import/inputs.rs::input_mutation_targets`). The bounds are **dead metadata**:

1. **Not persisted (F).** `import_pine` (`pine_import/mod.rs` L105–118) builds `ImportOutcome` from
   `map::map_script`'s `Strategy` (the `Strategy {…}` literal is in `map.rs::map_script` ~L874) and
   never calls `input_mutation_targets`. Nothing on `Strategy` carries the bounds.
2. **Optimizer can't enforce them (F).** Mutation values are **chosen by an LLM** —
   `Mutator::propose` (`mutator.rs` L1013) dispatches an `LlmRequest` (L1110) that returns a
   `MutationDiff`; there is **no numeric sampler** to constrain. The diff is materialized by
   `MutationDiff::apply_to(&self, base: &Strategy) -> Strategy` (`mutator.rs` L901) via
   `set_filter_value`/`set_mechanistic_value`/`set_param_value`, with no min/max awareness. So the
   LLM can (and does) propose values outside the author's declared ranges.

**Goal:** a strategy carries its tunable bounds; the optimizer **enforces** them on every proposed
mutation (hard clamp at apply time, plus surfacing them to the proposing LLM so it proposes in-range);
and the bounds render as an **auto-generated settings panel** on the strategy page.

## Non-goals
- Changing the optimizer's *search strategy* / LLM proposal logic beyond (a) surfacing bounds in the
  prompt context and (b) clamping the materialized value. No new sampler.
- A brand-new edit endpoint. (A `PUT /api/strategy/:id/mechanical_params` already exists, `server.rs`
  L31; WU-C may reuse it for in-bounds edits, else read-only v1.)
- Bounds for non-Pine strategies (empty list → behavior identical to today).

## Architecture
Add a persisted, serde-default bounds list to `Strategy`, exactly mirroring how #998 added
`briefing_indicators` (`#[serde(default, skip_serializing_if = "Vec::is_empty")]` + `StrategyRaw`
mirror + manual `Deserialize` + the workspace-wide `Strategy{}`-literal ripple — ~200 sites, found
via `cargo build --workspace`). `import_pine` populates it from `input_mutation_targets`. The mutator
**clamps** LLM-proposed values to the bounds in `apply_to`, and `program_view::to_markdown` surfaces
the ranges so the LLM proposes in-range; `program_view::from_markdown` must carry the field through
the round-trip or it is stripped before the mutator sees it. The SPA reads the field and renders a
settings strip.

## Work units

### WU-A — persist tunable bounds on `Strategy`
- **Files:**
  - `crates/xvision-engine/src/strategies/mod.rs` — new `TunableBound { path, min, max, step, kind }`
    (reuse `pine_import::InputKind`; or re-export `InputTarget` as the canonical type — pick one and
    state it) + `tunable_bounds: Vec<TunableBound>` field on `Strategy` AND the private `StrategyRaw`
    mirror AND the manual `Deserialize`.
  - `crates/xvision-engine/src/strategies/pine_import/map.rs` (and/or `pine_import/mod.rs::import_pine`)
    — populate `tunable_bounds` from `input_mutation_targets(&script, &outcome)`. (`map_script` owns the
    `Strategy{}` literal; `import_pine` is the natural place to call the WU3 emitter since it has both
    `script` and `outcome`.)
  - `crates/xvision-engine/src/autooptimizer/program_view.rs::from_markdown` — add
    `tunable_bounds: base.tunable_bounds.clone()` (L87–102 clone block) or the field is dropped on the
    mutator's round-trip.
  - **Ripple:** `cargo build --workspace` → add `tunable_bounds: Vec::new()` to every `Strategy{}`
    literal (~200 sites across crates + tests, incl. xvision-mcp/cli/dashboard/engine).
- **Contract:** `import_pine` output carries one `TunableBound` per WU3 `InputTarget`, path-aligned;
  empty for non-Pine; serde round-trips; existing stored JSON unaffected (serde default);
  `validate_strategy` unchanged (confirm it's a no-op for the additive field).
- **Test first:** import a 3-input fixture → 3 `tunable_bounds` (correct paths+bounds); non-Pine
  `Strategy` omits the key (skip-if-empty) and round-trips; `from_markdown` preserves `tunable_bounds`.
- **Verify:** `scripts/cargo test -p xvision-engine pine_import --lib strategies`; `cargo build --workspace` clean.
- **Deps:** none.

### WU-B — optimizer enforces bounds on LLM-proposed mutations
- **Files:**
  - `crates/xvision-engine/src/autooptimizer/mutator.rs` — in `MutationDiff::apply_to` (L901), before
    writing each `ParamChange`/`FilterEdit` value, if `base.tunable_bounds` has a matching `path`,
    **clamp** the `after` value into `[min,max]` honoring `step` (Int rounds; Bool coerces to a valid
    bool). Add a small `clamp_to_bound(value, &TunableBound) -> Value` helper. Paths with no bound are
    written unchanged (strict no-op — the regression guard).
  - `crates/xvision-engine/src/autooptimizer/program_view.rs::to_markdown(_with_resolved_prompts)` —
    surface each tunable path's `[min,max,step]` so the proposing LLM stays in range (soft guide).
  - `crates/xvision-engine/src/autooptimizer/validator.rs::validate_mutation_diff` (L34, has `&Strategy`)
    — OPTIONAL: warn (not hard-fail) when a proposed value is out of bounds, for observability. Clamp in
    `apply_to` is the guarantee.
- **Contract:** for any path with a `TunableBound`, the candidate produced by `apply_to` is always
  within `[min,max]` (step-aligned); Bool bounds yield a valid bool; unbounded paths are byte-identical
  to today.
- **Test first:** build a `MutationDiff` proposing an out-of-`[2,50]` RSI-length and a stop-% above its
  max → `apply_to` yields clamped in-range values; a Bool bound flips validly; a path with NO bound is
  unchanged (regression vs current `apply_to`); `to_markdown` includes the range text for a bounded path.
  (LLM is not called — drive `apply_to` directly with a hand-built diff.)
- **Verify:** `scripts/cargo test -p xvision-engine --lib autooptimizer`.
- **Deps:** WU-A (needs the field + `from_markdown` carry).

### WU-C — auto-generated settings UI
- **Files:**
  - `frontend/web/src/api/strategies.ts` — add `tunable_bounds?: TunableBound[]` (+ a `TunableBound`
    TS type) to the `Strategy` type (note: `briefing_indicators` is also missing from this type since
    #998 — add it too for parity while here).
  - `frontend/web/src/routes/strategies-detail.tsx` — widen the local `StrategyDetail` type and render
    an inline **settings strip**: one row per `tunable_bound` (label derived from `path`, current value
    if resolvable, `min`/`max`/`step`). Empty list → no panel.
  - Optional edit: if wiring to `PUT /api/strategy/:id/mechanical_params` is trivial, allow editing a
    value with client-side `[min,max]` validation; else read-only v1 (state which in the PR).
- **Contract:** the panel is generated entirely from `tunable_bounds`; CLAUDE.md SPA rules (inline,
  single-column, no popups, dark-mode-safe borders, reuse primitives); empty state renders nothing.
- **Test first:** RTL — a mocked strategy with `tunable_bounds` renders one row per bound with range;
  empty `tunable_bounds` → no panel; (if editable) an out-of-range edit is rejected client-side.
- **Verify:** `npm test` + `npm run build` in `frontend/web`.
- **Deps:** WU-A (field on the wire).

## Risks
- **Strategy-literal ripple (WU-A):** ~200 `Strategy{}` sites (not ~65) — mechanical `Vec::new()` per
  site, enumerated by `cargo build --workspace`; serde-default keeps stored data compatible.
- **Round-trip strip (WU-A/B):** `program_view::from_markdown` rebuilds `Strategy` field-by-field;
  forgetting `tunable_bounds` there silently drops bounds before the mutator runs → WU-B can't enforce.
  Covered by a from_markdown round-trip test in WU-A.
- **Clamp must be a no-op for unbounded paths (WU-B):** the regression test against current `apply_to`
  behavior is the guardrail; never alter the write where no bound matches.
- **LLM proposes out-of-range:** expected — the hard clamp in `apply_to` absorbs it; the `to_markdown`
  surfacing only reduces waste, it is not the guarantee.

## Revision log
- **rev 2 (2026-06-14)** — addresses gate iteration 1:
  - *Feasibility BLOCKING (no numeric sampler; mutation is LLM-driven)* → WU-B rewritten: enforce via a
    **clamp in `MutationDiff::apply_to`** (+ optional warn in `validate_mutation_diff`, + bounds surfaced
    in `program_view::to_markdown`), not a non-existent sampler.
  - *Completeness #1 (population site)* → WU-A now names `map.rs`/`import_pine` as the population site.
  - *Completeness #2 (`program_view::from_markdown` strips the field)* → added to WU-A/WU-B files with a
    round-trip test.
  - *Completeness #3 (TS `Strategy` type)* → WU-C now names `frontend/web/src/api/strategies.ts` (+ the
    pre-existing `briefing_indicators` TS gap).
  - *Completeness #4 (enforcement point unspecified)* → WU-B names the exact seam (`apply_to`) + a
    diff-driven test (no LLM).
  - *Ripple undercount* → risk updated to ~200 sites.
