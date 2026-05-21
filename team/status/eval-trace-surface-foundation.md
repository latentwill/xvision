# eval-trace-surface-foundation — worker status

**Updated:** 2026-05-21  
**Worker:** Claude (eval-trace-surface-foundation session)  
**Status:** complete — ready for PR review

## What was done

All acceptance items from the contract implemented:

1. **`trace_types.rs`** — new module with `FillBranch`, `FeeSource`, `AggressorSide`, `ToolCall`, `DecisionTrace`, `FillTrace` types. `DECISIONS_SCHEMA_VERSION = "2"`. All 6 types exported to TypeScript via ts-rs.

2. **`findings/mod.rs`** — `FINDING_SCHEMA_VERSION = "2"`. Added `evidence_cycle_ids: Option<Vec<String>>` and `produced_by_check: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` and `#[ts(optional)]`. All `Option<T>` review-linked fields (migration 017) also given `#[ts(optional)]` so Finding.ts uses `?` keys throughout. Regenerated `Finding.ts`.

3. **`cycle_features.rs`** — `CycleFeatureRow`, `CycleFeaturesWriter` with Parquet sidecar emit via arrow-array/parquet crates.

4. **`determinism.rs`** — `ReceiptInputs`, `DeterminismReceipt`, `mint()`, `persist_receipt()`, `read_receipt()` using SHA-256 (sha2 crate).

5. **Migration `026_trace_surface_foundation.sql`** — `determinism_receipts` table + `eval_findings` ALTER columns (used 026, not 023, due to on-disk collision).

6. **Migration `crates/xvision-core/migrations/0003_cycles_trace_indices.sql`** — model_id, prompt_template_hash, regime_tag columns + 4 indices on cycles table.

7. **`tests/trace_surface_schema.rs`** — 10 acceptance tests, all passing.

8. **`team/MANIFEST.md`** — updated migration registry: 023 marked as gap, 026 registered for this track, next available = 027.

## Key design decisions

- `evidence_cycle_ids` and `produced_by_check` changed from `Vec<String>`/`String` to `Option<Vec<String>>`/`Option<String>` so `#[ts(optional)]` generates `?` keys in TypeScript and test fixtures in forbidden paths don't need updating.
- Store reads: DB default `'[]'` → `None`, DB default `'legacy'` → `None`. Store writes: `None` → `'[]'`/`'legacy'` for backwards compatibility.
- ts-rs exported only the 6 new types + regenerated Finding.ts. Pre-existing `.gen/` files that were inadvertently regenerated (e.g. Scenario.ts) were restored to their committed versions.

## Verification

- `cargo clippy -p xvision-engine -- -D warnings` — clean
- `cargo test -p xvision-engine --features ts-export` — 1112 passed, 0 failed
- `cargo fmt --all` — clean
- `pnpm --dir frontend/web typecheck` — passes (0 new errors introduced by this track)
