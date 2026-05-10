# Terminology Rename (Option B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Sources:** Triage discussion 2026-05-10 (this conversation); ideonomy Run 2 (`docs/superpowers/research/2026-05-10-ideonomy-explorations.md` §156-235) on agent/strategy/variant terminology slippage.
> **Run before:** Wallet plan amendments (`docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-amendments.md`). The amendments use the post-rename terminology; running them on a pre-rename codebase is a merge-conflict hazard.

---

**Goal:** Apply Option B terminology cleanup across the xvision codebase: `setup_id` → `cycle_id` (203 occurrences + DB schema), `Strategy` trait → `Algorithm` trait (xvision-eval baselines), and establish naming policy for new code (`agent_id` for marketplace identity, `PerStrategyVerdict` for the wallet plan's planned per-rule verdict enum).

**Architecture:** Five-phase mechanical rename. Phase 0 establishes a green baseline (full workspace builds, all tests pass) so post-rename regressions are attributable. Phase 1 adds SQL migration 0002 that renames the `setups` table to `cycles` and the `setup_id` column to `cycle_id` across all six referencing tables. Phase 2 renames the Rust identifiers via `cargo check`-driven replace. Phase 3 renames the `Strategy` trait to `Algorithm`. Phase 4 documents the naming policy (no code changes; CLAUDE.md addition). Phase 5 does the verification pass (full build, full test suite, grep for stragglers).

**Tech Stack:** Rust 2021 workspace; sqlx migrations under `crates/xvision-core/migrations/`; SQLite 3.25+ (`ALTER TABLE … RENAME COLUMN` is required and is supported on every macOS/Linux toolchain shipped since 2019).

**Out of scope (deferred):**
- Renaming any of the 12 crate names (`xvision-intern`, `xvision-trader`, etc.) — out of scope per Option B.
- Renaming the `xvn strategy` CLI verb — it manages `StrategyBundle`s (engine pipeline configs); the verb-noun fit is correct.
- Renaming `StrategyBundle`, `StrategyConfigSummary`, `StrategyAction`, `StrategyCmd` — these refer to the immutable pipeline-config concept, which Option B explicitly preserves (the dictionary-cleanup recommendation is to keep `strategy` for *that* meaning and rename the conflicting trait).
- Renaming `arm` (the experimental-arm concept in xvision-eval) — well-bounded, no overlap.
- Renaming any field that's also a JSON key in persisted artifacts (none currently — `setup_id` lives only in DB columns and Rust, not in serialized JSON).

---

## File structure

```
crates/
├── xvision-core/
│   ├── migrations/
│   │   ├── 0001_init.sql                                # UNCHANGED (creates pre-rename schema)
│   │   └── 0002_rename_setup_to_cycle.sql               # NEW — renames setups→cycles, setup_id→cycle_id everywhere
│   └── src/
│       ├── trading.rs                                   # MODIFY: TraderDecision, InternBriefing, OpenPosition setup_id field → cycle_id
│       ├── store.rs                                     # MODIFY: 31 occurrences (struct fields, query strings, helper methods)
│       └── market.rs                                    # MODIFY: 2 occurrences
│
├── xvision-execution/
│   └── src/
│       ├── orderly.rs                                   # MODIFY: 22 occurrences (client_order_id derivation kept; just field rename)
│       ├── alpaca.rs                                    # MODIFY: 21 occurrences (same pattern)
│       └── executor.rs                                  # MODIFY: 4 occurrences
│
├── xvision-eval/
│   ├── src/
│   │   ├── strategy.rs → algorithm.rs                   # RENAME FILE: trait Strategy → Algorithm
│   │   ├── lib.rs                                       # MODIFY: pub mod algorithm; pub use ...
│   │   ├── ab_compare.rs                                # MODIFY: Box<dyn Strategy> → Box<dyn Algorithm>
│   │   ├── harness.rs                                   # MODIFY: 2 trait refs + 2 internal test impls
│   │   ├── backtest.rs                                  # MODIFY: 6 setup_id occurrences
│   │   ├── strategy.rs (the eval one is the file we're renaming; do not confuse with xvision-engine/bundle::StrategyBundle)
│   │   ├── metrics.rs                                   # MODIFY: 2 setup_id
│   │   └── baselines/
│   │       ├── mod.rs                                   # MODIFY: Box<dyn Strategy> → Box<dyn Algorithm>
│   │       ├── trader_arm.rs                            # MODIFY: impl Strategy → impl Algorithm + setup_id refs
│   │       ├── ma_crossover.rs                          # MODIFY: same shape
│   │       ├── macd_momentum.rs                         # MODIFY: same shape
│   │       ├── rsi_mean_reversion.rs                    # MODIFY: same shape
│   │       ├── random_direction.rs                      # MODIFY: same shape
│   │       ├── always_long.rs                           # MODIFY: same shape
│   │       ├── always_short.rs                          # MODIFY: same shape
│   │       └── buy_and_hold.rs                          # MODIFY: same shape
│
├── xvision-intern/
│   └── src/
│       ├── backend.rs                                   # MODIFY: 9 setup_id
│       ├── cache.rs                                     # MODIFY: 6 setup_id
│       ├── prompt.rs                                    # MODIFY: 3 setup_id
│       ├── lib.rs                                       # MODIFY: 1 setup_id
│
├── xvision-trader/
│   └── src/
│       ├── run.rs                                       # MODIFY: 8 setup_id
│       ├── parse.rs                                     # MODIFY: 4 setup_id
│       └── prompt.rs                                    # MODIFY: 3 setup_id
│
├── xvision-identity/
│   └── src/
│       ├── client.rs                                    # MODIFY: 12 setup_id
│       └── manifest.rs                                  # MODIFY: 7 setup_id (do NOT touch StrategyConfigSummary)
│
├── xvision-cli/
│   └── src/
│       ├── lib.rs                                       # MODIFY: 7 setup_id (CLI arg names + command dispatch)
│       └── commands/
│           ├── show_decision.rs                         # MODIFY: 8 setup_id (CLI arg, fn arg, queries)
│           ├── show_briefing.rs                         # MODIFY: 3 setup_id
│           ├── fire_trade.rs                            # MODIFY: 4 setup_id
│           ├── run_setup.rs                             # MODIFY: 2 setup_id (NOTE: filename "run_setup" is the verb — leave the file name; only the field/arg renames)
│           └── intern.rs                                # MODIFY: 1 setup_id
│
├── xvision-risk/
│   └── src/
│       └── lib.rs                                       # MODIFY: 1 setup_id
│
└── xvision-harness/
    └── src/
        └── lib.rs                                       # MODIFY: 1 setup_id

CLAUDE.md (root)                                         # MODIFY: add "Terminology" section documenting agent_id / cycle_id / strategy / Algorithm policy
docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md  # MODIFY: strategy_id → agent_id throughout
docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md  # MODIFY: strategy_id → agent_id throughout (handled by the wallet plan amendments doc, not here — but flag here for visibility)
```

---

## Phase 0 — Establish green baseline

### Task 0.1: Confirm compile + test pass before any changes

**Files:** none (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: success, no errors. If errors, stop — fix before starting rename so any breakage downstream is attributable to the rename, not a pre-existing issue.

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace`
Expected: all tests pass. Capture pass count for comparison after rename:

```bash
cargo test --workspace 2>&1 | grep "test result:" | tee /tmp/xvn-tests-pre-rename.txt
```

- [ ] **Step 3: Snapshot the count of `setup_id` references**

Run:
```bash
rg -c '\bsetup_id\b' --type rust 2>/dev/null | awk -F: '{s+=$2} END {print s}' > /tmp/xvn-setup-id-count-pre.txt
cat /tmp/xvn-setup-id-count-pre.txt
```
Expected: 203 (exact number may differ if files have changed since 2026-05-10; record whatever you see for post-rename comparison: the count must be 0 after Phase 2).

- [ ] **Step 4: Commit a checkpoint marker**

```bash
git add -A && git status
# If clean (no staged changes), skip; otherwise stash:
# git stash push -m "pre-rename-stash"
git tag pre-rename-baseline
```

This tag is the rollback target if Phase 2 goes sideways.

---

## Phase 1 — SQL migration 0002

### Task 1.1: Write migration 0002

**Files:**
- Create: `crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql`

- [ ] **Step 1: Write the migration**

Create `crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql` with exactly:

```sql
-- 0002: Rename setup_id → cycle_id and setups → cycles.
--
-- Rationale: "setup" is overloaded with the `xvn setup` CLI verb (config init).
-- The id ties one InternBriefing → TraderDecision → outcome together, which is
-- naturally a "cycle" through the pipeline. See plan
-- docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md.
--
-- SQLite ALTER TABLE RENAME COLUMN (>= 3.25) propagates references inside
-- the schema (foreign keys, indexes, triggers, views) automatically as long as
-- legacy_alter_table is OFF (default since 3.26). We rely on that here.

-- Step 1: rename the parent table
ALTER TABLE setups RENAME TO cycles;

-- Step 2: rename the primary-key column on cycles
ALTER TABLE cycles RENAME COLUMN setup_id TO cycle_id;

-- Step 3: rename setup_id on each child table. Foreign-key clauses are
-- updated automatically; indexes built on the column are also updated.
ALTER TABLE briefings        RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE decisions        RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE risk_outcomes    RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE executions       RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE traces           RENAME COLUMN setup_id TO cycle_id;

-- Step 4: rename indexes that have "setup" in the name. The columns inside
-- them have already been renamed by Step 3; the index names are cosmetic.
DROP INDEX IF EXISTS idx_decisions_setup;
DROP INDEX IF EXISTS idx_executions_setup;
DROP INDEX IF EXISTS idx_traces_run_setup;

CREATE INDEX IF NOT EXISTS idx_decisions_cycle    ON decisions(cycle_id);
CREATE INDEX IF NOT EXISTS idx_executions_cycle   ON executions(cycle_id);
CREATE INDEX IF NOT EXISTS idx_traces_run_cycle   ON traces(run_id, cycle_id);
```

- [ ] **Step 2: Apply the migration to a fresh database and confirm schema**

Run:
```bash
rm -f /tmp/xvn-rename-test.db
sqlite3 /tmp/xvn-rename-test.db < crates/xvision-core/migrations/0001_init.sql
sqlite3 /tmp/xvn-rename-test.db < crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql
sqlite3 /tmp/xvn-rename-test.db ".schema cycles"
sqlite3 /tmp/xvn-rename-test.db ".schema decisions"
```

Expected output for `cycles`:
```
CREATE TABLE "cycles" (
    cycle_id    TEXT PRIMARY KEY,
    asset       TEXT NOT NULL,
    horizon_h   INTEGER NOT NULL,
    market_state_json TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
```

Expected output for `decisions` (FK clause must reference `cycles(cycle_id)`):
```
CREATE TABLE "decisions" (
    cycle_id            TEXT NOT NULL,
    arm_name            TEXT NOT NULL,
    decision_json       TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (cycle_id, arm_name),
    FOREIGN KEY (cycle_id) REFERENCES cycles(cycle_id)
);
```

If the FK still says `setups(setup_id)`: SQLite is in legacy mode. Check version (`sqlite3 --version`); needs >= 3.26. If you're stuck on an older toolchain, use the table-rebuild fallback in Step 3.

- [ ] **Step 3 (only if Step 2 failed): table-rebuild fallback**

If `ALTER TABLE … RENAME COLUMN` did not propagate the FK reference, replace the migration with a table-rebuild pattern. The `cycles` rename in Step 1 is fine; replace Steps 2-3 with create-new + copy + drop-old + rename-new for each child. Skipped here because the modern path should work; record the SQLite version and contact the operator if you hit this.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql
git commit -m "feat(db): migration 0002 renames setup_id → cycle_id"
```

### Task 1.2: Update Store::migrate to apply 0002 automatically

**Files:**
- Modify: `crates/xvision-core/src/store.rs` (the `Store::migrate` method)

- [ ] **Step 1: Locate the migrate method**

Run: `rg -n "fn migrate" crates/xvision-core/src/store.rs`
Expected: a method that runs the migrations dir in lexical order. If it uses `sqlx::migrate!("./migrations")`, no code change is needed — sqlx picks up the new file automatically; skip steps 2-3.

- [ ] **Step 2: If migrate uses an explicit list, append the new migration**

If the migrate method explicitly enumerates each migration file (rather than relying on `sqlx::migrate!`), append `"0002_rename_setup_to_cycle.sql"` to the list in source order.

- [ ] **Step 3: Run the existing migration test**

Run: `cargo test -p xvision-core --lib migrate`
Expected: all migration tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-core/src/store.rs
git commit -m "feat(db): wire migration 0002 into Store::migrate"
```

(If Step 1 found that `sqlx::migrate!` already auto-discovers files, this commit is empty — skip.)

---

## Phase 2 — Rust source: setup_id → cycle_id

This is the wide mechanical change. ~203 occurrences across ~30 files. Do it in one commit per crate so each crate's tests pass independently after its commit.

### Task 2.1: Rename in `xvision-core`

**Files:**
- Modify: `crates/xvision-core/src/store.rs` (31 occurrences)
- Modify: `crates/xvision-core/src/trading.rs` (5 occurrences — `TraderDecision.setup_id`, `InternBriefing.setup_id`, `OpenPosition.setup_id`, etc.)
- Modify: `crates/xvision-core/src/market.rs` (2 occurrences)

- [ ] **Step 1: Apply the substitution in xvision-core only**

Run:
```bash
find crates/xvision-core -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-core -name '*.rs.bak' -delete
```

- [ ] **Step 2: Update SQL query strings inside store.rs**

The sed pass already handled the literal `setup_id` inside the SQL strings (since they're raw text). But the table name `setups` also needs updating to `cycles` inside SQL strings. Search:

```bash
rg -n '\bFROM setups\b|\bUPDATE setups\b|\bINTO setups\b|\bsetups\b' crates/xvision-core/src/store.rs
```

For each match, replace `setups` with `cycles`. Verify no remaining lowercase `setups` references in `.rs` files:

```bash
rg -n '"[^"]*\bsetups\b[^"]*"' crates/xvision-core/src/ 2>/dev/null
```
Expected: no matches.

- [ ] **Step 3: Build the crate**

Run: `cargo build -p xvision-core`
Expected: success. If failures, they are likely SQL string mismatches the sed missed; fix and retry.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p xvision-core`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-core
git commit -m "refactor(core): rename setup_id → cycle_id and setups → cycles"
```

### Task 2.2: Rename in `xvision-intern`

**Files:**
- Modify: `crates/xvision-intern/src/backend.rs` (9 occurrences)
- Modify: `crates/xvision-intern/src/cache.rs` (6 occurrences)
- Modify: `crates/xvision-intern/src/prompt.rs` (3 occurrences)
- Modify: `crates/xvision-intern/src/lib.rs` (1 occurrence)

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-intern -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-intern -name '*.rs.bak' -delete
```

- [ ] **Step 2: Build and test**

Run: `cargo build -p xvision-intern && cargo test -p xvision-intern`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-intern
git commit -m "refactor(intern): rename setup_id → cycle_id"
```

### Task 2.3: Rename in `xvision-trader`

**Files:**
- Modify: `crates/xvision-trader/src/run.rs` (8 occurrences)
- Modify: `crates/xvision-trader/src/parse.rs` (4 occurrences)
- Modify: `crates/xvision-trader/src/prompt.rs` (3 occurrences)

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-trader -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-trader -name '*.rs.bak' -delete
```

- [ ] **Step 2: Build and test**

Run: `cargo build -p xvision-trader && cargo test -p xvision-trader`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-trader
git commit -m "refactor(trader): rename setup_id → cycle_id"
```

### Task 2.4: Rename in `xvision-identity`

**Files:**
- Modify: `crates/xvision-identity/src/client.rs` (12 occurrences)
- Modify: `crates/xvision-identity/src/manifest.rs` (7 occurrences — do NOT touch `StrategyConfigSummary`, `strategy_config`, or any `Strategy*` identifier; only `setup_id`)

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-identity -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-identity -name '*.rs.bak' -delete
```

- [ ] **Step 2: Verify no Strategy* identifiers were touched**

```bash
rg -n 'Strategy' crates/xvision-identity/src/ | head -10
```
Expected: still see `StrategyConfigSummary`, `strategy_config` (unchanged).

- [ ] **Step 3: Build and test**

Run: `cargo build -p xvision-identity && cargo test -p xvision-identity`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-identity
git commit -m "refactor(identity): rename setup_id → cycle_id"
```

### Task 2.5: Rename in `xvision-execution`

**Files:**
- Modify: `crates/xvision-execution/src/orderly.rs` (22 occurrences)
- Modify: `crates/xvision-execution/src/alpaca.rs` (21 occurrences)
- Modify: `crates/xvision-execution/src/executor.rs` (4 occurrences)

Important: `client_order_id` derivation (`format!("tp-{}", td.setup_id)` etc.) must be updated to use the renamed field.

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-execution -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-execution -name '*.rs.bak' -delete
```

- [ ] **Step 2: Verify the client_order_id format strings still compile**

Run:
```bash
rg -n 'format!\("(tp|sl)-\{\}' crates/xvision-execution/src/orderly.rs
```
Expected: any matches show `td.cycle_id` (or whatever the surrounding variable was) — confirm the field rename propagated into the format args.

- [ ] **Step 3: Build and test**

Run: `cargo build -p xvision-execution && cargo test -p xvision-execution`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-execution
git commit -m "refactor(execution): rename setup_id → cycle_id"
```

### Task 2.6: Rename in `xvision-eval`

**Files:**
- Modify: `crates/xvision-eval/src/baselines/trader_arm.rs` (10 setup_id occurrences — do NOT change `impl Strategy for TraderArm` here; that's Task 3.x)
- Modify: `crates/xvision-eval/src/baselines/ma_crossover.rs` (5 setup_id)
- Modify: `crates/xvision-eval/src/baselines/macd_momentum.rs` (4)
- Modify: `crates/xvision-eval/src/baselines/buy_and_hold.rs` (4)
- Modify: `crates/xvision-eval/src/baselines/rsi_mean_reversion.rs` (3)
- Modify: `crates/xvision-eval/src/baselines/random_direction.rs` (3)
- Modify: `crates/xvision-eval/src/baselines/always_long.rs` (3)
- Modify: `crates/xvision-eval/src/baselines/always_short.rs` (3)
- Modify: `crates/xvision-eval/src/baselines/mod.rs` (1)
- Modify: `crates/xvision-eval/src/backtest.rs` (6)
- Modify: `crates/xvision-eval/src/strategy.rs` (2 — soon to be renamed in Phase 3, but rename setup_id first)
- Modify: `crates/xvision-eval/src/metrics.rs` (2)
- Modify: `crates/xvision-eval/src/harness.rs` (2)

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-eval -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-eval -name '*.rs.bak' -delete
```

- [ ] **Step 2: Build and test**

Run: `cargo build -p xvision-eval && cargo test -p xvision-eval`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-eval
git commit -m "refactor(eval): rename setup_id → cycle_id"
```

### Task 2.7: Rename in `xvision-cli`

**Files:**
- Modify: `crates/xvision-cli/src/lib.rs` (7 occurrences — includes CLI argument names like `setup_id` in `Command::ShowDecision { setup_id, .. }`)
- Modify: `crates/xvision-cli/src/commands/show_decision.rs` (8)
- Modify: `crates/xvision-cli/src/commands/show_briefing.rs` (3)
- Modify: `crates/xvision-cli/src/commands/fire_trade.rs` (4)
- Modify: `crates/xvision-cli/src/commands/run_setup.rs` (2 — only the field; the file itself stays `run_setup.rs`)
- Modify: `crates/xvision-cli/src/commands/intern.rs` (1)

Important: CLI argument names visible to operators change here. `xvn show-decision --setup-id <id>` becomes `xvn show-decision --cycle-id <id>`. This is a breaking CLI change for the operator. There is one operator (the user); this is acceptable. Document in commit message.

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-cli -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates/xvision-cli -name '*.rs.bak' -delete
```

- [ ] **Step 2: Verify CLI arg attribute spellings**

The sed handles `setup_id` but CLI args may use `--setup-id` (kebab-case in the CLI). Search:

```bash
rg -n '"setup-id"|"setup_id"|setup-id' crates/xvision-cli/src/
```

For any match, replace with the cycle equivalent (`cycle-id` for kebab-case, `cycle_id` for snake_case). If clap derives the CLI arg from the field name automatically, no manual change needed; clap will now expose `--cycle-id`.

- [ ] **Step 3: Build and test**

Run: `cargo build -p xvision-cli && cargo test -p xvision-cli`
Expected: success. Test fixtures may need their argument strings updated if they pass `--setup-id` literally.

- [ ] **Step 4: Smoke-test the CLI**

Run: `cargo run -p xvision-cli -- show-decision --help 2>&1 | head -20`
Expected: usage shows `--cycle-id` (not `--setup-id`).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli
git commit -m "refactor(cli)!: rename --setup-id arg → --cycle-id

BREAKING: CLI argument --setup-id is now --cycle-id. Single-operator project pre-launch; no external users."
```

### Task 2.8: Rename in remaining crates (risk, harness, engine if any)

**Files:**
- Modify: `crates/xvision-risk/src/lib.rs` (1)
- Modify: `crates/xvision-harness/src/lib.rs` (1)
- Plus any `setup_id` still found in xvision-engine, xvision-mcp, xvision-data (likely none; verify)

- [ ] **Step 1: Apply substitution**

```bash
find crates/xvision-risk crates/xvision-harness crates/xvision-engine crates/xvision-mcp crates/xvision-data -name '*.rs' -exec sed -i.bak 's/\bsetup_id\b/cycle_id/g' {} \;
find crates -name '*.rs.bak' -delete
```

- [ ] **Step 2: Build the workspace**

Run: `cargo build --workspace`
Expected: success.

- [ ] **Step 3: Run the workspace tests**

Run: `cargo test --workspace`
Expected: all pass. Compare pass count to `/tmp/xvn-tests-pre-rename.txt` — must match.

- [ ] **Step 4: Verify zero remaining setup_id references in Rust**

Run:
```bash
rg -c '\bsetup_id\b' --type rust 2>/dev/null | awk -F: '{s+=$2} END {print s+0}'
```
Expected: `0`. If non-zero, locate and fix:
```bash
rg -n '\bsetup_id\b' --type rust 2>/dev/null
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: rename setup_id → cycle_id in remaining crates"
```

---

## Phase 3 — `Strategy` trait → `Algorithm` trait

The `Strategy` trait lives in `xvision-eval` and is implemented by 8 baselines + a test impl. The trait's purpose is "an executable trading-decision producer that runs against a fixed input and emits an `Action`-like output." `Algorithm` captures that role without colliding with the marketplace `strategy` concept (i.e., `StrategyBundle`).

### Task 3.1: Rename `strategy.rs` → `algorithm.rs`

**Files:**
- Rename: `crates/xvision-eval/src/strategy.rs` → `crates/xvision-eval/src/algorithm.rs`
- Modify: `crates/xvision-eval/src/lib.rs`

- [ ] **Step 1: Move the file**

```bash
git mv crates/xvision-eval/src/strategy.rs crates/xvision-eval/src/algorithm.rs
```

- [ ] **Step 2: Rename the trait inside the file**

In `crates/xvision-eval/src/algorithm.rs`, replace `pub trait Strategy` with `pub trait Algorithm`. Verify with:

```bash
rg -n 'trait (Strategy|Algorithm)' crates/xvision-eval/src/algorithm.rs
```
Expected: one line, `pub trait Algorithm: Send + Sync {`.

- [ ] **Step 3: Update lib.rs**

In `crates/xvision-eval/src/lib.rs`, find:

```rust
pub mod strategy;
```
or
```rust
mod strategy;
pub use strategy::Strategy;
```

Replace with the algorithm equivalent. Both forms must end up exposing `Algorithm`:

```rust
pub mod algorithm;
pub use algorithm::Algorithm;
```

- [ ] **Step 4: Build the crate (will fail; that's expected)**

Run: `cargo build -p xvision-eval`
Expected: failures of the form `cannot find trait 'Strategy'` in baselines/* and other places. Capture the file list:

```bash
cargo build -p xvision-eval 2>&1 | grep "cannot find trait" | head
```

This is the input to Task 3.2.

### Task 3.2: Update all `impl Strategy` and `dyn Strategy` references

**Files:**
- Modify: `crates/xvision-eval/src/baselines/trader_arm.rs`
- Modify: `crates/xvision-eval/src/baselines/ma_crossover.rs`
- Modify: `crates/xvision-eval/src/baselines/macd_momentum.rs`
- Modify: `crates/xvision-eval/src/baselines/rsi_mean_reversion.rs`
- Modify: `crates/xvision-eval/src/baselines/random_direction.rs`
- Modify: `crates/xvision-eval/src/baselines/always_long.rs`
- Modify: `crates/xvision-eval/src/baselines/always_short.rs`
- Modify: `crates/xvision-eval/src/baselines/buy_and_hold.rs`
- Modify: `crates/xvision-eval/src/baselines/mod.rs`
- Modify: `crates/xvision-eval/src/ab_compare.rs`
- Modify: `crates/xvision-eval/src/harness.rs` (also the two test impls inside)

- [ ] **Step 1: Apply targeted replacements**

These replacements are word-bounded but limited to `xvision-eval` to avoid touching `StrategyBundle`, `StrategyConfigSummary`, etc. in other crates:

```bash
find crates/xvision-eval -name '*.rs' -exec sed -i.bak 's/\bimpl Strategy\b/impl Algorithm/g' {} \;
find crates/xvision-eval -name '*.rs' -exec sed -i.bak 's/\bdyn Strategy\b/dyn Algorithm/g' {} \;
find crates/xvision-eval -name '*.rs' -exec sed -i.bak 's/\buse crate::strategy::Strategy\b/use crate::algorithm::Algorithm/g' {} \;
find crates/xvision-eval -name '*.rs' -exec sed -i.bak 's/\buse crate::Strategy\b/use crate::Algorithm/g' {} \;
find crates/xvision-eval -name '*.rs' -exec sed -i.bak 's/\bxvision_eval::Strategy\b/xvision_eval::Algorithm/g' {} \;
find crates/xvision-eval -name '*.rs.bak' -delete
```

- [ ] **Step 2: Inspect any remaining `Strategy` reference in xvision-eval**

```bash
rg -n '\bStrategy\b' crates/xvision-eval/src/
```
Expected: zero matches. If matches remain, they are likely:
- A `Strategy` argument-type in a function signature: replace with `Algorithm`.
- A bound on a generic: `T: Strategy` → `T: Algorithm`.
- A doc comment or string mentioning the old trait: leave it if it's prose; replace if it's intended to be code-accurate.

- [ ] **Step 3: Build the crate**

Run: `cargo build -p xvision-eval`
Expected: success.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p xvision-eval`
Expected: success.

- [ ] **Step 5: Build and test the workspace (catches downstream consumers)**

Run: `cargo build --workspace && cargo test --workspace`
Expected: success. If a downstream crate (e.g., xvision-cli, xvision-engine) imports `xvision_eval::Strategy`, that import fails and the build error names the file. Replace with `xvision_eval::Algorithm`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(eval): rename Strategy trait → Algorithm; strategy.rs → algorithm.rs"
```

---

## Phase 4 — Naming policy for new code

No code changes here — only documentation that locks in the policy decisions for forward work.

### Task 4.1: Add a "Terminology" section to CLAUDE.md

**Files:**
- Modify: `/Users/edkennedy/Code/xvision/CLAUDE.md` (project-level CLAUDE.md if it exists; or create the project section in the parent)

- [ ] **Step 1: Locate the project-level CLAUDE.md**

Run: `ls /Users/edkennedy/Code/xvision/CLAUDE.md /Users/edkennedy/Code/xvision/.claude/CLAUDE.md 2>/dev/null`

If neither exists, create `/Users/edkennedy/Code/xvision/CLAUDE.md`. Otherwise, modify the existing one.

- [ ] **Step 2: Append the Terminology section**

Add the following section. If the file already has a `## Terminology` section, replace its body; otherwise, append at the bottom of the file:

```markdown
## Terminology

Naming conventions across the xvision codebase. Lock in 2026-05-10 (terminology
rename Option B). Diverging from these names should require a written rationale.

| Concept | Use this name | Don't use |
|---|---|---|
| Per-decision-cycle id (briefing → decision → outcome) | `cycle_id` | ~~setup_id~~ |
| Pre-mint local id of a marketplace pipeline | `agent_id` (string ULID, becomes the NFT token id post-mint) | ~~strategy_id~~ |
| Immutable pipeline configuration (Phase-1 engine artifact) | `StrategyBundle` (existing type) | (no rename) |
| Trading-decision producer trait (xvision-eval baselines) | `Algorithm` | ~~Strategy~~ |
| One experimental arm in A/B compare | `arm` / `Box<dyn Algorithm>` | (no change) |
| The trader's call (input to risk) | `TraderDecision` | (no change) |
| The risk gate's verdict (Approved / Modified / Vetoed) | `RiskDecision` | (no change) |
| The wallet plan's per-rule verdict (planned new enum) | `PerStrategyVerdict` | ~~Verdict~~ (collides with RiskDecision in spirit) |

**Pipeline-stage names** (intern, trader, risk, executor) are roles in the
processing pipeline and are NOT renamed. The `xvn strategy` CLI verb manages
`StrategyBundle`s and is NOT renamed.

**Migration notes:**
- DB migration `0002_rename_setup_to_cycle.sql` renamed the `setups` table to
  `cycles` and the `setup_id` column to `cycle_id` across all six referencing
  tables (briefings, decisions, risk_outcomes, executions, traces).
- The CLI argument `--setup-id` is now `--cycle-id`. Pre-launch breaking change.
- Pre-rename git tag: `pre-rename-baseline`.
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add Terminology section to CLAUDE.md"
```

### Task 4.2: Update wallet plan & spec doc terminology

**Files:**
- Modify: `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`
- Modify: `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`

The wallet plan amendments doc (`2026-05-10-blockchain-1-non-custodial-wallets-amendments.md`) handles its own terminology updates. This task only updates the spec + the existing plan so they're consistent before amendments execution.

- [ ] **Step 1: Replace strategy_id → agent_id in the spec**

```bash
sed -i.bak 's/\bstrategy_id\b/agent_id/g' docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md
rm docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md.bak
```

- [ ] **Step 2: Replace in the existing wallet plan**

```bash
sed -i.bak 's/\bstrategy_id\b/agent_id/g' docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md
rm docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md.bak
```

- [ ] **Step 3: Add a header note to both files**

Add a single line near the top of each file (under the existing first paragraph), exactly:

```
> **Terminology:** Updated 2026-05-10 — `strategy_id` renamed to `agent_id` per Option B (see `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`). The id is a local ULID pre-mint, resolves to the NFT token id post-SLF3.
```

- [ ] **Step 4: Verify no `strategy_id` references remain in the wallet artifacts**

```bash
rg -n '\bstrategy_id\b' docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md
```
Expected: no matches.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md
git commit -m "docs: rename strategy_id → agent_id in non-custodial-wallets spec + plan"
```

---

## Phase 5 — Verification

### Task 5.1: Full-workspace build + test

**Files:** none

- [ ] **Step 1: Clean build**

Run: `cargo clean && cargo build --workspace`
Expected: success.

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace 2>&1 | tee /tmp/xvn-tests-post-rename.txt`
Expected: pass count matches `/tmp/xvn-tests-pre-rename.txt`. If pass count changed, investigate which tests changed status.

- [ ] **Step 3: Verify zero `setup_id` and zero `Strategy` (trait) refs in eval**

```bash
echo "setup_id remaining (should be 0):"
rg -c '\bsetup_id\b' --type rust 2>/dev/null | awk -F: '{s+=$2} END {print s+0}'

echo "Strategy in xvision-eval (should be 0 — only Algorithm):"
rg -c '\bStrategy\b' crates/xvision-eval/src/ 2>/dev/null | awk -F: '{s+=$2} END {print s+0}'
```
Expected: both 0.

- [ ] **Step 4: Verify StrategyBundle / StrategyConfigSummary still exist (those should NOT have been renamed)**

```bash
rg -l 'StrategyBundle|StrategyConfigSummary' crates/ --type rust
```
Expected: matches in xvision-engine and xvision-identity (not zero, not in xvision-eval).

- [ ] **Step 5: Smoke-test a CLI command end-to-end**

Run: `cargo run -p xvision-cli -- --help 2>&1 | head -30`
Expected: success, no `setup` references in arg names.

- [ ] **Step 6: Smoke-test the migration on an existing (pre-migration) database, if one exists**

If a developer DB exists at `xvn.db` or similar, point the migration at it:

```bash
DB_PATH=$(find . -name "xvn*.db" -maxdepth 3 2>/dev/null | head -1)
if [ -n "$DB_PATH" ]; then
  echo "Running migration on $DB_PATH"
  sqlite3 "$DB_PATH" < crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql
  sqlite3 "$DB_PATH" "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;"
fi
```
Expected: tables include `cycles` (not `setups`).

- [ ] **Step 7: Final commit (only if any docs touched in verification)**

If no changes from steps 1-6, no commit needed. Otherwise:

```bash
git add -A
git commit -m "verify: post-rename smoke checks pass"
```

### Task 5.2: Update memory + cleanup

**Files:** none (housekeeping)

- [ ] **Step 1: Delete the rollback tag if everything is green**

```bash
git tag -d pre-rename-baseline
```

If you are not confident, leave the tag and document its expiration in the next PR.

- [ ] **Step 2: Verify the wallet plan amendments doc references current terminology**

```bash
rg -n 'setup_id|strategy_id|Strategy(?!Bundle|ConfigSummary|Cmd|Action)' docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-amendments.md 2>/dev/null
```
Expected: no matches (or only matches that are explicitly historical references in prose).

If matches exist, hand off to the wallet-amendments executor to update before that plan starts.

---

## Self-review

Per the writing-plans skill, after writing run a final pass:

**Spec coverage** — every concrete rename in the Option B set is covered:
- `setup_id` → `cycle_id` (DB schema in Phase 1; Rust source in Phase 2)
- `setups` table → `cycles` table (Phase 1.1)
- `Strategy` trait → `Algorithm` trait (Phase 3)
- `strategy.rs` → `algorithm.rs` (Phase 3.1)
- `agent_id` policy for new code (Phase 4.1 doc)
- `PerStrategyVerdict` policy for the wallet plan's planned enum (Phase 4.1 doc)
- `strategy_id` in spec/plan docs → `agent_id` (Phase 4.2)

**Placeholder scan** — no TBDs, no "implement later", no "similar to Task N". Each step has either an exact command, a code block, or a specific file modification with the change shown.

**Type/name consistency** — `Algorithm` (not `Algorithm`/`EvalArm`/`Baseline` mixed); `cycle_id` not `cycle-id`/`cycleId`; CLI arg uses kebab-case `--cycle-id` (Task 2.7) which clap derives from the snake_case field automatically.

**Known cross-plan touch** — Phase 4.2 only updates the spec + the original wallet plan doc. The wallet plan AMENDMENTS doc (separate plan, sibling) is written using the post-rename terminology natively, so it does not need a sed pass.
