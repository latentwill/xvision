# Remove AutoOptimizer Cryptographic-Provenance Layer — Implementation Plan

**Date:** 2026-06-01  
**Design:** `docs/superpowers/specs/2026-06-01-remove-autooptimizer-crypto-provenance-design.md`  
**Branch:** cut from `origin/main`; work in an isolated worktree

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
```

---

## Step 1 — DB migration SQL

**Files touched:**
- `crates/xvision-engine/migrations/051_drop_autooptimizer_provenance.sql` (create)
- `crates/xvision-engine/migrations/051_drop_autooptimizer_provenance.down.sql` (create)

**What to write — up migration:**

```sql
-- Drop provenance tables
DROP TABLE IF EXISTS cycle_seals;
DROP TABLE IF EXISTS session_commitments;

-- Rebuild lineage_nodes without the three hash columns.
-- SQLite < 3.35 has no DROP COLUMN; table-rebuild is the portable approach.
CREATE TABLE lineage_nodes_new (
    bundle_hash       TEXT PRIMARY KEY,
    parent_hash       TEXT REFERENCES lineage_nodes_new(bundle_hash),
    gate_verdict      TEXT NOT NULL,
    status            TEXT NOT NULL,
    cycle_id          TEXT,
    created_at        TEXT NOT NULL,
    diversity_score   REAL
);
INSERT INTO lineage_nodes_new
    SELECT bundle_hash, parent_hash, gate_verdict, status,
           cycle_id, created_at, diversity_score
    FROM lineage_nodes;
DROP TABLE lineage_nodes;
ALTER TABLE lineage_nodes_new RENAME TO lineage_nodes;
```

**What to write — down migration (dev rollback; data not restored):**

```sql
ALTER TABLE lineage_nodes ADD COLUMN diff_hash TEXT;
ALTER TABLE lineage_nodes ADD COLUMN metrics_day_hash TEXT;
ALTER TABLE lineage_nodes ADD COLUMN metrics_untouched_hash TEXT;

