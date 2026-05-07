# CV Extraction Plan — Addendum (Schema Cut)

**Date:** 2026-05-07
**Parent plan:** `docs/superpowers/plans/2026-05-07-cv-extraction.md`
**Trigger:** Plan Task 8 (pre-flight scan) surfaced CV references in the
`Decision` struct schema and SQLite persistence layer that the original
plan did not anticipate.

## What was missed

The original spec/plan assumed CV was contained in three crates
(`-inference`, `-introspect`, `-gating`) plus `VectorConfig` in
`xianvec-eval`. Reality:

1. **`Decision` struct schema** in `xianvec-core/src/trading.rs` has:
   - `pub enum DispositionAxis { Conviction, Patience, RiskAppetite, TrendDisposition }` (line 77)
   - `pub active_vectors: BTreeMap<DispositionAxis, f32>` field on `Decision` (line 143)

2. **SQLite schema** in `xianvec-core/src/store.rs`:
   - `decisions` table keyed on `(setup_id, vector_config_hash)`
   - `risk_outcomes` table keyed on `(setup_id, vector_config_hash)`
   - `vector_config_hash()` function computes hash of `active_vectors`
   - Multiple `INSERT` and `SELECT` statements thread `vector_config_hash`

3. **`xianvec-core/src/substrate.rs`** — entire file is CV substrate
   (steering tensor + introspection hook recording structures).

4. **Construction sites** — every `Decision` literal in the codebase
   sets `active_vectors: BTreeMap::new()` (or with values in trader/harness):
   - `xianvec-eval/src/baselines/{always_long, always_short, buy_and_hold, ma_crossover, macd_momentum, random_direction, rsi_mean_reversion, trader_arm}.rs`
   - `xianvec-eval/src/{harness, metrics, backtest}.rs`
   - `xianvec-execution/src/{alpaca, orderly}.rs`
   - `xianvec-risk/src/lib.rs`
   - `xianvec-harness/src/lib.rs`
   - `xianvec-trader/src/{parse, run, params}.rs` (5 tests)
   - `xianvec-cli` test fixtures (already covered by parent plan Task 17)

5. **Additional ADRs** referencing CV:
   - `decisions/0002-spike-validation.md` — vector validation spike outcome
   - `decisions/0003-related-work.md` — comparable systems
   - `decisions/0005-lookahead-audit.md` — referenced CV in audit
   - `decisions/0007-inference-throughput-routes.md` — CV-driven throughput analysis

6. **Scripts:** `scripts/download_qwen.py` references repeng + steering hooks.

## Resolution: full schema cut (Option B)

Per operator decision (2026-05-07), the field is removed cleanly with a
SQLite migration. No vestigial CV residue. The `Decision` schema becomes
strategy-agnostic; SQLite primary keys move from `(setup_id, vector_config_hash)`
to `(setup_id, arm_name)`, leveraging the already-existing `arm_name` column
to disambiguate per-strategy decisions on the same setup.

## Inserted tasks

These insert into the parent plan **between Task 13 (workspace Cargo.toml)
and Task 14 (xianvec-trader Cargo.toml deps)**. Numbered 13.1 through
13.10 to keep the parent plan numbering stable.

### Task 13.1: Delete xianvec-core/src/substrate.rs

**Files:**
- Delete: `crates/xianvec-core/src/substrate.rs`
- Modify: `crates/xianvec-core/src/lib.rs` (drop `mod substrate;`)

- [ ] **Step 1: Confirm what substrate.rs contains**

```bash
head -50 crates/xianvec-core/src/substrate.rs
```

Expected: steering tensor type, introspection hook structures. Entire
file is CV-only.

- [ ] **Step 2: Confirm dependents**

```bash
grep -rn "use xianvec_core::substrate\|crate::substrate\|substrate::" crates/ --include="*.rs" 2>/dev/null
```

Anything outside `xianvec-core` itself or the deleted CV crates is a gap
to flag.

- [ ] **Step 3: Delete the file**

```bash
git rm crates/xianvec-core/src/substrate.rs
```

- [ ] **Step 4: Update `crates/xianvec-core/src/lib.rs`**

