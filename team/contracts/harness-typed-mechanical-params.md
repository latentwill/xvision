---
track: harness-typed-mechanical-params
lane: integration
wave: harness-observability-audit
worktree: .worktrees/harness-typed-mechanical-params
branch: task/harness-typed-mechanical-params
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/mod.rs
  - crates/xvision-engine/src/strategies/mechanical.rs            # new file — MechanicalParams enum + per-template structs
  - crates/xvision-engine/src/strategies/validate.rs              # single pre-persist validate seam
  - crates/xvision-engine/src/strategies/store.rs                 # route save through validator
  - crates/xvision-engine/src/strategies/templates.rs             # seed strategies use typed constructors
  - crates/xvision-engine/src/templates/breakout.rs
  - crates/xvision-engine/src/templates/custom.rs
  - crates/xvision-engine/src/templates/mean_reversion.rs
  - crates/xvision-engine/src/templates/momentum.rs
  - crates/xvision-engine/src/templates/news_trader.rs
  - crates/xvision-engine/src/templates/range_trade.rs
  - crates/xvision-engine/src/templates/scalping.rs
  - crates/xvision-engine/src/templates/trend_follower.rs
  - crates/xvision-engine/src/authoring.rs                        # set_mechanical_param validates against active variant
  - crates/xvision-engine/tests/mechanical_params.rs              # new integration tests
  - crates/xvision-engine/tests/strategy_roundtrip.rs             # extend existing round-trip
  - crates/xvision-core/src/trading.rs                            # deny_unknown_fields + TP/SL cross-field
  - crates/xvision-core/src/config.rs                             # deny_unknown_fields on RiskConfig/Limits/Stops
  - crates/xvision-core/src/risk.rs                               # deny_unknown_fields on RiskCaps
  - team/contracts/harness-typed-mechanical-params.md
  - team/status/harness-typed-mechanical-params.md
  - team/board.md
