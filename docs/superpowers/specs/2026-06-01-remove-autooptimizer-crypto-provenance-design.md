# Remove AutoOptimizer Cryptographic-Provenance Layer — Design Spec

**Date:** 2026-06-01  
**Status:** Approved (Approach A)  
**Scope:** xvision-engine, xvision-dashboard, xvision-cli, frontend/web, scripts

---

## 1. Motivation

xvision is an eval-only system. A strategy earns standing through its eval results,
not through a cryptographic certificate of authorship. The tamper-evidence and
proof machinery (Ed25519 key ceremony, Merkle root computation, CycleSeal signing,
cycle_seals / session_commitments tables) exists to answer a question the product
does not need to ask. Removing it:

- Eliminates a key-management surface that operators must never touch (rotate,
  backup, lose).
- Removes the `CycleSeal` from `CycleResult`, decoupling the optimization loop
  from a ceremony it never needed.
- Keeps BLAKE3 as a pure dedup / content-addressed identifier — correct
  functional role, zero ceremony overhead.
- Brings `run_evening_cycle` down to a plain async function without
  `operator_key: &SigningKey` / `session_id: &str` parameters.

Out of scope: marketplace on-chain anchoring (separate spec). The optimization
loop itself (mutate / judge / gate) is not touched.

---

## 2. What Is Removed

### 2.1 Engine — crates/xvision-engine/src/autooptimizer/

| File | Action | Reason |
|------|--------|--------|
| `seal.rs` | **Delete** | Entire file: `CycleSeal`, `build_and_sign`, `verify`, `persist`, `load`, `OPERATOR_DISPLAY_LABEL`. Ed25519 signing root. |
| `session.rs` | **Delete** | Entire file: `SessionCommitment`, `load_or_generate_key`, `default_key_path`. Key-ceremony root. |

**`cycle.rs` — sealing tail stripped**

Remove from `run_evening_cycle`:
- Parameters `operator_key: &SigningKey` and `session_id: &str`
- Import `use crate::autooptimizer::seal::{build_and_sign, CycleSeal}`
- Import `use ed25519_dalek::SigningKey`
- Lines that compute `merkle_root`, call `build_and_sign`, call `seal.persist`, and
  emit `CycleProgressEvent::CycleSealed`

Remove from `CycleResult`:
- Field `pub seal: CycleSeal`

Remove from `MutationOutcome` (private struct):
- Fields `diff_hash: ContentHash`, `day_hash: ContentHash`, `untouched_hash: ContentHash`
- The corresponding DB INSERT columns (`diff_hash`, `metrics_day_hash`,
  `metrics_untouched_hash`) in the lineage_nodes write path

**`lineage.rs` — Merkle helpers removed**

Remove:
- `pub async fn merkle_root_for_cycle(&self, cycle_id: &str) -> Result<ContentHash>`
- `fn compute_merkle_root(nodes: &[LineageNode]) -> Result<ContentHash>`

Keep: `LineageStore`, `LineageNode`, `LineageStatus`, all CRUD ops, parent-tree
queries, `diversity_score`. `bundle_hash` remains the primary key — no PK migration.

**`content_hash.rs` — unchanged**

`ContentHash`, `hash_bytes`, `hash_canonical_json`, `canonicalize_json` are kept.
BLAKE3 stays as a pure dedup / content-address hasher.

**`progress.rs` — CycleSealed removed from both enums**

Remove from `CycleProgressEvent`:
```rust
CycleSealed { cycle_id: String, merkle_root: String, node_count: usize }
```

Remove from `AutoOptimizerEvent` (legacy):
```rust
CycleSealed { cycle_id: String, seal_blob_hash: String, merkle_root: String }
```

**`mod.rs` — module declarations dropped**

Remove `pub mod seal;`, `pub mod session;`, and their `pub use` re-exports
(`CycleSeal`, `OPERATOR_DISPLAY_LABEL`, `SessionCommitment`).

### 2.2 Database — crates/xvision-engine/migrations/

New migration **`051_drop_autooptimizer_provenance.sql`**:

```sql
-- Drop provenance tables
DROP TABLE IF EXISTS cycle_seals;
DROP TABLE IF EXISTS session_commitments;

-- Drop hash columns from lineage_nodes.
-- SQLite < 3.35 lacks ALTER TABLE … DROP COLUMN; use table-rebuild fallback.
CREATE TABLE lineage_nodes_new (
    bundle_hash            TEXT PRIMARY KEY,
    parent_hash            TEXT REFERENCES lineage_nodes_new(bundle_hash),
    gate_verdict           TEXT NOT NULL,
    status                 TEXT NOT NULL,
    cycle_id               TEXT,
    created_at             TEXT NOT NULL,
    diversity_score        REAL
);
INSERT INTO lineage_nodes_new
    SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, diversity_score
    FROM lineage_nodes;
DROP TABLE lineage_nodes;
ALTER TABLE lineage_nodes_new RENAME TO lineage_nodes;
```

Companion **`051_drop_autooptimizer_provenance.down.sql`** recreates both tables
and adds back the three columns (data is not restorable; acceptable for
development rollback only).

Columns dropped: `diff_hash`, `metrics_day_hash`, `metrics_untouched_hash`.  
Columns kept: `bundle_hash` (PK), `parent_hash`, `gate_verdict`, `status`,
`cycle_id`, `created_at`, `diversity_score`.

