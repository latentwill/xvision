---
track: eval-candle-integrity-and-manifest
lane: foundation
wave: v2e
worktree: .worktrees/eval-candle-integrity-and-manifest
branch: task/eval-candle-integrity-and-manifest
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-data/src/fixtures.rs                     # validator hook at load_ohlcv_fixture; bars_content_hash mint
  - crates/xvision-data/src/validate.rs                     # NEW — validate_ohlcv impl + DataDefect enum
  - crates/xvision-data/src/manifest.rs                     # NEW — DataManifest type + canonicalization
  - crates/xvision-data/Cargo.toml                          # if sha2 / parquet hash deps need adding (likely already present)
  - crates/xvision-engine/src/eval/scenario.rs              # data_source + manifest fields on Scenario — disjoint region with cost-model
  - crates/xvision-engine/src/eval/compare.rs               # manifest mismatch refusal on ComparisonReport build
  - crates/xvision-engine/src/eval/findings.rs              # data_defect finding kind registration — disjoint region with trace-foundation (foundation adds the schema columns; this track adds one new kind variant)
  - crates/xvision-engine/migrations/024_run_bars_manifest.sql      # NEW
  - crates/xvision-engine/migrations/024_run_bars_manifest.down.sql # NEW
  - team/MANIFEST.md                                         # migration 024 registration only
  - crates/xvision-data/tests/**
  - crates/xvision-engine/tests/data_integrity_*.rs          # NEW
  - frontend/web/src/api/types.gen/**                        # ts-rs regenerated
forbidden_paths:
  - frontend/web/src/**                                      # no UI work this track
  - crates/xvision-engine/src/eval/executor/**               # not this track's concern
interfaces_used:
  - xvision-data::fixtures::Ohlcv
  - xvision-data::asset_whitelist
  - xvision-engine::eval::findings::Finding
parallel_safe: true
parallel_conflicts:
  - eval-cost-model-per-bar-and-volume-share (scenario.rs — disjoint regions; this track adds manifest+data_source fields, cost-model adds VenueOverride and per-bar arrays)
  - eval-trace-surface-foundation (findings.rs — foundation adds schema columns; this track adds the data_defect kind variant)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-data -- -D warnings
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-data validate
  - cargo test -p xvision-engine data_integrity_
  - pnpm --dir frontend/web typecheck
acceptance:
  - **`validate_ohlcv(bars: &[Ohlcv], cadence: Duration) -> Vec<DataDefect>`** runs at every fixture load and at scenario start. Returns a structured vec, never panics.
  - **`DataDefect` enum** with variants: `NonMonotonicTimestamp { at, prev_ts, this_ts }`, `DuplicateTimestamp { at }`, `MissingBar { expected_ts, gap_bars }`, `OhlcViolation { at, kind: OhlcViolationKind }` (kinds: LowAboveOpen, LowAboveClose, HighBelowOpen, HighBelowClose, HighBelowLow), `NegativeOrNanField { at, field }`, `ZeroVolumeBar { at }` (warn-only), `WickShockOutlier { at, sigma }` where `sigma = (high-low) / rolling_median(range, 200)`.
  - **Cadence-aware MissingBar detection.** For equity scenarios, respects market hours via the `calendar` field on Scenario (`NYSE` skips weekends/holidays; `Continuous24x7` does not). For crypto, every bar at the cadence is expected.
  - **Per-defect severity tier.** `NonMonotonicTimestamp`, `DuplicateTimestamp`, `OhlcViolation`, `NegativeOrNanField` → severity `Error`. `MissingBar`, `WickShockOutlier` → severity `Warning`. `ZeroVolumeBar` → severity `Info`. A scenario with any `Error`-tier defect requires `--allow-defective-data` to run.
  - **Defects emit as `data_defect` findings** in `findings.jsonl` with `evidence_cycle_ids` empty (data defects pre-exist the cycle) and `produced_by_check = "validator:ohlcv"`.
  - **`bars_content_hash`** computed at fixture-load as sha256 of the Parquet bytes. Persisted on the `Run` record (new column via migration 024).
  - **Data manifest** persisted alongside the bars hash: `DataManifest { feed: FeedKind, adjustment: AdjustmentKind, timeframe: BarGranularity, session_filter: SessionFilter, calendar: CalendarRef, timezone: String }`. `manifest_canonical = sha256(JSON_canonical(manifest))`. Persisted on the `Run` record.
  - **Compare refusal.** `ComparisonReport::build` refuses to render two runs together when their `manifest_canonical` differ, unless an explicit `allow_manifest_mismatch: bool` override is passed. Refusal surfaces a `ManifestMismatch { run_a, run_b, diff_fields }` error from the API.
  - **Determinism receipt integration.** Update the receipt minter from `eval-trace-surface-foundation` to hash `(bars_content_hash || manifest_canonical || strategy_hash || scenario_id || seed || engine_version)` so a feed change without a bar change still produces distinct receipts. (Open question 3 from the intake — accept the recommendation.)
  - **Migration 024** adds `runs.bars_content_hash`, `runs.manifest_canonical`, `runs.feed`, `runs.adjustment`, `runs.session_filter`, `runs.calendar`, `runs.timezone` (or one consolidated `runs.bars_manifest TEXT` JSON column — implementation choice; tests assert round-trip). Down rolls back.
  - **ts-rs exports.** `DataDefect`, `OhlcViolationKind`, `DataManifest`, `FeedKind`, `AdjustmentKind`, `SessionFilter` regenerated under `frontend/web/src/api/types.gen/`.
  - **Tests:** one positive case per defect kind; manifest mismatch refusal returns `ManifestMismatch`; `bars_content_hash` is stable across re-pulls of the same Parquet (assert byte-identical hash); `--allow-defective-data` flag bypasses the refusal on `Error`-tier defects.

---

# Scope

V2E foundation pair: candle integrity validator (research doc §3.1) +
pinned canonical fixtures + content-hash receipts + data manifest
(research doc §3.2). Merged because both touch fixture-load and emit
the same `data_defect`-family findings; the intake recommends the
merge.

Surfaces silent data corruption as first-class findings, makes
re-pulled Alpaca bars produce visible hash drift instead of a
reproducibility leak, and prevents two runs with the same bars but
different `feed: iex` vs `feed: sip` from being compared on the same
chart without an explicit override.

# Out of scope

- §3.3 multi-source bar cross-check (Polygon/Tiingo/Yahoo). Useful, not
  v1; defer until paper-parity drift findings post-V2C suggest source-
  side bug hunts are worth the cost.
- §3.4 corporate-action ledger (`splits.parquet`, `dividends.parquet`).
  Equities-readiness follow-up.
- §3.6 point-in-time universe (`delisted.parquet`). Equities-readiness
  follow-up.
- Bar-vs-tick fidelity guard (§3.7). Scenario-level policy, not a code
  change in this track.
- Replay parity test against Alpaca paper (§3.8). Deferred to paper-
  parity intake gating V2C marketplace.

# Migration coordination

Migration **024** claimed here. `eval-trace-surface-foundation` claims
**023**. First to merge updates `team/MANIFEST.md`; second rebases the
migration registry hunk.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-candle-integrity-and-manifest status
git -C .worktrees/eval-candle-integrity-and-manifest log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-candle-integrity-and-manifest -b task/eval-candle-integrity-and-manifest origin/main
```

# Notes

The `WickShockOutlier` threshold (`sigma > 8`) is a starting point.
Tune against real Alpaca history once the validator runs over a few
hundred days; if it fires too often, raise the threshold. Don't make it
configurable in the first pass — single tunable per release is fine.

If `runs.feed` / `runs.adjustment` etc. already exist as columns,
migration 024 idempotently checks via `IF NOT EXISTS` patterns and
skips. The consolidated JSON column approach is simpler if there are
no current consumers reading discrete columns — verify before
committing to either path.