forbidden_paths:
  - crates/xvision-engine/migrations/**                           # F-3 owns migrations; F-6 needs none
  - crates/xvision-observability/**                               # F-2/F-4 own observability
  - crates/xvision-engine/src/agent/**                            # F-5 owns recovery dispatcher
  - crates/xvision-engine/src/eval/executor/**                    # F-5 owns failure classification
  - crates/xvision-execution/**
  - crates/xvision-dashboard/**
  - frontend/web/**                                               # F-7 owns trace-dock UI; mechanical_params SPA path unchanged
interfaces_used:
  - xvision_engine::strategies::Strategy
  - xvision_engine::strategies::manifest::PublicManifest          # `template` field is the variant discriminator
  - xvision_engine::strategies::store::StrategyStore              # save path routes through validator
  - xvision_engine::authoring::SetMechanicalParamReq
  - xvision_core::trading::{InternBriefing, TraderDecision, RiskDecision}
  - xvision_core::config::{RiskConfig, RiskLimits, RiskStops}
  - xvision_core::risk::RiskCaps
  - garde::Validate
parallel_safe: true   # disjoint files from F-2/F-3/F-4/F-5/F-7
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine -- strategies mechanical_params authoring api::strategy
  - cargo test -p xvision-engine --test mechanical_params
  - cargo test -p xvision-engine --test strategy_roundtrip
  - cargo test -p xvision-core
  - cargo clippy -p xvision-engine -p xvision-core -- -D warnings
  - cargo build --workspace
acceptance:
  - New module `crates/xvision-engine/src/strategies/mechanical.rs` defines:
      - `enum MechanicalParams` with one variant per template id present in
        `crates/xvision-engine/src/templates/`: `TrendFollower`,
        `MeanReversion`, `Breakout`, `Momentum`, `Scalping`, `RangeTrade`,
        `NewsTrader`, and a fallback `Custom(serde_json::Value)` for
        operator-defined templates that aren't in the canonical set.
      - Each typed-variant struct (e.g. `TrendFollowerParams`,
        `BreakoutParams`, `MeanReversionParams`, `MomentumParams`,
        `ScalpingParams`, `RangeTradeParams`, `NewsTraderParams`) carries
        `#[derive(Validate, Serialize, Deserialize)]` with
        `#[serde(deny_unknown_fields)]`. Field shapes match the existing
        `serde_json::json!({…})` literals in the matching
        `crates/xvision-engine/src/templates/*.rs` constructor (no
        renames, no new fields, no removed fields in this track).
      - Each typed-variant struct exposes `fn min_warmup_bars(&self) -> u32`.
        Implementations match what the legacy `max_indicator_period` JSON
        walker computed for the same params (multiply the largest
        period-like field by 2) so the warmup-bar semantics are
        byte-identical to today's behaviour. The legacy
        `max_indicator_period` helper is removed in favour of typed
        dispatch.
  - `Strategy.mechanical_params` field type becomes `MechanicalParams`.
    Wire format on disk is unchanged: the params object is still the
    flat `{ "ema_fast": 12, … }` shape today's strategies have on disk.
    Variant discrimination is driven by `manifest.template` via a custom
    `Deserialize` impl on `Strategy` (or equivalent two-step parse).
    Unknown / non-canonical template strings deserialize as
    `MechanicalParams::Custom(serde_json::Value)`. Existing strategies on
    disk parse round-trip without any data rewrite.
  - `Strategy::min_warmup_bars` delegates to the typed variant (or to
    `manifest.min_warmup_bars` if explicitly set). The fallback constant
    `FALLBACK_MIN_WARMUP_BARS` is preserved for the `Custom` arm with no
    derivable hint. **No regression on warmup-bar derivation** —
    existing `eval_executor_warmup` tests stay green unchanged.
  - `#[serde(deny_unknown_fields)]` is added to:
      - `xvision_core::trading::InternBriefing`
      - `xvision_core::trading::TraderDecision`
      - `xvision_core::trading::RiskDecision`
      - `xvision_core::config::RiskConfig`
      - `xvision_core::config::RiskLimits`
      - `xvision_core::config::RiskStops`
      - `xvision_core::risk::RiskCaps`
    Existing tests under `crates/xvision-core/src/trading.rs` and the
    config test module continue to pass. Any callers that were passing
    extra fields are surfaced as parse errors with file:line evidence
    captured in the PR description.
  - One cross-field garde validator added per applicable type:
      - `TraderDecision`: if `Direction::Long`, the persisted plan
        implies `take_profit_pct > stop_loss_pct`. Concretely, add a
        `#[garde(custom(validate_long_tp_above_sl))]` (or struct-level
        `#[garde(custom(...))]`) that rejects long decisions where
        `take_profit_pct <= stop_loss_pct`. Short side mirrored.
      - `RiskStops`: `stop_loss_min_pct <= stop_loss_max_pct` (already
        implicit, now enforced).
    No other invariants are introduced in this track — the goal is
    discipline at the existing boundaries, not new policy.
  - Single pre-persist validate seam: `StrategyStore::save` (and any
    other entry point that persists a `Strategy`) calls
    `strategy.validate_typed()` exactly once before writing. The method
    runs `garde::Validate` on the typed variant and on `risk`. Today's
    scattered ad-hoc validation in `authoring.rs` /
    `crates/xvision-engine/src/api/strategy.rs` is collapsed into this
    single call.
  - `authoring::set_mechanical_param` keeps its existing surface
    (`SetMechanicalParamReq { id, key, value }`) but now:
      - Loads the strategy + active template.
      - Serializes the current variant to a `serde_json::Map`, applies
        the patch, re-deserializes through the typed enum.
      - Returns a structured error when the key is unknown for the
        active template (no more "mechanical_params is not a JSON
        object" runtime panics — the boundary catches it).
      - For `MechanicalParams::Custom`, the legacy free-form behavior
        is preserved.
  - Integration tests in `crates/xvision-engine/tests/mechanical_params.rs`:
      - `each_template_default_params_validate` — round-trips every
        canonical template's default `mechanical_params` constructor
        through serde + garde without error.
      - `unknown_field_on_canonical_template_rejected` — adding
        `{"not_a_real_param": 1}` to a `TrendFollower` strategy fails
        deserialization with `unknown field` (deny_unknown_fields).
      - `custom_template_accepts_arbitrary_json` — a strategy whose
        `manifest.template = "my-experimental"` accepts arbitrary
        `mechanical_params` JSON without rejection.
      - `long_tp_below_sl_rejected` — constructing a
        `TraderDecision { action: Long, stop_loss_pct: 5.0,
        take_profit_pct: 3.0, … }` and calling `.validate()` fails
        the new cross-field garde rule. Short-side mirrored.
      - `legacy_strategy_json_roundtrips` — parses today's on-disk
        strategy JSON shape (from `crates/xvision-engine/tests/strategy_roundtrip.rs`
        fixture) and re-serializes byte-for-byte identical for the
        canonical template fixtures.
      - `set_mechanical_param_rejects_unknown_key` — calls
        `authoring::set_mechanical_param` on a `TrendFollower`
        strategy with `key = "not_a_real_param"`; expects a typed
        error, not a save.
      - `set_mechanical_param_accepts_known_key` — updating
        `ema_fast` on a `TrendFollower` strategy succeeds and the
        updated `Strategy` round-trips through the validator.
      - `deny_unknown_fields_intern_briefing_rejects_extra` —
        `serde_json::from_str::<InternBriefing>` on a payload with
        a stray `extra_field` fails.
      - `deny_unknown_fields_risk_config_rejects_extra` — same for
        `RiskConfig`.
      - `strategy_validate_typed_invoked_on_save` — uses an
        in-memory `StrategyStore` stub to assert that
        `validate_typed()` is called on `save` (via a counter on the
        stub or by injecting an invalid trader decision in the
        manifest path that should be rejected pre-persist).
  - Existing tests pass without modification (or with **trivial**
    adjustments where the fixture relied on extra JSON fields that
    `deny_unknown_fields` now rejects — those changes are listed in
    the PR description with one-liner justification per test).
  - `cargo clippy -p xvision-engine -p xvision-core -- -D warnings`
    clean. No new `unwrap()` in production paths; the validator
    returns typed errors.
  - **No schema migration. No frontend changes. No new top-level
    dependencies.** (`garde` is already on the workspace.)
  - **No setup→cycle re-introduction.** Terminology stays current.
---

# Scope

Implements F-6 from the 2026-05-18 harness observability audit
(`team/intake/2026-05-18-harness-observability-audit.md` finding F-6).

`Strategy.mechanical_params: serde_json::Value`
(`crates/xvision-engine/src/strategies/mod.rs:59`) is an untyped escape
hatch — template-specific params skip all validation, the warmup-bar
derivation is a JSON walker keyed on regex-ish key names
(`is_period_like_key`), and the `set_mechanical_param` authoring path
panics on a non-object value with a runtime `anyhow!` instead of
catching the shape at the boundary. `InternBriefing`, `TraderDecision`,
`RiskConfig`, and `RiskCaps` all rely on field-level `garde` checks but
do **not** set `#[serde(deny_unknown_fields)]`, so spurious trailing
fields silently survive a round-trip.

F-6 replaces the JSON escape hatch with a typed enum keyed on
`manifest.template`, adds `#[serde(deny_unknown_fields)]` to the four
trading-side payload types, and consolidates the pre-persist validate
pass behind a single seam (`Strategy::validate_typed()` called from
`StrategyStore::save`). The wire format stays the same — the
`mechanical_params` JSON object on disk does not need to migrate —
which keeps F-6 a pure code change rather than a data migration.

Stacked on nothing. Parallel-safe with F-2 (PR #293), F-3 (migrations),
F-4 (span taxonomy), F-5 (recovery dispatcher), and F-7 (frontend
toggle). The F-5 contract explicitly carves out
`crates/xvision-engine/src/strategies/**` as F-6's territory.

Reference: 2026-05-18 harness audit intake, finding F-6
("`Strategy.mechanical_params: serde_json::Value` is an untyped escape
hatch — template-specific params skip all validation. `InternBriefing`
and `RiskConfig` lack `deny_unknown_fields`. Type `mechanical_params`
per template; tighten serde discipline on trading types.").

# Out of scope

- A typed `mechanical_params` UI editor in the SPA. Today the frontend
  renders `mechanical_params` as `JSON.stringify(strategy.mechanical_params)`
  (`frontend/web/src/routes/authoring.tsx:962`). That stays as-is in
  this track. A schema-driven form is a follow-up (V2 surface).
- New trading-style invariants beyond `TP > SL`. The audit calls out
  TP/SL as the example; that's the only cross-field rule this track
  introduces. Other invariants (max simultaneous longs, regime-fit
  consistency, etc.) are follow-ups if the operator surfaces them.
- A migration to a tagged JSON shape (`{"trend_follower": {...}}`).
  The wire format keeps the flat params object today's strategies use,
  with `manifest.template` driving variant choice. A future move to
  internally-tagged JSON would require a data migration and is not
  worth the churn for an internal-only representational change.
- The `xvision-eval` baselines / `Algorithm` trait. F-6 is about the
  pipeline-config artifact, not the eval-time decision producers.
- `Strategy.mechanical_params` per-run override on `EvalRunRequest`
  (`crates/xvision-engine/src/api/eval.rs:549`). The override path
  stays a `serde_json::Value` patch — it doesn't persist to a
  Strategy row, so the typed enum doesn't apply. (A follow-up could
  route the override through the same validator before merging.)
- New garde dependencies, new validation crates, or a switch away
  from garde.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-typed-mechanical-params status
git -C .worktrees/harness-typed-mechanical-params log --oneline -5 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-typed-mechanical-params
#   - base is origin/main
#   - CARGO_TARGET_DIR is exported to a shared cache, not the worktree
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-typed-mechanical-params \
  -b task/harness-typed-mechanical-params origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
```

# Notes

Append checkpoints / PR links below.
