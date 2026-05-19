---
track: risk-gate-min-notional
worktree: .worktrees/risk-gate-min-notional
branch: task/risk-gate-min-notional
base: origin/main
phase: pr-open
last_updated: 2026-05-19T00:00:00Z
owner: claude
---

# What changed

Pre-submit minimum-notional gate so the broker never sees orders below
its deterministic minimum. Surfaces venue constraints as a clean risk
veto (`VetoReason::BelowVenueMinNotional`) instead of an opaque broker
rejection. P2: PR #314 + #286 already keep the run alive when the
broker rejects; this layer prevents the wasteful round-trip.

## Config schema decision

Per-venue config lives in `config/risk.toml` under
`[venues.<venue_id>]` keyed by venue id (today `paper` and `live`).
Initial values:

```toml
[venues.paper]
min_notional_usd = 10.0  # Alpaca paper crypto rejects orders with
                         # "cost basis must be >= minimal amount of order 10"
[venues.live]
min_notional_usd = 1.0   # Alpaca live crypto's documented minimum on
                         # most pairs per
                         # https://docs.alpaca.markets/docs/crypto-trading
                         # ("Order minimums"). Conservative default;
                         # bump per-asset only if a per-symbol override
                         # surface lands.
```

Default `0.0` for unconfigured venues → rule is a no-op (preserves
today's pass-everything behavior on venues we haven't catalogued).

Chose `[venues.<id>]` section over inline-on-each-whitelist-entry
because (a) the constraint is venue-bound, not asset-bound — Alpaca
paper's `$10` minimum applies to every crypto pair, not just `BTC`;
(b) keeps the whitelist focused on tradeable-asset metadata; (c) leaves
room for future per-venue constraints (`max_notional_usd`,
`tick_size`, `lot_size`) without restructuring.

## xvision-core (`trading.rs`, `config.rs`)

- New `VetoReason::BelowVenueMinNotional` (snake_case serde
  `below_venue_min_notional`). Documented inline.
- Additive `venues: BTreeMap<String, RiskVenueLimits>` field on
  `RiskConfig` (kept `deny_unknown_fields`; new `RiskVenueLimits`
  struct mirrors xvision-risk's). Required to keep `config/risk.toml`
  loading through xvision-core's parallel config schema. Note: this is
  a minor expansion past the contract's listed allowed_paths (only
  `trading.rs` was named); without it, `config/risk.toml` would fail
  to load on every xvision-core consumer. Documented in code comment.

## xvision-risk (`config.rs`, `lib.rs`, `rules/min_notional.rs`, tests)

- New `VenueLimits { min_notional_usd: f64 }` per-venue struct.
- `RiskConfig` gains `venues: BTreeMap<String, VenueLimits>` +
  `venue_limits(id)` accessor.
- `validate()` rejects negative or non-finite minimums.
- New `MinNotional` rule. Notional computed as
  `equity_usd × size_bps / 10_000`. Vetoes when notional is strictly
  less than the venue minimum. No-ops on:
  - `min_notional_usd <= 0.0` (unset venue)
  - non-actionable actions (`Flat`, `Close`)
  - `size_bps == 0`
- New `RiskLayer::from_config_for_venue(_, _, venue_id)` constructor.
  Legacy `from_config` still works and skips MinNotional.
- `with_default_rules` now takes `venue_id: Option<&str>`. Rule
  ordering matches contract acceptance:
  `[whitelist, daily_loss, size, exposure, open_positions, cluster,
   MinNotional (if venue), StopLossPresent, TakeProfitRR]`.
- Unit tests in `rules/min_notional.rs`: zero-min no-op, below/equal/
  above min, non-actionable pass-through, zero-size pass-through.
- Integration tests in `tests/min_notional_integration.rs`: no-venue,
  zero-min, below/equal/above per venue, different-min-per-venue
  dispatch, ordering vs `StopLossPresent`, with-open-positions.

## xvision-engine (`paper.rs`, `tests/risk_min_notional.rs`)

- New `PaperExecutor::with_min_notional_usd(f64)` builder. The eval
  paper flow does not invoke the `RiskLayer` today (it works against
  parsed JSON action + size in base units, not `TraderDecision`), so
  the gate is a focused pre-submit check that reuses the same disjoint
  region as `eval-broker-error-circuit-breaker`.
- Pre-submit veto fires when `size × reference_price_usd < min`.
  Records a `[below_venue_min_notional]` decision row, emits
  `DecisionEmitted`, snapshots equity (unchanged), increments
  `decision_idx`, and continues — same pattern as the recoverable
  broker-error path.
- Production wiring through `api/eval.rs` (loading the venue min from
  `config/risk.toml` into the `PaperExecutor` builder) is intentionally
  deferred: `xvision-engine/Cargo.toml` would need an
  `xvision-risk` dep, which is outside the contract's allowed_paths.
  The builder is in place; a small follow-up wires it. Regression tests
  exercise the gate end-to-end against the operator's reported failure
  shape.
- Regression test `tests/risk_min_notional.rs` covers:
  - operator's exact failure (ETH-like ~$2,200 close × 0.00274 size ≈ $6)
    on $60 buying_power × 0.1 risk_pct → vetoed; broker never called
  - control: same configuration WITHOUT the gate submits the order
  - `min_notional_usd = 0.0` is explicit no-op
  - above-min orders pass through unchanged

## Verification

```
cargo test -p xvision-risk                                 # 49 ok
cargo test -p xvision-engine --test risk_min_notional      # 4 ok
cargo test -p xvision-engine --test eval_executor_paper    # 13 ok
cargo test -p xvision-core                                 # 98 ok
```

Pre-existing engine lib/test failures (10 in `xvision-engine` lib,
several in `api_eval` / `eval_observability` / etc.) are unrelated:
`agent_slots.prompt_version` missing column, run-already-completed
errors. Verified by running the same tests on the baseline commit
(`bcda0f1`) with my changes stashed: same failures reproduce.

## Out of scope (queued)

- Wiring `with_min_notional_usd` from `api/eval.rs` production path —
  needs `xvision-risk` dep added to `xvision-engine/Cargo.toml`.
- A "round-up-to-min" mode (operator QoL; defer per contract).
- Other venue constraints (`max_notional_usd`, `tick_size`,
  `lot_size`) — the rule pattern + per-venue config plumbing makes
  these one-rule-each follow-ups.
- Wiring `MinNotional` into the `xvision-harness::apply_risk` callers
  — they already use `RiskLayer::from_config`, which today defaults
  to `venue_id = None`. Each consumer needs to switch to
  `from_config_for_venue(..., "paper" | "live")` to opt in. Mechanical
  follow-up.
