---
track: wire-min-notional-into-eval
lane: leaf
wave: qa-operator-2026-05-19
worktree: .worktrees/wire-min-notional-into-eval
branch: task/wire-min-notional-into-eval
base: origin/main
status: ready
depends_on: []   # #324 merged 2026-05-19 (commit b309e16)
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/Cargo.toml
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/tests/api_eval_min_notional.rs
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/migrations/**
  - crates/xvision-risk/**
  - crates/xvision-core/**
  - config/**
  - frontend/web/**
interfaces_used:
  - PaperExecutor::with_min_notional_usd(f64) (builder added by #324)
  - xvision_risk::config::RiskConfig (or equivalent — the type that
    holds the `venues: BTreeMap<String, VenueConfig>` map added by #324)
  - api/eval.rs's existing executor construction (line 1324:
    `PaperExecutor::with_bars(broker, bars)`)
verification:
  - cargo test -p xvision-engine --test api_eval_min_notional
  - cargo test -p xvision-engine
  - cargo build -p xvision-engine
acceptance:
  - `crates/xvision-engine/Cargo.toml` — add `xvision-risk` to
    `[dependencies]`. Use the same `path = "../xvision-risk"` pattern
    the workspace uses for sibling crates.
  - `crates/xvision-engine/src/api/eval.rs` — at the `PaperExecutor::with_bars(broker, bars)`
    site (~line 1324, or wherever the paper executor is constructed
    for production), chain `.with_min_notional_usd(min_notional)` where
    `min_notional` is resolved from the active risk config's `venues`
    map. The venue id for the paper executor is `"paper"` (matches
    `config/risk.toml`'s `[venues.paper] min_notional_usd = 10.0`
    landed by #324).
  - If the risk config isn't already loaded in the api/eval.rs path,
    load it via the same loader the risk layer uses. Look for
    `RiskConfig::from_path` or equivalent on `xvision-risk::config`.
    Worker decides the cleanest plumbing — if the api/eval.rs
    construction site doesn't have an obvious config handle, document
    the choice in the status note.
  - Default behavior when the venue isn't in the config: `0.0` (rule
    is a no-op — matches the contract from #324). Do NOT panic, do
    NOT error.
  - Regression test `crates/xvision-engine/tests/api_eval_min_notional.rs`:
    end-to-end test using the test risk config from `config/risk.toml`
    (which has `[venues.paper] min_notional_usd = 10.0`). Construct an
    eval run targeting paper, submit an order sized below $10 notional,
    assert the order is **vetoed at the risk layer** (decision row
    carries `[below_venue_min_notional]` per #324) and the broker is
    **never called**. This is the operator's exact 2026-05-19 ETH/USD
    ~$6 failure, but now with the production-wired veto.
  - `cargo build -p xvision-engine` clean — no new circular dep, no
    feature-flag conflicts.
  - No production code changes outside `api/eval.rs` and `Cargo.toml`.
    The builder method `with_min_notional_usd` shipped with #324; this
    track just calls it.
  - No `try/catch` silencing (`feedback_alpha_root_cause`).
parallel_safe: true
parallel_conflicts: []
---

# Scope

PR #324 (merged 2026-05-19, commit b309e16) shipped the
`risk-gate-min-notional` track: `MinNotional` risk rule,
`VetoReason::BelowVenueMinNotional`, per-venue `min_notional_usd`
config in `config/risk.toml`, and a `PaperExecutor::with_min_notional_usd(f64)`
builder. But the production wiring was deferred — `xvision-engine`
doesn't depend on `xvision-risk`, and `api/eval.rs` doesn't call the
new builder.

That defer was correct: `xvision-engine/Cargo.toml` and `api/eval.rs`
were outside #324's `allowed_paths`. This is the one-line follow-up.

End state: a paper-eval run that emits an order with notional
< $10 (the configured paper minimum) is **vetoed pre-submit at the
risk layer** instead of producing a wasteful broker round-trip + the
classified `MinOrderSize` recovery dance from #314 + #286.

Anchor reading:

- PR #324 (`b309e16`) — the contract's status note at
  `team/status/risk-gate-min-notional.md` (if present) documents
  the per-venue config schema choice.
- `crates/xvision-engine/src/api/eval.rs:1320-1340` — the paper
  executor construction site to wire.
- `crates/xvision-engine/src/eval/executor/paper.rs::with_min_notional_usd`
  — the builder added by #324 (forbidden to modify; just call it).
- `config/risk.toml` (post-#324) — confirms `[venues.paper]
  min_notional_usd = 10.0` and `[venues.live] min_notional_usd = 1.0`.

# Out of scope

- Changing the `MinNotional` rule itself — owned by #324.
- Changing `with_min_notional_usd` signature — owned by #324.
- Live broker integration — `[venues.live]` config already exists;
  live executor follows the same pattern when it lands.
- Other broker constraints (tick size, lot size). Same
  pattern, separate follow-up tracks.
- Frontend changes — the veto surfaces via the existing
  decision-row `[below_venue_min_notional]` classification (free
  reuse of the `eval-broker-error-circuit-breaker` UI from #320 + #328).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/wire-min-notional-into-eval status
git -C .worktrees/wire-min-notional-into-eval log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/wire-min-notional-into-eval \
  -b task/wire-min-notional-into-eval origin/main
```

# Notes

Append checkpoints / PR links below.
