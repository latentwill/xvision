---
track: risk-gate-min-notional
lane: integration
wave: qa-operator-2026-05-19
worktree: .worktrees/risk-gate-min-notional
branch: task/risk-gate-min-notional
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-core/src/trading.rs
  - crates/xvision-risk/src/lib.rs
  - crates/xvision-risk/src/rules/**
  - crates/xvision-risk/src/config.rs
  - crates/xvision-risk/src/whitelist.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/tests/risk_min_notional.rs
  - crates/xvision-risk/tests/**
  - config/risk.toml
  - config/whitelist.toml
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-execution/src/broker_surface.rs
  - frontend/web/**
interfaces_used:
  - RiskLayer::evaluate
  - RiskRule (trait)
  - RiskDecision::Vetoed
  - VetoReason (extend with new variant)
  - TraderDecision / PortfolioState / AssetSymbol
verification:
  - cargo test -p xvision-risk
  - cargo test -p xvision-engine --test risk_min_notional
  - cargo test -p xvision-engine
  - cargo test -p xvision-core
acceptance:
  - New `VetoReason::BelowVenueMinNotional` variant on
    `crates/xvision-core/src/trading.rs` (snake_case serde: `below_venue_min_notional`).
    Documented next to the existing variants.
  - New `MinNotional` risk rule under `crates/xvision-risk/src/rules/`
    that vetoes any `TraderDecision` whose notional (size_bps × portfolio
    equity / 10_000, OR an explicit USD size when present on the action)
    is below the venue's configured minimum. Returns
    `RuleVerdict::Veto(VetoReason::BelowVenueMinNotional)`.
  - Minimum is configurable per venue in `config/risk.toml` under a new
    `[venues.<venue_id>]` section: `min_notional_usd = 10.0` for paper,
    `min_notional_usd = 1.0` for live (verify against Alpaca docs; cite
    the source in the status note). Default if unset: `0.0` (rule is
    a no-op, matches today's behavior).
  - `RiskLayer::with_default_rules` registers `MinNotional` after the
    size-modifying rules (so any `Modify` rule that shrinks size runs
    first, then `MinNotional` catches a too-small modified decision)
    but before `StopLossPresent` / `TakeProfitRR` (no point validating
    stops on an order we're about to veto).
  - `crates/xvision-engine/src/eval/executor/paper.rs` calls the risk
    layer before `submit_order` in the same flow that today calls it,
    so a `BelowVenueMinNotional` veto **never reaches the broker**. The
    rejected-by-broker code path for `broker_min_order_size` should
    become unreachable in normal operation. (If it does fire, that's a
    venue-config-mismatch bug; the run still completes via the existing
    rejection path, which is handled by the parallel
    `eval-broker-error-circuit-breaker` track.)
  - Regression test `crates/xvision-engine/tests/risk_min_notional.rs`
    exercises the operator's exact failure: ETH/USD, ~$6 notional, paper
    venue with `min_notional_usd = 10.0`. Confirm the decision is
    vetoed pre-submit; confirm the broker mock is **never called**.
  - Unit tests under `crates/xvision-risk/tests/` cover: venue with
    unset min (rule is no-op), venue with set min (below vetoes, equal
    passes, above passes), modified-decision interaction (a `Modify`
    rule that shrinks size below the min is then vetoed by `MinNotional`).
  - No `try/catch` silencing, no fallback shim
    (`feedback_alpha_root_cause`).
  - No migration. No frontend changes. No broker-surface changes.
parallel_safe: true
parallel_conflicts:
  - "eval-broker-error-circuit-breaker: same wave, both touch crates/xvision-engine/src/eval/executor/paper.rs but in disjoint regions (this track adds a pre-submit veto seam; the circuit-breaker track adds a consecutive-error counter on the rejection-handling path). Coordinate via team/queue/ if the diffs end up overlapping; otherwise ship independently."
---

# Scope

Root-cause fix for the operator-reported broker rejection loop on
2026-05-19: paper-venue ETH/USD orders sized ~$6 (0.00274 ETH) were
submitted every decision cycle and rejected by Alpaca with
`broker_min_order_size` ("cost basis must be >= minimal amount of order
10"). The size calculator has no awareness of the broker's deterministic
minimum, so every cycle pays the broker round-trip to learn what we
already know.

This track adds a `MinNotional` risk rule and a per-venue
`min_notional_usd` config. The risk gate vetoes pre-submit, and the
broker call never fires for too-small orders. The veto reason
(`BelowVenueMinNotional`) surfaces in the run trace exactly like any
other risk veto — operator gets a clear "the venue minimum is $10, your
order was $6" signal, instead of an opaque broker rejection.

**Context as of 2026-05-19 (post-#314 merge):** PR #314 (merged
2026-05-19 00:19 +0800) added the Alpaca `"cost basis must be >=
minimal amount of order N"` phrase to the `MinOrderSize` branch of
`classify_broker_error_message`. The broker error is now classified
as `MinOrderSize` (recoverable) instead of `AuthFailed` (fatal); #286
then injects `BrokerErrorFeedback` into the next decision seed so
the trader agent re-decides with a larger size. The *critical*
failure mode the operator originally reported (run terminating
immediately) is fixed at the classifier + feedback layer.

This contract is the **proactive** layer — prevent the wasteful
broker round-trip on known-bad orders, surface the constraint as a
clear `BelowVenueMinNotional` risk veto instead of hiding it inside
the trader's invisible re-sizing, and give the risk layer awareness
of venue minimums for future deterministic constraints (tick size,
lot size). **Priority revised: P2 (post-#314), not P1.** Worth
shipping but not blocking the operator.

Anchor reading:

- `team/intake/2026-05-19-qa-operator-round-4.md` "Round-4 addendum"
  section, item 5 (Finding B).
- `crates/xvision-risk/src/lib.rs` for the rule trait + RiskLayer wiring.
- `crates/xvision-core/src/trading.rs:296` for `VetoReason`.
- `crates/xvision-engine/src/eval/executor/paper.rs:495-520` for the
  current submit seam (no pre-submit risk check today; the risk layer
  is wired elsewhere — confirm where it's called for the eval path and
  ensure `MinNotional` runs there).

# Out of scope

- Live broker integration (live Alpaca uses a different minimum but
  the rule is venue-config-driven, so live picks up the right value
  from `config/risk.toml`'s `[venues.live]` block without code change).
- A "round up to min" mode. Tempting (paper-only QoL) but adds a
  second policy axis; defer to a follow-up if operator asks for it.
- Other broker constraints (tick size, lot size, max notional). The
  `MinNotional` rule pattern is reusable for them; this track ships
  the first concrete instance and the per-venue config plumbing.
- Changing the executor's rejection-retry behavior — the parallel
  `eval-broker-error-circuit-breaker` track owns that.
- Touching `crates/xvision-execution/src/alpaca.rs` or
  `broker_surface.rs`. The broker doesn't change; we just stop
  calling it for known-bad orders.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/risk-gate-min-notional status
git -C .worktrees/risk-gate-min-notional log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/risk-gate-min-notional \
  -b task/risk-gate-min-notional origin/main
```

# Notes

Append checkpoints / PR links below. The per-venue config schema choice
(`[venues.<id>]` section vs. inline on each whitelist entry) is
acceptance-bearing — document the decision in the status note before
opening the PR.
