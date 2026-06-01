# RiskVetoTaxonomist

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/`
**Script:** `scripts/risk-veto-taxonomy.sql`

## Surfaces Inspected

- `crates/xvision-core/src/trading.rs` — `VetoReason` and `RiskDecision`
- `crates/xvision-core/migrations/0001_init.sql` and `0002_rename_setup_to_cycle.sql` — `risk_outcomes`
- `crates/xvision-risk/src/**` — deterministic risk rule sources

## Findings

The right first pass is structured aggregation, not embeddings. `VetoReason` is already an enum with structured variants plus `Custom(String)`.

The SQL reports:

- top-level `approved` / `modified` / `vetoed` distribution
- reason-frequency breakdown for modified and vetoed decisions
- recent custom/free-text reason samples

## Why This Is Useful

This can reveal repeated unintended gate firing without spending tokens on clustering enum names. It also tells the operator whether `Custom(String)` is actually used enough to justify a more expensive text-taxonomy pass.

## Waste Avoided

No embeddings. No k-means. No LLM summarization unless the SQL finds real custom/free-text reasons worth summarizing.

## Files Changed

- `scripts/risk-veto-taxonomy.sql`
- `docs/research/overnight-agents/2026-06-01/RiskVetoTaxonomist.md`

## Verification

Run against the core DB:

```bash
sqlite3 "$XVN_HOME/core.db" < scripts/risk-veto-taxonomy.sql
```

Expected behavior:

- returns zero rows on an empty core DB
- returns frequency counts on populated `risk_outcomes`
- performs only read-only `SELECT` statements

## Residual Risks

- SQLite JSON extraction around `Custom(String)` should be verified against real serialized rows. If serde emits a shape different from `{"custom": "..."}`, adjust the `json_extract` paths.
- Risk outcomes live in the core DB, not the engine DB; do not run this against the wrong database.