CREATE TABLE IF NOT EXISTS cycle_seals (
    seal_id            TEXT PRIMARY KEY,
    cycle_id           TEXT NOT NULL,
    merkle_root        TEXT NOT NULL,
    operator_signature TEXT NOT NULL,
    sealed_at          TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS session_commitments (
    session_id                   TEXT PRIMARY KEY,
    config_hash                  TEXT NOT NULL,
    parent_strategy_hashes_json  TEXT NOT NULL,
    signature                    TEXT NOT NULL,
    created_at                   TEXT NOT NULL
);
```

**Verification:** SQL only — no Rust compilation needed. Confirm file names match
sqlx migration convention (`NNN_name.sql` / `NNN_name.down.sql`).

---

## Step 2 — Strip CycleSealed from progress events + SSE labels

**Files touched:**
- `crates/xvision-engine/src/autooptimizer/progress.rs`
- `crates/xvision-dashboard/src/sse/autooptimizer_labels.rs`

**progress.rs — remove `CycleSealed` from both enums:**

From `AutoOptimizerEvent` remove:
```rust
CycleSealed {
    cycle_id: String,
    seal_blob_hash: String,
    merkle_root: String,
},
```

From `CycleProgressEvent` remove:
```rust
/// Fired once the evening summary is signed. Operator label: "Evening summary signed".
CycleSealed {
    cycle_id: String,
    merkle_root: String,
    node_count: usize,
},
```

**autooptimizer_labels.rs — remove CycleSealed match arms and tests:**

In `display_label`: remove `CycleSealed { .. } => "Evening summary signed"`.  
In `event_kind`: remove `CycleSealed { .. } => "cycle_sealed"`.  
Delete the `cycle_sealed()` helper function in the test module and its two
`assert_eq!` calls.

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 3 — Strip sealing tail from cycle.rs

**Files touched:**
- `crates/xvision-engine/src/autooptimizer/cycle.rs`
- Any CLI / test callers of `run_evening_cycle` (update call sites to drop the
  two removed parameters)

**cycle.rs changes:**

1. Remove imports:
   ```rust
   use ed25519_dalek::SigningKey;
   use crate::autooptimizer::seal::{build_and_sign, CycleSeal};
   ```

2. Remove `pub seal: CycleSeal` from `CycleResult`.

3. Remove from `MutationOutcome`:
   ```rust
   diff_hash: ContentHash,
   day_hash: ContentHash,
   untouched_hash: ContentHash,
   ```
   Remove the corresponding DB INSERT column bindings when a `LineageNode` is
   written (the three `.bind(...)` calls for diff_hash / metrics_day_hash /
   metrics_untouched_hash). Remove the local variable assignments that produced
   those hashes.

4. Remove from `run_evening_cycle` signature:
   ```rust
   operator_key: &SigningKey,
   session_id: &str,
   ```

5. Remove the sealing tail (approximately lines 168–183):
   ```rust
   let merkle_root = lineage_store.merkle_root_for_cycle(&cycle_id).await?;
   // ... node_count, build_and_sign, seal.persist, progress(CycleSealed{..})
   ```

6. Remove `seal` from the `CycleResult { .. }` construction at the return site.

**Update callers** — search for all `run_evening_cycle(` call sites
(`crates/xvision-cli/`, tests) and drop the `operator_key` and `session_id`
arguments.

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 4 — Remove Merkle helpers from lineage.rs

**Files touched:**
- `crates/xvision-engine/src/autooptimizer/lineage.rs`

Remove:
- `pub async fn merkle_root_for_cycle(&self, cycle_id: &str) -> Result<ContentHash>`
  (the async DB query + call to `compute_merkle_root`)
- `fn compute_merkle_root(nodes: &[LineageNode]) -> Result<ContentHash>`
  (private helper that iterates nodes and folds hashes)

Keep everything else in `lineage.rs` unchanged.

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 5 — Delete seal.rs, session.rs; drop module declarations from mod.rs

**Files touched:**
- `crates/xvision-engine/src/autooptimizer/seal.rs` (delete)
- `crates/xvision-engine/src/autooptimizer/session.rs` (delete)
- `crates/xvision-engine/src/autooptimizer/mod.rs`

**mod.rs — remove:**
- `pub mod seal;`
- `pub mod session;`
- `pub use seal::{CycleSeal, OPERATOR_DISPLAY_LABEL};` (and any other re-exports
  from these two modules)
- `pub use session::{SessionCommitment, ...};`

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 6 — CLI: remove seal / session-init subcommands and --key-path

**Files touched:**
- `crates/xvision-cli/src/commands/autooptimizer.rs`

Remove:
- `seal` subcommand handler and its `Subcommand::Seal` match arm
- `session-init` (or `SessionInit`) subcommand handler and match arm
- `--key-path <PATH>` argument (hidden, defaulted to `~/.xvn/keys/operator.ed25519`)
  from `evening-cycle` / `mutate-once` / any top-level autooptimizer args struct
- The `merkle root: {e}` error-message string (dead code once seal is gone)

Keep: `gate`, `mutate`, `inspect`, `run`, `ls`, `activate`, `retire`, `lineage`,
`mutate-once`, `evening-cycle`, `demo`.

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 7 — Dashboard: drop seals endpoints + hash column reads

**Files touched:**
- `crates/xvision-dashboard/src/routes/autooptimizer.rs`

Remove:
- `GET /api/autooptimizer/seals` route (list, with limit/offset query params)
- `GET /api/autooptimizer/seals/:cycle_id` route (single by cycle_id)
- `CycleSealRow` struct and its sqlx query
- Any `use crate::... CycleSeal` import in this file
- Router registration for both seals routes

Adjust the `findings/:bundle_hash` SELECT: confirm it reads only columns that
survive the migration (`bundle_hash`, `parent_hash`, `gate_verdict`, `status`,
`cycle_id`, `created_at`, `diversity_score`); remove any reference to the dropped
columns (`diff_hash`, `metrics_day_hash`, `metrics_untouched_hash`).

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 8 — Frontend: remove seal types, API calls, and seal UI view

**Files touched:**
- `frontend/web/src/features/autooptimizer/api.ts`
- The seal UI view component file (identify via `import.*listSeals\|getSeal\|CycleSeal`
  search in `frontend/web/src/features/autooptimizer/`)

**api.ts — remove:**
- `CycleSeal` type definition
- `listSeals()` function (calls `/api/autooptimizer/seals`)
- `getSeal(cycleId: string)` function (calls `/api/autooptimizer/seals/:cycleId`)

**Seal UI view:** delete the component file; remove its import and usage from
any parent route/page component.

**Verification:**
```bash
cd frontend/web
pnpm type-check        # or: pnpm exec tsc --noEmit
pnpm test -- features/autooptimizer
```

---

## Step 9 — Delete seal/session tests; update cycle test

**Files touched:**
- `crates/xvision-engine/tests/autooptimizer_seal.rs` (delete)
- `crates/xvision-engine/tests/autooptimizer_session.rs` (delete)
- `crates/xvision-engine/tests/autooptimizer_cycle.rs` (update)
- `crates/xvision-cli/tests/autooptimizer_cli_session_init.rs` (delete if it
  covers only the removed `session-init` subcommand)
- `crates/xvision-cli/tests/autooptimizer_cli_inspect.rs` (update: remove any
  assertion on seal output in inspect results)

**autooptimizer_cycle.rs updates:**
- Remove `operator_key` / `session_id` from the `run_evening_cycle` call
- Assert that `CycleResult` has no `seal` field (compile-time proof: any access
  to `.seal` should be absent)

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo test --workspace
```

---

## Step 10 — Drop ed25519-dalek (and rand_core if unused)

**Files touched:**
- `crates/xvision-engine/Cargo.toml`

Before editing, grep to confirm no remaining users:
```bash
grep -r "ed25519_dalek\|SigningKey\|VerifyingKey" \
    crates/xvision-engine/src/ --include="*.rs"
```

If the grep is empty, remove from `[dependencies]`:
```toml
# remove:
ed25519-dalek = { version = "2", features = ["rand_core"] }
```

Also check `rand_core` — if it was only needed for ed25519 key generation,
remove it too. Leave `blake3`, `hex`, `sha2`, `dirs` (each has remaining users).

**Verification:**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
```

---

## Step 11 — Delete terminology-lock-sentinel.sh; clean pre-commit hook

**Files touched:**
- `scripts/terminology-lock-sentinel.sh` (delete)
- `.githooks/pre-commit` (edit: remove the sentinel invocation line)

Locate the sentinel call in `.githooks/pre-commit`:
```bash
grep -n "terminology-lock-sentinel\|sentinel" .githooks/pre-commit
```

Remove only that invocation. The worktree-isolation guard (the `if [ "$git_dir" !=
"$common_dir" ]` block) must be preserved intact.

**Verification:**
```bash
bash .githooks/pre-commit   # should pass on main branch with no violations
```

---

## Step 12 — Amend docs: terminology lock + CLAUDE.md

**Files touched:**
- `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`
- `CLAUDE.md`

**Terminology lock doc:** At the top of the cryptographic-provenance section
(Section 2 or equivalent), insert:

> **[REMOVED 2026-06-01]** The provenance layer described in this section
> has been deleted. See
> `docs/superpowers/specs/2026-06-01-remove-autooptimizer-crypto-provenance-design.md`.
> The operator-surface rename pairs in the remaining sections remain in force.

**CLAUDE.md:** Remove the single sentence:

> Cryptographic primitives (BLAKE3, Ed25519, "merkle," canonical JSON) must
> never appear on an operator surface.

Keep all other operator-surface rename pairs (Mutation→Experiment, etc.).

**Verification:** No compilation required. Confirm `git diff --stat` shows only
these two doc files changed in this step.

---

## Final Gate

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-provenance-rm"
scripts/cargo build --workspace
scripts/cargo test --workspace
cd frontend/web && pnpm type-check && pnpm test -- features/autooptimizer
```

Confirm `git status` shows no unexpected modifications outside the planned file
list. The `cycle_seals` and `session_commitments` tables should be absent from
any fresh DB; `run_evening_cycle` should accept no key/session parameters.
