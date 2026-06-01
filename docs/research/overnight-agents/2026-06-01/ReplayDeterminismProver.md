# ReplayDeterminismProver

**Date:** 2026-06-01
**Source:** 100x overnight suite — docs/tmp/2026-06-01-100x-overnight-research-agent-suite.md

## Surfaces Inspected

- `crates/xvision-engine/migrations/018_agent_run_observability.sql` — `checkpoints` table schema
- `crates/xvision-engine/migrations/002_eval.sql` — `determinism_receipts` table
- `crates/xvision-engine/src/eval/attestation.rs` — attestation / receipt logic
- `crates/xvision-engine/src/baselines/trader_arm.rs` — BriefingReplay::replay path

## What Can Be Proved Now

**Determinism receipts** (`determinism_receipts` table, keyed on `run_id`):

Each `receipt_hash` is a SHA-256 over five canonical inputs:
```
(strategy_bundle_hash, scenario_id, bars_content_hash, seed, engine_version)
```

For any two completed eval_runs in the same cohort (same five inputs), provable determinism means:
```sql
SELECT er1.id AS run_a, er2.id AS run_b,
       dr1.receipt_hash = dr2.receipt_hash AS receipts_match
FROM eval_runs er1
JOIN eval_runs er2 ON er1.agent_id = er2.agent_id
    AND er1.scenario_id = er2.scenario_id
    AND er1.bars_content_hash = er2.bars_content_hash
JOIN determinism_receipts dr1 ON dr1.run_id = er1.id
JOIN determinism_receipts dr2 ON dr2.run_id = er2.id
WHERE er1.id < er2.id   -- avoid self-joins
  AND dr1.receipt_hash != dr2.receipt_hash;
-- Zero rows = determinism holds across all comparable completed runs.
```

A non-empty result set indicates non-determinism in the engine (bug) or a cohort definition mismatch (different seed or engine version). Both cases should be investigated.

## What Blocks Full Agent-Run Replay

| Missing capability | Table | Column | Required for |
|---|---|---|---|
| `output_hash` is mostly NULL | `checkpoints` | `output_hash` | Hash-level checkpoint comparison replay |
| Retention mode `full_debug` rare/absent | `agent_runs` | `retention_mode` | Payload-level replay (actual LLM outputs stored) |
| Sidecar wiring not complete | `agent_runs` | `sidecar_version` | End-to-end live record → sidecar emission |
| `manifest_canonical` is nullable | `eval_runs` | `manifest_canonical` | Candle-integrity binding for cross-run comparability |

The replay scaffold exists in `crates/xvision-eval/src/baselines/trader_arm.rs` (`BriefingReplay::replay`) but is tested via store seeding, not end-to-end live record→replay. The Cline runtime unification branch (`feat/cline-runtime-unification`) implemented stage 4 (replay + unified eval) but the live record→sidecar wiring was left as a follow-up (see handoff note: `docs/superpowers/notes/2026-05-25-cline-runtime-unification-status.md`).

## Gap Report

**Hash-level replay is partially provable.** The SQL query above can verify receipt determinism today for any run that has a `determinism_receipts` row. This is the highest-confidence, lowest-risk form of replay verification.

**Payload-level replay is blocked** by `output_hash IS NULL` on most checkpoint rows. To unlock it:

1. Submit at least one eval run with `--retention-mode full_debug` (or the equivalent API flag).
2. Confirm that `checkpoints.output_hash` is populated for those rows.
3. Re-run the replay prover against those specific run IDs.

**Agent-run replay (full Cline trace → compare)** requires completing the sidecar wiring deferred from the Cline runtime unification work.

## Files Changed

None. Gap report only. No code changes, no live model calls.

## Verification

The SQL query in the "What Can Be Proved Now" section can be run directly:

```bash
sqlite3 "$XVN_HOME/engine.db" << 'SQL'
SELECT COUNT(*) AS mismatched_receipts
FROM eval_runs er1
JOIN eval_runs er2
    ON er1.agent_id = er2.agent_id
    AND er1.scenario_id = er2.scenario_id
    AND er1.bars_content_hash = er2.bars_content_hash
JOIN determinism_receipts dr1 ON dr1.run_id = er1.id
JOIN determinism_receipts dr2 ON dr2.run_id = er2.id
WHERE er1.id < er2.id
  AND dr1.receipt_hash != dr2.receipt_hash;
SQL
```

Zero rows = engine is deterministic for all receipt-covered runs. Positive count = non-determinism bug.

## Residual Risks

- `bars_content_hash` may be absent on pre-migration rows; ensure the cohort join excludes NULL hashes to avoid false-positive mismatches.
- The seed input to the receipt hash — confirm it is the eval_run seed (if any), not the system random seed, before trusting the cohort grouping.
- Live record→sidecar wiring gap means checkpoint rows written by real agent runs may differ from those written by the store-seeded replay tests. Verify on a real run before publishing replay-provable claims.