### 2.3 CLI — crates/xvision-cli/src/commands/autooptimizer.rs

Remove subcommands:
- `seal` — cycle seal inspection
- `session-init` — write signed pre-commitment

Remove argument:
- `--key-path <PATH>` (operator.ed25519) from `evening-cycle` and `mutate-once`

Remove error path:
- `merkle root: {e}` error message (dead once seal removed)

Keep: `gate`, `mutate`, `inspect`, `run`, `ls`, `activate`, `retire`, `lineage`,
`mutate-once`, `evening-cycle`, `demo`.

### 2.4 Dashboard — crates/xvision-dashboard/

**`src/routes/autooptimizer.rs`**:
- Remove `GET /api/autooptimizer/seals` (list) and `GET /api/autooptimizer/seals/:cycle_id` (get)
- Remove `CycleSealRow` struct and its query
- Adjust `findings/:bundle_hash` SELECTs: `bundle_hash` and `parent_hash` stay; do
  not read the dropped hash columns

**`src/sse/autooptimizer_labels.rs`**:
- Remove match arm `CycleSealed { .. } => "Evening summary signed"` from
  `display_label`
- Remove match arm `CycleSealed { .. } => "cycle_sealed"` from `event_kind`
- Remove the `cycle_sealed` test helper and its assertions

### 2.5 Frontend — frontend/web/src/features/autooptimizer/

**`api.ts`**:
- Remove `CycleSeal` type
- Remove `listSeals()` function and `/seals` endpoint call
- Remove `getSeal(cycleId)` function and `/seals/:cycleId` endpoint call
- Delete the seal UI view component (the view that renders CycleSeal data)

### 2.6 Tests — crates/xvision-engine/tests/

| File | Action |
|------|--------|
| `autooptimizer_seal.rs` | **Delete** — tests for `build_and_sign`, verify round-trip, tamper detection |
| `autooptimizer_session.rs` | **Delete** — tests for key generation, atomic writes, SessionCommitment signing |
| `autooptimizer_cycle.rs` | **Update** — assert that `CycleResult` has no `seal` field; remove `operator_key` / `session_id` from `run_evening_cycle` call |
| `autooptimizer_cli_session_init.rs` | **Delete or update** — covers removed subcommand |
| `autooptimizer_cli_inspect.rs` | **Verify** — remove any assertion on seal output |

### 2.7 Dependencies — crates/xvision-engine/Cargo.toml

Remove if nothing outside seal.rs / session.rs uses them:
- `ed25519-dalek`
- `rand_core` (if only imported for ed25519 key generation)

Keep: `blake3`, `hex`, `sha2`, `dirs` (verify each has remaining users before
removing).

### 2.8 Sentinel & Hook

- **Delete** `scripts/terminology-lock-sentinel.sh` — the forbidden-term scan it
  performs is moot once the crypto layer and its operator-surface names are gone.
- **Remove** its invocation from `.githooks/pre-commit` (the worktree-isolation
  guard in that hook is separate and must be preserved).

---

## 3. What Is Kept

| Item | Why kept |
|------|----------|
| `content_hash.rs` / `ContentHash` / `hash_bytes` / `hash_canonical_json` | BLAKE3 as dedup ID — correct role, no ceremony |
| `lineage_nodes` table (minus 3 hash columns) | Lineage graph, parent-tree ops, diversity scoring |
| `lineage_embeddings` table | Diversity pipeline |
| Optimization loop: mutate / judge / gate / canary | Core product value |
| All operator-surface rename pairs from terminology lock | Good UX; unrelated to provenance |
| Worktree-isolation guard in `.githooks/pre-commit` | Still needed |

---

## 4. CLAUDE.md & Terminology Lock Amendments

**CLAUDE.md — remove one rule only:**  
The sentence "Cryptographic primitives (BLAKE3, Ed25519, 'merkle,' canonical JSON)
must never appear on an operator surface" is derived from the now-removed provenance
layer. Remove it. Keep all other operator-surface rename pairs (Mutation→Experiment,
etc.) — those are good UX conventions unrelated to crypto.

**docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md:**  
Prepend a notice to Section 2 (the cryptographic-provenance subsection) marking it
obsolete: "**[REMOVED 2026-06-01]** The provenance layer described in this section
has been deleted (see 2026-06-01-remove-autooptimizer-crypto-provenance-design.md).
The operator-surface rename pairs in Sections 1 and 3 remain in force."

---

## 5. Operator Impact

No operator-visible change:
- The "Evening summary signed" SSE event disappears from the activity feed (it was
  a post-cycle banner; the cycle summary page remains).
- The `/api/autooptimizer/seals` endpoint returns 404 after deployment; no existing
  operator dashboard feature calls it (seals were never surfaced in a nav page).
- The `xvn optimizer seal` and `xvn optimizer session-init` CLI subcommands are
  removed; they were not in MANUAL.md and had no operator-facing documentation.
- No key file (`~/.xvn/keys/operator.ed25519`) is created or required.

---

## 6. Future Work (Out of Scope Here)

- Marketplace on-chain anchor security — separate spec.
- Any cryptographic guarantee for on-chain strategy provenance will be re-introduced
  at the marketplace layer, not the optimizer layer.