Remove `pub mod substrate;` (or `mod substrate;`) declaration.

- [ ] **Step 5: Build xianvec-core**

```bash
cargo build -p xianvec-core 2>&1 | tail -20
```

Expected: success.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(core): delete substrate.rs (CV steering tensors + introspection)"
```

### Task 13.2: Remove DispositionAxis enum and active_vectors field

**Files:**
- Modify: `crates/xianvec-core/src/trading.rs`

- [ ] **Step 1: Locate and inspect**

```bash
grep -n "DispositionAxis\|active_vectors" crates/xianvec-core/src/trading.rs
```

- [ ] **Step 2: Edit trading.rs**

- Delete the `pub enum DispositionAxis { ... }` block (around line 77, plus its derives + impls).
- Delete the `pub active_vectors: BTreeMap<DispositionAxis, f32>,` line from the `Decision` (or `TraderDecision`) struct.
- Delete any helper `impl Decision { ... }` methods that reference `active_vectors`.
- Update tests in the same file (around line 331) that construct `Decision` with `active_vectors: BTreeMap::from(...)`.
- Drop the `BTreeMap` import if no longer used in this file.

- [ ] **Step 3: Build (will fail across the workspace)**

```bash
cargo build --workspace 2>&1 | tail -40
```

Expected: many compile errors at construction sites. Each will be fixed in subsequent tasks. Note the count for cross-reference.

- [ ] **Step 4: Don't commit yet** — the workspace is broken until 13.3 finishes.

### Task 13.3: Update all Decision construction sites

**Files (one batch commit):**
- Modify: `crates/xianvec-eval/src/baselines/always_long.rs`
- Modify: `crates/xianvec-eval/src/baselines/always_short.rs`
- Modify: `crates/xianvec-eval/src/baselines/buy_and_hold.rs`
- Modify: `crates/xianvec-eval/src/baselines/ma_crossover.rs`
- Modify: `crates/xianvec-eval/src/baselines/macd_momentum.rs`
- Modify: `crates/xianvec-eval/src/baselines/random_direction.rs`
- Modify: `crates/xianvec-eval/src/baselines/rsi_mean_reversion.rs`
- Modify: `crates/xianvec-eval/src/baselines/trader_arm.rs`
- Modify: `crates/xianvec-eval/src/harness.rs`
- Modify: `crates/xianvec-eval/src/metrics.rs`
- Modify: `crates/xianvec-eval/src/backtest.rs`
- Modify: `crates/xianvec-execution/src/alpaca.rs`
- Modify: `crates/xianvec-execution/src/orderly.rs`
- Modify: `crates/xianvec-risk/src/lib.rs`
- Modify: `crates/xianvec-harness/src/lib.rs`

For each file: delete the `active_vectors: BTreeMap::new(),` line (or `active_vectors: BTreeMap::from([...]),` for harness) from `Decision { ... }` literals. Drop unused `use std::collections::BTreeMap;` and `use xianvec_core::trading::DispositionAxis;` imports if they're no longer needed in the file.

- [ ] **Step 1: Mechanical edit pass**

For each file in the list above:
1. Open the file.
2. `grep -n "active_vectors\|DispositionAxis" <file>` to find each occurrence.
3. Delete the `active_vectors: ...,` line in each `Decision { ... }` literal.
4. Remove unused imports.

- [ ] **Step 2: Build progressively**

```bash
cargo build -p xianvec-core 2>&1 | tail -10
cargo build -p xianvec-eval 2>&1 | tail -10
cargo build -p xianvec-execution 2>&1 | tail -10
cargo build -p xianvec-risk 2>&1 | tail -10
cargo build -p xianvec-harness 2>&1 | tail -10
```

Each should pass. If a crate fails: open it, find any remaining `active_vectors` or `DispositionAxis` reference, fix.

- [ ] **Step 3: Don't commit yet** — trader still has refs (Task 13.4).

### Task 13.4: Update xianvec-trader (parse, run, params)

**Files:**
- Modify: `crates/xianvec-trader/src/params.rs`
- Modify: `crates/xianvec-trader/src/parse.rs`
- Modify: `crates/xianvec-trader/src/run.rs`
- Modify: `crates/xianvec-trader/src/prompt.rs`

- [ ] **Step 1: Inspect each file**

```bash
grep -n "active_vectors\|DispositionAxis" crates/xianvec-trader/src/*.rs
```

- [ ] **Step 2: Edit `params.rs`**

Drop the `active_vectors: BTreeMap<DispositionAxis, f32>` field from the params struct. Drop related constructor argument if any.

- [ ] **Step 3: Edit `parse.rs`**

The `parse_trader_response` function takes `active_vectors: BTreeMap<DispositionAxis, f32>` as a parameter and stamps it onto the parsed Decision. Remove this parameter from the function signature. Update the function body to drop the `active_vectors` field assignment. Update tests at lines ~103, ~167, ~174 (`fills_active_vectors_from_caller`, `round_trip_with_active_vectors`) — delete these tests entirely; the behavior they tested no longer exists.

- [ ] **Step 4: Edit `run.rs`**

`run_trader` (or similar) calls `parse_trader_response(..., params.active_vectors.clone())`. Drop the `active_vectors` argument. Drop the test `active_vectors_are_stamped_on_decision` (around line 257) — delete entirely. Update remaining tests' fixtures.

- [ ] **Step 5: Edit `prompt.rs`**

Test name `prompt_advertises_vectors_off_by_default` references the vectors-off framing. The test asserts the prompt advertises vectors are off — but with no vectors at all, this is vestigial. Either delete the test or rename + simplify it to assert the prompt makes no vector mentions.

```bash
grep -n "vectors_off\|vectors_on\|vector\|VectorConfig" crates/xianvec-trader/src/prompt.rs
```

If the prompt template itself contains "vectors off" language, remove that language.

- [ ] **Step 6: Build and test trader**

```bash
cargo build -p xianvec-trader 2>&1 | tail -20
cargo test -p xianvec-trader --lib 2>&1 | tail -30
```

Expected: green.

- [ ] **Step 7: Commit 13.2 + 13.3 + 13.4 together**

```bash
git add -A
git commit -m "feat(core): remove DispositionAxis + active_vectors from Decision schema

Decision struct loses active_vectors: BTreeMap<DispositionAxis, f32>.
DispositionAxis enum deleted. All construction sites updated across
eval baselines, execution, risk, harness, trader. SQLite schema
migration follows in next commit (Task 13.5)."
```

### Task 13.5: SQLite schema migration (store.rs)

**Files:**
- Modify: `crates/xianvec-core/src/store.rs`

The existing `decisions` table is keyed on `(setup_id, vector_config_hash)`. Post-cut, the new key is `(setup_id, arm_name)` (the arm_name column already exists).

- [ ] **Step 1: Inspect current schema**

```bash
grep -n "CREATE TABLE\|PRIMARY KEY\|vector_config_hash" crates/xianvec-core/src/store.rs
```

- [ ] **Step 2: Find the schema-creation SQL**

This is likely in a function like `init_schema` or similar. The CREATE TABLE statement defines columns and PK.

- [ ] **Step 3: Edit the schema**

For the `decisions` table:
- Drop the `vector_config_hash TEXT NOT NULL` column.
- Change `PRIMARY KEY (setup_id, vector_config_hash)` to `PRIMARY KEY (setup_id, arm_name)`.

For the `risk_outcomes` table (if it has the same key):
- Drop `vector_config_hash`.
- New PK depends on what arm_name-equivalent it has. Investigate.

- [ ] **Step 4: Update INSERT/SELECT statements**

Find every `INSERT INTO decisions` and `INSERT INTO risk_outcomes` statement; remove the `vector_config_hash` column and its corresponding `?` bind. Same for SELECT statements that join on or filter by `vector_config_hash`.

- [ ] **Step 5: Delete the `vector_config_hash` function**

```bash
grep -n "fn vector_config_hash" crates/xianvec-core/src/store.rs
```

Delete the function definition and any internal callers.

- [ ] **Step 6: Update `insert_decision` and `insert_risk_outcome`**

Public function signatures that took `vector_config_hash: &str` lose that parameter. Update bodies to not pass it.

- [ ] **Step 7: Update tests**

Tests in store.rs (the spec mentioned line 333 mentions "different vector_config_hash" — those tests need to be reframed as "different arm_name on same setup_id" or deleted if they're vector-specific).

- [ ] **Step 8: Build and test**

```bash
cargo build -p xianvec-core 2>&1 | tail -10
cargo test -p xianvec-core --lib 2>&1 | tail -30
```

Expected: green.

- [ ] **Step 9: Update callers of `insert_decision` / `insert_risk_outcome`**

```bash
grep -rln "insert_decision\|insert_risk_outcome" crates/ --include="*.rs" 2>/dev/null
```

Each caller previously passed `vector_config_hash` as an argument. Drop that argument. Likely callers: xianvec-eval, xianvec-execution, xianvec-risk, xianvec-cli, xianvec-trader.

- [ ] **Step 10: Build the workspace**

```bash
cargo build --workspace 2>&1 | tail -20
```

Expected: green.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat(core): SQLite migration — drop vector_config_hash from decisions/risk_outcomes

decisions PK changes from (setup_id, vector_config_hash) to (setup_id, arm_name).
risk_outcomes follows the same pattern. vector_config_hash() function deleted.
All callers updated to drop the hash argument."
```

### Task 13.6: Workspace test gate

**Files:**
- (None modified; verification gate)

- [ ] **Step 1: Full test run**

```bash
cargo test --workspace 2>&1 | tail -40
```

Expected: green across all 11 remaining crates.

If any test fails: open the failing test, identify the CV-shaped expectation, fix or delete the test.

- [ ] **Step 2: Whole-tree CV grep — early check**

```bash
grep -rn "active_vectors\|DispositionAxis\|vector_config_hash\|vector_config" crates/ --include="*.rs" --include="*.toml" 2>/dev/null
```

Expected: empty. Any matches surface dangling references.

If grep is clean and tests pass: schema cut complete. Resume parent plan at Task 14.

## Inserted doc-reconciliation tasks

These insert into the parent plan **as additional steps under Task 30 (decisions/0001 + 0010 reconciliation)** and **Task 31 (operator docs + scripts)**. Numbered 30.1, 30.2, 30.3, 30.4 and 31.1.

### Task 30.1: Reconcile ADR 0002 (vector validation spike outcome)

**Files:**
- Modify: `decisions/0002-spike-validation.md`

- [ ] **Step 1: Read header**

ADR 0002 documents the vector validation spike outcome (Phase 0.3). It was a CRITICAL GATE result — historical evidence that vectors worked.

- [ ] **Step 2: Add header note at top**

```markdown
> **2026-05-07 status:** Per ADR 0011, CV substrate moved to xianvec-play.
> This ADR is preserved as historical record of the validation spike;
> the work it documents now continues in xianvec-play. References to
> Phase 0.3 / Phase 4 in the body are obsolete in xianvec; their
> equivalents live in xianvec-play.
```

Do not delete the body — it's load-bearing as evidence the technique works.

- [ ] **Step 3: Commit**

```bash
git add decisions/0002-spike-validation.md
git commit -m "docs(decisions): ADR 0002 historical-record header note (ADR 0011)"
```

### Task 30.2: Reconcile ADR 0003 (related work)

**Files:**
- Modify: `decisions/0003-related-work.md`

- [ ] **Step 1: Inspect CV refs**

```bash
grep -n "vector\|control[ -]vector\|steering\|FAISS\|repeng" decisions/0003-related-work.md
```

- [ ] **Step 2: Light edit**

ADR 0003 is a "related work and differentiation" doc — mostly comparing xianvec to TradingAgents, FinMem, etc. Where it positions xianvec as "control-vector trading agent," soften to "multistrategy trading agent with on-chain reputation" — the new positioning.

If a section is entirely about CV differentiation (e.g., "How is this different from TradingAgents — we use control vectors..."), reword to use the new positioning.

- [ ] **Step 3: Commit**

```bash
git add decisions/0003-related-work.md
git commit -m "docs(decisions): ADR 0003 reposition — multistrategy framing (ADR 0011)"
```

### Task 30.3: Reconcile ADR 0005 (lookahead audit)

**Files:**
- Modify: `decisions/0005-lookahead-audit.md`

- [ ] **Step 1: Inspect**

```bash
grep -n "vector\|control[ -]vector\|steering\|active_vectors" decisions/0005-lookahead-audit.md
```

- [ ] **Step 2: Edit**

Lookahead audit is about ensuring backtests don't peek at future data. CV references are likely incidental (e.g., "vector arm scored X"). Drop CV-specific arm references; replace with strategy-arm references.

- [ ] **Step 3: Commit**

```bash
git add decisions/0005-lookahead-audit.md
git commit -m "docs(decisions): ADR 0005 strip CV references (ADR 0011)"
```

### Task 30.4: Reconcile ADR 0007 (inference throughput routes)

**Files:**
- Modify: `decisions/0007-inference-throughput-routes.md`

- [ ] **Step 1: Inspect**

```bash
grep -n "vector\|control[ -]vector\|steering\|FP16\|repeng" decisions/0007-inference-throughput-routes.md
```

- [ ] **Step 2: Edit**

ADR 0007 analyzes inference throughput options. Much of its CV-driven framing (FP16 weights for extraction, candle vs llama.cpp for hidden-state hooks) is no longer relevant. Either:

A) Add a header note marking the ADR partially superseded:
```markdown
> **2026-05-07 status (ADR 0011):** CV-driven sections (FP16 weights for
> extraction, hidden-state hook requirements) are obsolete. Trader-only
> inference throughput considerations remain valid.
```

B) Excise CV-driven sections (preferred — match the "fresh basis" principle).

Operator preference: B per the schema-cut directive. Excise CV sections.

- [ ] **Step 3: Commit**

```bash
git add decisions/0007-inference-throughput-routes.md
git commit -m "docs(decisions): ADR 0007 excise CV-driven sections (ADR 0011)"
```

### Task 31.1: Update or delete scripts/download_qwen.py

**Files:**
- Modify or delete: `scripts/download_qwen.py`

- [ ] **Step 1: Inspect**

```bash
head -30 scripts/download_qwen.py
grep -n "FP16\|repeng\|steering\|hooks\|MLX" scripts/download_qwen.py
```

- [ ] **Step 2: Decide**

The script's docstring mentions:
> 1. MLX 4-bit checkpoint (`mlx-community/Qwen3-32B-4bit`, ~18 GB) — for repeng-style
> ...with steering hooks installed (Phase 0.2 smoke test + Phase 3+ runtime).

If the script is purely CV download orchestration: delete it. The trader doesn't need MLX/repeng-specific weights post-pivot; HTTP backend is the default.

If the script also handles a non-CV download path: edit to keep the non-CV path, drop the CV path.

- [ ] **Step 3: Likely path: delete**

```bash
git rm scripts/download_qwen.py
```

- [ ] **Step 4: Commit**

```bash
git commit -m "chore(scripts): delete download_qwen.py (CV-specific weight downloads, ADR 0011)"
```

## Updated final cleanliness check (parent Task 32)

Add these grep terms to the whole-tree check:

```bash
grep -rin "active_vectors\|DispositionAxis\|vector_config_hash\|vector_config\|substrate::SteeringHook\|introspection.*hook" \
    . \
    --include="*.rs" --include="*.toml" --include="*.md" --include="*.json" --include="*.py" --include="*.sh" --include="*.mermaid" \
    --exclude-dir=target --exclude-dir=.git \
    2>/dev/null
```

Acceptable matches: ADR 0011, ADR 0010 (revision note), ADR 0001/0002/0007 (revision/header notes), spec, plan, plan-addendum, FOLLOWUPS (CVF closed-queue epitaph).

NOT acceptable: any match in `crates/`, `tools/`, live scripts, architecture.md.

---

*Addendum: `docs/superpowers/plans/2026-05-07-cv-extraction-addendum.md`*
*Parent plan: `docs/superpowers/plans/2026-05-07-cv-extraction.md`*
*Spec: `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`*
*ADR: `decisions/0011-cv-extraction.md`*
