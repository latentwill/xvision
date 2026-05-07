# CV Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract control-vector substrate from xianvec to xianvec-play (full-history merge), then slim CV from xianvec, leaving a multistrategy + ERC-8004 marketplace codebase.

**Architecture:** Two-phase. Phase 1 fast-merges `xianvec/main` into xianvec-play with `--allow-unrelated-histories` so xianvec-play inherits the full CV development trail. Phase 2 branches `pivot/cv-extract` off xianvec's `main`, deletes CV crates and tooling, modifies remaining crates to drop `VectorConfig` references, and reconciles docs (architecture.md, FOLLOWUPS, ADRs, operator docs). Final merge to `main`; `hackathon/turing` deleted post-merge.

**Tech Stack:** Rust workspace (Cargo, ~14 crates → 11), git, bash. No new dependencies. Validation is `cargo build --workspace` and `cargo test --workspace` between code-change tasks.

**Spec:** `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`
**ADR:** `decisions/0011-cv-extraction.md`

---

## File Structure

### Phase 1 — xianvec-play (`/Users/edkennedy/Code/xianvec-play`)

No file additions or deletions. The merge brings xianvec's tree in alongside xianvec-play's existing `README.md`, `RESEARCH_LOG.md`, and `research/`. No path conflicts.

### Phase 2 — xianvec (`/Users/edkennedy/Code/xianvec`)

**Files deleted:**
- `crates/xianvec-inference/` (whole crate)
- `crates/xianvec-introspect/` (whole crate)
- `crates/xianvec-gating/` (whole crate)
- `tools/extract_vectors/` (Python repeng extractor)
- `data/vectors/` (FAISS indexes + manifests)
- `notebooks/inspect_vector.py`
- `identity/vectors_off.agent.json`
- `identity/vectors_on.agent.json`
- `steering-vector-architecture.md`
- `implementation-plan-python-archive.md`
- `crates/xianvec-cli/src/commands/explain_vectors.rs`
- `decisions/0009-qwen3-next-runtime-options.md` (moved to xianvec-play, not deleted from history)

**Files modified:**
- `Cargo.toml` (workspace `members` + `default-members`)
- `crates/xianvec-eval/src/baselines/mod.rs` and `crates/xianvec-eval/src/baselines/trader_arm.rs` (or wherever `VectorConfig` lives)
- `crates/xianvec-eval/src/ab_compare.rs`
- `crates/xianvec-cli/src/commands/mod.rs`
- `crates/xianvec-cli/src/main.rs`
- `crates/xianvec-cli/src/commands/show_metrics.rs`
- `crates/xianvec-cli/src/commands/report.rs`
- `crates/xianvec-cli/src/commands/show_decision.rs`
- `crates/xianvec-identity/src/manifest.rs`
- `crates/xianvec-identity/src/lib.rs`
- `crates/xianvec-trader/Cargo.toml`
- `architecture.md` (multi-section revision)
- `architecture-diagram.mermaid`
- `FOLLOWUPS.md`
- `implementation-plan.md`
- `decisions/0001-inference-backend.md`
- `decisions/0010-hackathon-pivot-strategy-loom.md` (header note)
- `MANUAL.md`
- `v1-build-steps.md`
- `scripts/setup_runpod.sh`

---

## Phase 1 — Copy (full-history merge into xianvec-play)

### Task 1: Pull origin updates on xianvec-play

**Files:**
- Modify: `.git/refs/heads/master` (fast-forward via pull)

- [ ] **Step 1: Switch to xianvec-play and check status**

```bash
cd /Users/edkennedy/Code/xianvec-play
git status
```

Expected: `On branch master`, working tree clean (apart from `.DS_Store` untracked).

- [ ] **Step 2: Pull**

```bash
git pull
```

Expected: fast-forward of 2 commits OR "Already up to date." Either is fine.

- [ ] **Step 3: Verify clean state**

```bash
git status
git log --oneline -5
```

Expected: clean tree, log shows the 2 commits (`Initial commit: xianvec-play vision...` and `Add CAST, SVF, and all discussed research links`) at HEAD.

### Task 2: Add xianvec as a local remote and fetch

**Files:**
- Modify: `.git/config` (add remote)

- [ ] **Step 1: Add the remote**

```bash
git remote add xianvec /Users/edkennedy/Code/xianvec
```

- [ ] **Step 2: Fetch all branches**

```bash
git fetch xianvec
```

Expected: list of fetched refs including `xianvec/main`, `xianvec/hackathon/turing`, `xianvec/phase-0-1`.

- [ ] **Step 3: Confirm fetched refs**

```bash
git branch -r | grep xianvec
git rev-parse xianvec/main
```

Expected: three `xianvec/*` remote branches; `xianvec/main` resolves to a commit SHA (should be `eb800d7...` matching the spec commit).

### Task 3: Merge xianvec/main with --allow-unrelated-histories

**Files:**
- Modify: working tree (xianvec contents land in xianvec-play)
- Modify: `.git/HEAD` (merge commit on master)

- [ ] **Step 1: Capture source SHA into commit message**

```bash
SOURCE_SHA=$(git rev-parse --short xianvec/main)
echo "$SOURCE_SHA"
```

Expected: 7-char SHA like `eb800d7`. Use this in the next step.

- [ ] **Step 2: Merge**

```bash
git merge xianvec/main --allow-unrelated-histories \
    -m "Import xianvec @ $SOURCE_SHA (CV extraction, ADR 0011)" \
    -m "Full-history merge per spec docs/superpowers/specs/2026-05-07-cv-extraction-design.md. xianvec-play becomes the home of the CV substrate; xianvec slims down separately."
```

Expected: merge commit created. No conflicts (xianvec-play's `README.md`, `RESEARCH_LOG.md`, `research/` do not exist in xianvec; xianvec's tree comes in clean).

If the merge editor opens despite `-m`, save and exit. If unexpected conflicts arise on `README.md` or other paths, abort and investigate:

```bash
git merge --abort
ls -la README.md research/ RESEARCH_LOG.md  # confirm xianvec-play only
git ls-tree --name-only xianvec/main | grep -E "^(README\.md|RESEARCH_LOG\.md|research)$"  # should be empty
```

- [ ] **Step 3: Verify the merge result**

```bash
git log --oneline -10
ls
```

Expected:
- Log: shows merge commit at top, then both histories interleaved (xianvec-play's 2 commits + xianvec's commits including `eb800d7 docs(pivot): ADR 0011...`).
- Listing: shows BOTH xianvec-play files (`README.md`, `RESEARCH_LOG.md`, `research/`) AND xianvec files (`architecture.md`, `Cargo.toml`, `crates/`, `decisions/`, etc.).

### Task 4: Validate the merge — workspace builds

**Files:**
- (None modified; verification only)

- [ ] **Step 1: Build the Rust workspace**

```bash
cd /Users/edkennedy/Code/xianvec-play
cargo build --workspace 2>&1 | tail -40
```

Expected: build succeeds with the same output as the source xianvec repo. (If you want to be fastidious: also run from xianvec and diff outputs.)

If build fails specifically due to a stale `target/` directory mismatch between repos: clean and retry.

```bash
cargo clean
cargo build --workspace 2>&1 | tail -20
```

- [ ] **Step 2: Smoke-run library tests**

```bash
cargo test --workspace --lib 2>&1 | tail -30
```

Expected: tests pass at parity with xianvec (any pre-existing flakes carry over). If a test fails that passes on xianvec, abort and investigate — it suggests path or env state differences between the repos.

### Task 5: Remove the local xianvec remote

**Files:**
- Modify: `.git/config` (remove remote)

- [ ] **Step 1: Remove the remote**

```bash
git remote remove xianvec
git remote -v
```

Expected: only `origin` remains.

### Task 6: Push xianvec-play to origin

**Files:**
- Update remote: `origin/master`

- [ ] **Step 1: Final state check**

```bash
git status
git log --oneline -5
```

Expected: clean tree; merge commit + ADR 0011 commit visible at the top of the log.

- [ ] **Step 2: Push**

```bash
git push origin master
```

If push is rejected (origin diverged unexpectedly), STOP and confirm with the operator before forcing or rebasing. Do not force-push without authorization.

Phase 1 complete: xianvec-play has the full xianvec history and the CV substrate is live there.

---

## Phase 2 — xianvec slim-down

### Task 7: Create the pivot branch

**Files:**
- Modify: `.git/refs/heads/pivot/cv-extract` (created)

- [ ] **Step 1: Switch to xianvec, ensure on main**

```bash
cd /Users/edkennedy/Code/xianvec
git checkout main
git status
git rev-parse --short HEAD
```

Expected: on `main`, clean tree, HEAD at `eb800d7` (the spec commit).

- [ ] **Step 2: Create and switch to pivot branch**

```bash
git checkout -b pivot/cv-extract
git branch --show-current
```

Expected: `pivot/cv-extract`.

### Task 8: Pre-flight scan for CV-shaped references

**Files:**
- Create: `/tmp/cv-references.txt` (working file, not committed)

This task produces a reference list to cross-check that no CV-shaped code is missed by the deletion/modification tasks below.

- [ ] **Step 1: Grep for CV terms**

```bash
cd /Users/edkennedy/Code/xianvec
grep -rln "VectorConfig\|vectors_on\|vectors_off\|vector_config\|vector_manifest\|active_vectors\|FAISS\|faiss-rs\|faiss_rs\|repeng\|control[ -]vector\|steering[ -]vector\|introspect\|control_vectors\|--features control-vectors" \
    crates/ tools/ scripts/ identity/ notebooks/ \
    architecture.md FOLLOWUPS.md MANUAL.md v1-build-steps.md implementation-plan.md \
    architecture-diagram.mermaid steering-vector-architecture.md implementation-plan-python-archive.md \
    decisions/ \
    --include="*.rs" --include="*.toml" --include="*.md" --include="*.json" --include="*.py" --include="*.sh" --include="*.mermaid" \
    2>/dev/null | sort -u > /tmp/cv-references.txt
echo "TOTAL FILES:"; wc -l /tmp/cv-references.txt
echo "TOP MATCHES:"; head -50 /tmp/cv-references.txt
```

Expected: a list of files referencing CV concepts. Save the count for sanity check after slim-down (Task 30).

- [ ] **Step 2: Cross-check against the deletion list**

Compare `/tmp/cv-references.txt` against the "Files deleted" and "Files modified" lists in the File Structure section above. Anything in `/tmp/cv-references.txt` that doesn't appear in either list is a gap — flag it before proceeding to deletion tasks.

```bash
cat /tmp/cv-references.txt
```

Manually inspect output. If unexpected files surface (e.g., a crate not yet covered), note them; they will need additional tasks added before the plan is complete.

### Task 9: Move ADR 0009 to xianvec-play

ADR 0009 (`Qwen3-Next runtime options`) is a CV-spike runtime question. Per spec, it relocates to xianvec-play.

**Files:**
- Create: `/Users/edkennedy/Code/xianvec-play/decisions/0009-qwen3-next-runtime-options.md`
- Delete: `/Users/edkennedy/Code/xianvec/decisions/0009-qwen3-next-runtime-options.md`

- [ ] **Step 1: Copy to xianvec-play**

```bash
cp /Users/edkennedy/Code/xianvec/decisions/0009-qwen3-next-runtime-options.md \
   /Users/edkennedy/Code/xianvec-play/decisions/0009-qwen3-next-runtime-options.md
```

- [ ] **Step 2: Commit in xianvec-play**

```bash
cd /Users/edkennedy/Code/xianvec-play
git add decisions/0009-qwen3-next-runtime-options.md
git commit -m "docs: import ADR 0009 (Qwen3-Next runtime) from xianvec — CV substrate home"
```

- [ ] **Step 3: Delete from xianvec**

```bash
cd /Users/edkennedy/Code/xianvec
git rm decisions/0009-qwen3-next-runtime-options.md
```

- [ ] **Step 4: Commit deletion in xianvec**

```bash
git commit -m "docs(decisions): move ADR 0009 to xianvec-play (CV substrate home)"
```

### Task 10: Delete xianvec-inference crate

**Files:**
- Delete: `crates/xianvec-inference/` (entire directory)

- [ ] **Step 1: Confirm no in-tree dependents besides expected ones**

```bash
cd /Users/edkennedy/Code/xianvec
grep -rln "xianvec-inference\|xianvec_inference" crates/ Cargo.toml --include="*.rs" --include="*.toml" 2>/dev/null
```

Expected dependents (will be cleaned in later tasks):
- `Cargo.toml` (workspace member)
- `crates/xianvec-trader/Cargo.toml` if it depends on -inference
- Any other crate Cargo.toml referencing -inference

If anything outside that expected set surfaces, flag and add a remediation task before proceeding.

- [ ] **Step 2: Delete the crate directory**

```bash
git rm -r crates/xianvec-inference
```

- [ ] **Step 3: Don't build yet** — workspace `Cargo.toml` still references the deleted crate. Build will be re-validated after Task 13.

### Task 11: Delete xianvec-introspect crate

**Files:**
- Delete: `crates/xianvec-introspect/` (entire directory)

- [ ] **Step 1: Confirm no in-tree dependents besides expected**

```bash
grep -rln "xianvec-introspect\|xianvec_introspect" crates/ Cargo.toml --include="*.rs" --include="*.toml" 2>/dev/null
```

Expected: `Cargo.toml` (workspace member); possibly `xianvec-inference/Cargo.toml` (already deleted).

- [ ] **Step 2: Delete**

```bash
git rm -r crates/xianvec-introspect
```

### Task 12: Delete xianvec-gating crate

**Files:**
- Delete: `crates/xianvec-gating/` (entire directory)

- [ ] **Step 1: Confirm dependents**

```bash
grep -rln "xianvec-gating\|xianvec_gating" crates/ Cargo.toml --include="*.rs" --include="*.toml" 2>/dev/null
```

Expected: `Cargo.toml`; possibly `xianvec-inference/Cargo.toml` (deleted).

- [ ] **Step 2: Delete**

```bash
git rm -r crates/xianvec-gating
```

### Task 13: Update workspace Cargo.toml

**Files:**
- Modify: `Cargo.toml` (root)

- [ ] **Step 1: Read current workspace section**

```bash
grep -n "xianvec-inference\|xianvec-introspect\|xianvec-gating" Cargo.toml
```

Expected: 3 lines in `members = [...]` array, 3 lines in `default-members = [...]` array.

- [ ] **Step 2: Edit Cargo.toml — remove deleted crate paths**

Open `Cargo.toml`. In the `members = [...]` array, delete these three lines:

```toml
    "crates/xianvec-inference",
    "crates/xianvec-gating",
    "crates/xianvec-introspect",
```

In the `default-members = [...]` array (if present), delete the same three lines.

- [ ] **Step 3: Validate the workspace builds**

```bash
cargo build --workspace 2>&1 | tail -30
```

Expected: build succeeds OR fails with errors pointing to crates that depend on the deleted ones (likely `xianvec-trader/Cargo.toml`, `xianvec-eval/Cargo.toml`). Note any remaining build errors — they will be addressed in subsequent tasks.

If trader/eval still depend on `xianvec-inference` etc., mark those errors and continue to Task 14 (which addresses Cargo.toml dependencies for those crates).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore(workspace): remove xianvec-inference/-introspect/-gating crates

CV substrate moves to xianvec-play per ADR 0011. The three CV crates
are deleted; workspace members updated."
```

### Task 14: Drop CV-crate dependencies from xianvec-trader

**Files:**
- Modify: `crates/xianvec-trader/Cargo.toml`

- [ ] **Step 1: Inspect current deps**

```bash
cat crates/xianvec-trader/Cargo.toml
```

Look for entries referencing `xianvec-inference`, `xianvec-introspect`, `xianvec-gating`.

- [ ] **Step 2: Edit `crates/xianvec-trader/Cargo.toml`**

Remove any lines like:

```toml
xianvec-inference = { path = "../xianvec-inference" }
xianvec-introspect = { path = "../xianvec-introspect" }
xianvec-gating = { path = "../xianvec-gating" }
```

If `[dependencies]` becomes empty after removal, leave the header and add a comment noting deps are pulled in by sibling crates, OR (if the crate truly needs no deps now) leave the empty header — Cargo accepts that.

- [ ] **Step 3: Inspect trader source for dangling imports**

```bash
grep -rln "xianvec_inference\|xianvec_introspect\|xianvec_gating" crates/xianvec-trader/src/ 2>/dev/null
```

If matches surface: open those files and remove the `use` statements. If the imports were used in actual code (function calls, types), the trader needs a deeper edit — note this as a sub-task and resolve before commit.

- [ ] **Step 4: Build the trader crate**

```bash
cargo build -p xianvec-trader 2>&1 | tail -20
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-trader/Cargo.toml
git commit -m "chore(trader): drop CV-crate dependencies (-inference/-introspect/-gating)"
```

### Task 15: Drop VectorConfig + four-arm TraderArm from xianvec-eval

**Files:**
- Modify: `crates/xianvec-eval/src/baselines/` (the file containing `VectorConfig` + `TraderArm`)
- Modify: `crates/xianvec-eval/src/ab_compare.rs`

- [ ] **Step 1: Locate `VectorConfig` and `TraderArm`**

```bash
grep -rn "enum VectorConfig\|struct TraderArm\|impl TraderArm" crates/xianvec-eval/src/ 2>/dev/null
```

This identifies the file(s) containing the type definitions. Common location: `crates/xianvec-eval/src/baselines/trader_arm.rs` or `crates/xianvec-eval/src/baselines/mod.rs`.

- [ ] **Step 2: Read the file(s)**

Use `Read` on each identified file to understand the current shape.

- [ ] **Step 3: Remove `VectorConfig` enum**

Delete the `VectorConfig` enum definition entirely (typically `enum VectorConfig { Off, On { magnitudes: HashMap<...> }, Random {...}, Orthogonal {...} }` plus impls).

- [ ] **Step 4: Reshape `TraderArm`**

`TraderArm` becomes a `Strategy` impl that takes a neutral briefing and returns a decision via the LLM, no vectors. Drop any field of type `VectorConfig` from the struct. Drop any code path branching on `VectorConfig::On`/`Random`/`Orthogonal` — keep only what was the `Off` branch.

If `TraderArm::new` previously took a `VectorConfig` argument, change the signature to remove it. Update the constructor and any tests in the same file.

- [ ] **Step 5: Update `ab_compare.rs`**

Open `crates/xianvec-eval/src/ab_compare.rs`. Find the construction of the four `ArmConfig`s (the spec identified them at lines 48–102 of the current file).

Replace the four-arm hardcoded list with two arms — one TraderArm (no `VectorConfig`), one a placeholder `Strategy` named e.g. `buy_hold` from baselines. Or, more general: take a list of `Box<dyn Strategy>` from the caller and iterate. The minimum change to compile is: remove `vector: VectorConfig::*` fields from each `ArmConfig`, drop three of the four arms (keeping a single TraderArm and one or more baseline strategies), and rename the surviving arms to non-vector labels (e.g., `"trader_arm"` and `"buy_hold"`).

- [ ] **Step 6: Update test fixtures in the same file**

The spec noted line 179: `assert_eq!(a.name, "vectors_off");`. Update to match the new arm name (e.g., `"trader_arm"`). Update any other assertions that reference `vectors_on`/`vectors_off`/`VectorConfig`.

- [ ] **Step 7: Build and test the eval crate**

```bash
cargo build -p xianvec-eval 2>&1 | tail -20
cargo test -p xianvec-eval --lib 2>&1 | tail -30
```

Expected: build succeeds; tests pass.

If a test fails because a label changed: update the test's expected value to the new label. Don't weaken assertions to side-step the label change.

- [ ] **Step 8: Commit**

```bash
git add crates/xianvec-eval/
git commit -m "feat(eval): drop VectorConfig + four-arm TraderArm

Per ADR 0011, CV is gone. TraderArm becomes a single LLM-without-steering
Strategy. ab_compare arms generalize: one TraderArm + one baseline,
generic strategy-name labels (no vectors_on/vectors_off)."
```

### Task 16: Delete xianvec-cli explain_vectors command

**Files:**
- Delete: `crates/xianvec-cli/src/commands/explain_vectors.rs`
- Modify: `crates/xianvec-cli/src/commands/mod.rs`
- Modify: `crates/xianvec-cli/src/main.rs`

- [ ] **Step 1: Read affected files**

```bash
cat crates/xianvec-cli/src/commands/mod.rs
grep -n "explain_vectors\|explain-vectors\|ExplainVectors" crates/xianvec-cli/src/main.rs
```

- [ ] **Step 2: Delete the command file**

```bash
git rm crates/xianvec-cli/src/commands/explain_vectors.rs
```

- [ ] **Step 3: Edit `crates/xianvec-cli/src/commands/mod.rs`**

Remove the line `pub mod explain_vectors;` (and any re-export of its types).

- [ ] **Step 4: Edit `crates/xianvec-cli/src/main.rs`**

Remove the `ExplainVectors` variant from the `clap` enum (likely a `#[derive(Subcommand)]` enum). Remove the match arm in the dispatch (`Commands::ExplainVectors(...) => ...`). Remove any `use` statement importing `explain_vectors::*`.

- [ ] **Step 5: Build the CLI**

```bash
cargo build -p xianvec-cli 2>&1 | tail -20
```

Expected: success.

- [ ] **Step 6: Smoke-test the CLI**

```bash
cargo run -p xianvec-cli --bin xvn -- --help 2>&1 | head -40
```

Expected: help output no longer lists `explain-vectors`.

- [ ] **Step 7: Commit**

```bash
git add crates/xianvec-cli/
git commit -m "feat(cli): drop explain-vectors subcommand (CV removed per ADR 0011)"
```

### Task 17: Rename hardcoded vectors_on/vectors_off labels in CLI

**Files:**
- Modify: `crates/xianvec-cli/src/commands/show_metrics.rs`
- Modify: `crates/xianvec-cli/src/commands/report.rs`
- Modify: `crates/xianvec-cli/src/commands/show_decision.rs`

- [ ] **Step 1: Inspect current usage**

```bash
grep -n "vectors_on\|vectors_off" crates/xianvec-cli/src/commands/*.rs
```

Reference these lines from the spec's pre-flight scan:
- `show_metrics.rs:47, 49` — uses `"vectors_off"` as default arm name
- `report.rs:32, 34, 51, 53, 82` — uses both labels in markdown output and tests
- `show_decision.rs:65, 73` — uses `"vectors_on"` as test fixture

- [ ] **Step 2: Edit `show_metrics.rs`**

Replace literal `"vectors_off"` with `"trader_arm"` (matching the new ab_compare default). Keep the variable name and surrounding logic unchanged.

- [ ] **Step 3: Edit `report.rs`**

Replace literal `"vectors_off"` → `"trader_arm"` and `"vectors_on"` → `"buy_hold"` (or the second arm name chosen in Task 15). Update the test assertion at line 82 (`assert!(md.contains("vectors_on"));`) to match the new label.

- [ ] **Step 4: Edit `show_decision.rs`**

Replace literal `"vectors_on"` test fixture (lines 65, 73) with `"trader_arm"`. Update both the `insert_decision` call and the `assert_eq!` check.

- [ ] **Step 5: Build and test the CLI**

```bash
cargo build -p xianvec-cli 2>&1 | tail -10
cargo test -p xianvec-cli --lib 2>&1 | tail -20
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-cli/
git commit -m "refactor(cli): rename vectors_on/vectors_off labels to strategy names

Hardcoded arm names in show_metrics, report, show_decision tests
no longer reference CV. Labels now reflect Strategy names per
ADR 0011 ab_compare reshape."
```

### Task 18: Drop VectorConfigSummary from xianvec-identity

**Files:**
- Modify: `crates/xianvec-identity/src/manifest.rs`
- Modify: `crates/xianvec-identity/src/lib.rs`
- Delete: `identity/vectors_off.agent.json`
- Delete: `identity/vectors_on.agent.json`

- [ ] **Step 1: Inspect current shape**

```bash
cat crates/xianvec-identity/src/manifest.rs | head -120
grep -n "VectorConfigSummary\|vector_config" crates/xianvec-identity/src/
```

- [ ] **Step 2: Edit `crates/xianvec-identity/src/manifest.rs`**

Remove:
- `pub struct VectorConfigSummary { ... }` (around line 41+ per the pre-flight scan).
- `pub vector_config: VectorConfigSummary,` field on `AgentManifest` (around line 30).
- Any constructors or test fixtures setting `vector_config` (around line 94).

Expected new `AgentManifest` shape (illustrative — adapt to the actual struct):

```rust
pub struct AgentManifest {
    pub agent_id: u64,
    pub strategy_name: String,
    pub code_commit: String,
    pub strategy_adapter_type: String,
    pub risk_preset: RiskPreset,
}
```

If the existing struct already has analogous fields, the change is purely subtractive.

- [ ] **Step 3: Edit `crates/xianvec-identity/src/lib.rs`**

Remove the doc-comment lines (around lines 10–11 per the pre-flight scan) referencing `vectors_off.agent.json` and `vectors_on.agent.json` in the module-level documentation.

Update the `pub use` line (around line 40) to drop `VectorConfigSummary`:

Before:
```rust
pub use manifest::{AgentManifest, ReputationEntry, TradeOutcome, VectorConfigSummary};
```

After:
```rust
pub use manifest::{AgentManifest, ReputationEntry, TradeOutcome};
```

- [ ] **Step 4: Delete the per-config manifest fixtures**

```bash
git rm identity/vectors_off.agent.json identity/vectors_on.agent.json
```

- [ ] **Step 5: Build and test identity crate**

```bash
cargo build -p xianvec-identity 2>&1 | tail -20
cargo test -p xianvec-identity --lib 2>&1 | tail -30
```

Expected: build succeeds. Tests may fail if they referenced `VectorConfigSummary` or the deleted JSON fixtures — fix by removing the relevant test cases or migrating them to use the simplified manifest.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-identity/ identity/
git commit -m "feat(identity): drop VectorConfigSummary + per-config manifests

AgentManifest no longer carries vector_config. The two
vectors_off.agent.json / vectors_on.agent.json fixtures are deleted.
Per ADR 0011, manifests track strategy name + adapter type instead."
```

### Task 19: Delete CV tooling, data, and notebook

**Files:**
- Delete: `tools/extract_vectors/`
- Delete: `data/vectors/`
- Delete: `notebooks/inspect_vector.py`

- [ ] **Step 1: Confirm no in-tree references to these paths**

```bash
grep -rln "extract_vectors\|data/vectors\|inspect_vector" \
    crates/ scripts/ Cargo.toml \
    --include="*.rs" --include="*.toml" --include="*.sh" 2>/dev/null
```

Expected references: `MANUAL.md`, `scripts/setup_runpod.sh`, `architecture.md` — these are docs/scripts that will be reconciled in later tasks. No live Rust code should reference them.

- [ ] **Step 2: Delete**

```bash
git rm -r tools/extract_vectors/ data/vectors/ notebooks/inspect_vector.py
```

If `tools/` or `data/` are now empty, leave them (other tooling may land later) or remove them — operator preference. The repo currently has no `.gitkeep`-style files there, so empty dirs disappear naturally.

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: delete CV tooling, FAISS indexes, vector notebook

tools/extract_vectors/ (Python repeng), data/vectors/ (FAISS indexes
+ manifests), and notebooks/inspect_vector.py move to xianvec-play
via the Phase 1 history merge. Per ADR 0011."
```

### Task 20: Build + test full workspace

**Files:**
- (None modified; verification gate)

Validation gate before doc reconciliation. The workspace must be green at this point — all CV code is gone, all CV-coupled crates have been updated.

- [ ] **Step 1: Clean build**

```bash
cargo clean
cargo build --workspace 2>&1 | tail -30
```

Expected: green build. If errors surface, do not proceed to doc tasks until they're resolved.

- [ ] **Step 2: Full test run**

```bash
cargo test --workspace 2>&1 | tail -40
```

Expected: green. Pre-existing flakes that carry over from `main` are acceptable; new failures introduced by these tasks are not.

- [ ] **Step 3: CLI smoke**

```bash
cargo run -p xianvec-cli --bin xvn -- --help 2>&1 | head -40
```

Expected: help output, no `explain-vectors`. Any subcommands related to other CV concepts (if missed): flag and add a remediation task.

- [ ] **Step 4: Confirm grep is clean for code paths**

```bash
grep -rln "VectorConfig\|vectors_on\|vectors_off\|vector_config\|active_vectors\|FAISS\|faiss-rs\|repeng" \
    crates/ Cargo.toml --include="*.rs" --include="*.toml" 2>/dev/null
```

Expected: empty output. If anything surfaces in `crates/` or `Cargo.toml`, address before continuing.

If green: proceed to doc reconciliation.

### Task 21: Delete steering-vector-architecture.md and python archive

**Files:**
- Delete: `steering-vector-architecture.md`
- Delete: `implementation-plan-python-archive.md`

- [ ] **Step 1: Confirm what's there**

```bash
head -10 steering-vector-architecture.md
head -10 implementation-plan-python-archive.md
```

These are CV-specific design / planning docs that are now redundant (CV substrate lives in xianvec-play with full history; the docs are preserved there).

- [ ] **Step 2: Delete**

```bash
git rm steering-vector-architecture.md implementation-plan-python-archive.md
```

- [ ] **Step 3: Commit**

```bash
git commit -m "docs: delete CV design docs (preserved in xianvec-play)"
```

### Task 22: Architecture.md — delete §7 (Control vector strategy)

**Files:**
- Modify: `architecture.md` (delete lines 319–440)

§7 spans lines 319 (heading `## 7. Control vector strategy`) through line 440 (last line before §8). Total ~122 lines deleted.

- [ ] **Step 1: Capture exact byte range**

```bash
grep -n "^## 7\|^## 8" architecture.md
```

Expected: line 319 (`## 7. ...`) and line 441 (`## 8. ...`). Delete everything from 319 inclusive through 440 inclusive (line 441's `## 8.` is the next surviving heading).

- [ ] **Step 2: Delete the section**

Use the editor to delete lines 319–440 inclusive. The blank line above `## 8.` should remain (one blank line of separation).

If your editor doesn't make line-range deletion easy:

```bash
# sed -i is darwin-specific; use the in-place form:
sed -i '' '319,440d' architecture.md
```

- [ ] **Step 3: Verify**

```bash
grep -n "^## " architecture.md
wc -l architecture.md
```

Expected: section list now goes 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13 (no 7). Total lines drop by ~122.

- [ ] **Step 4: Commit**

```bash
git add architecture.md
git commit -m "docs(architecture): delete §7 (Control vector strategy)

CV substrate moved to xianvec-play per ADR 0011. §7 was the
single largest CV-specific section; the rest of architecture.md
will be revised in subsequent commits."
```

### Task 23: Architecture.md — rewrite §1 (Thesis)

**Files:**
- Modify: `architecture.md` lines ~7–17 (current §1 Thesis block)

The original thesis is CV-driven ("control vectors are the mechanism to encode dispositional knowledge..."). Rewrite around multistrategy + marketplace.

- [ ] **Step 1: Read current §1**

```bash
sed -n '5,20p' architecture.md
```

- [ ] **Step 2: Replace §1**

Find the section starting at `## 1. Thesis` (line 7) through the last line before `## 2. System overview` (line 18). Replace with:

```markdown
## 1. Thesis

A multistrategy population evaluated through a deterministic loom, with
on-chain reputation and validation receipts via ERC-8004, produces a
credibly auditable ranking of trading strategy variants. The system is a
*marketplace* in shape — strategies have provenance, performance history,
and fork lineage as first-class on-chain artifacts.

The hackathon claim is narrower than the long-term thesis. We are not yet
claiming the loom can self-improve indefinitely (that's the deferred
Karpathy autoresearch direction). The hackathon claim is:

> On a fixed set of trading setups, a population of N strategies (classical
> TA + onchain + LLM-driven) evaluated through the loom produces an
> on-chain ranking that distinguishes strategies beyond noise on a
> pre-committed risk-adjusted return metric, with reputation and validation
> receipts visible on Mantle.

Everything in this document is in service of evaluating that claim cleanly.
```

- [ ] **Step 3: Verify**

```bash
sed -n '5,30p' architecture.md
```

Expected: §1 is the new thesis; §2 starts immediately after.

- [ ] **Step 4: Commit**

```bash
git add architecture.md
git commit -m "docs(architecture): rewrite §1 thesis around multistrategy + marketplace"
```

### Task 24: Architecture.md — revise §4 Stage 2 Trader

**Files:**
- Modify: `architecture.md` (current §4 Stage 2 Trader, ~lines 188–224)

§4 currently describes Stage 2 as "vectors active." Without vectors, Stage 2 is a vanilla LLM call.

- [ ] **Step 1: Read current §4**

```bash
sed -n '188,225p' architecture.md
```

- [ ] **Step 2: Edit the section**

Replace the current §4 content with:

```markdown
## 4. Stage 2 — Trader

**Purpose:** Make the final trading decision based on the Intern's neutral
evidence briefing. Stage 2 is one Strategy variant in the loom (LLM judgment
on balanced inputs); other strategies in the loom are classical TA, onchain
signal, or hybrid implementations.

**Naming:** "Trader" is the characterological role — the Intern researches
neutrally, the Trader decides. Both stages are LLM-backed; only the Trader
emits the final action.

**Model choice:** Backend-agnostic, picked at runtime via config (same
`InternBackend`-style trait used by Stage 1, OR a separate `TraderBackend`
if their evolution diverges). OpenAI-compatible HTTP is the default
(covers OpenAI, Anthropic, OpenRouter, vLLM, llama.cpp, Ollama). Local
candle inference is an optional path for fully air-gapped runs.

**Inference path:**
1. Receive Intern Briefing JSON.
2. Render briefing as a prompt requesting a structured decision. Prompt
   presents bull/bear/flat cases in parallel structure with no anchored
   recommendation.
3. Call the configured backend.
4. Parse output as JSON via `serde_json` with `garde` validation; on parse
   failure, retry once with a corrective system message before falling back
   to a parse-error path.

**Output (JSON):**

```json
{
  "setup_id": "uuid",
  "action": "buy | sell | flat | close",
  "size_bps": 75,
  "direction": "long | short | flat",
  "stop_loss_pct": 2.5,
  "take_profit_pct": 5.0,
  "trader_summary": "string — one-line rationale"
}
```

The Trader is wrapped as a `Strategy` adapter (`TraderArm`) so it competes
in the loom against classical TA + onchain strategies on equal terms.
```

- [ ] **Step 3: Verify**

```bash
grep -n "vectors\|VectorConfig\|active_vectors\|candle hooks" architecture.md
```

Expected: no matches inside §4 (other matches in earlier sections will be addressed in later tasks).

- [ ] **Step 4: Commit**

```bash
git add architecture.md
git commit -m "docs(architecture): rewrite §4 Trader without vector references"
```

### Task 25: Architecture.md — revise §6.1 (ERC-8004 receipts)

**Files:**
- Modify: `architecture.md` §6.1 subsection

§6.1 currently includes `active_vector_alphas` and `vector_manifest_hash` in the Validation Registry receipt schema. Both fields are gone with CV.

- [ ] **Step 1: Locate §6.1**

```bash
grep -n "^### 6\.1\|^## 7" architecture.md
```

Expected: §6.1 starts at some line, §7 starts at line 319 (or now: deleted, with §8 the next heading). The section ends at §7's start.

- [ ] **Step 2: Read the receipt schema block**

```bash
grep -n "active_vector_alphas\|vector_manifest_hash\|trade_result_hash" architecture.md
```

- [ ] **Step 3: Edit the JSON schema in §6.1**

Replace the receipt JSON block:

Before (illustrative):
```json
{
  "setup_id": "uuid",
  "action": "buy | sell | flat | close",
  "active_vector_alphas": { "conviction": 0.8 },
  "vector_manifest_hash": "0x...",
  "vectors_enabled": true,
  "trade_result_hash": "keccak256(closed_pnl | timestamp | price)",
  "run_id": "uuid"
}
```

After:
```json
{
  "setup_id": "uuid",
  "action": "buy | sell | flat | close",
  "strategy_id": 42,
  "strategy_name": "trader_arm",
  "trade_result_hash": "keccak256(closed_pnl | timestamp | price)",
  "run_id": "uuid"
}
```

Update surrounding prose: drop sentences explaining `active_vector_alphas` size budget, drop the `vectors_enabled` falsification-control language. Replace with prose explaining `strategy_id` references the per-strategy NFT and `strategy_name` is the human-readable label.

The §6.1 prose paragraph that begins "active_vector_alphas is one float in v1..." (around 4 bytes) is replaced with: "strategy_id is the agent NFT token ID = 8 bytes; strategy_name is the readable label preserved off-chain. The proof is cheap to post and gives anyone the ability to verify on-chain that a specific strategy produced a specific trade."

The §6.1 "On-chain footprint summary" table: drop the row for "Full control vectors..." and the row for "active_vector_alphas + manifest hash per trade." Replace the latter with `strategy_id + receipt fields per trade ~32 bytes`.

- [ ] **Step 4: Verify**

```bash
grep -n "active_vector\|vector_manifest\|vectors_enabled" architecture.md
```

Expected: empty.

- [ ] **Step 5: Commit**

```bash
git add architecture.md
git commit -m "docs(architecture): drop CV fields from §6.1 ERC-8004 receipts

Validation Registry receipts now reference strategy_id instead of
active_vector_alphas + vector_manifest_hash."
```

### Task 26: Architecture.md — revise §9, §10, §11, §12, §13

These five sections all reference CV throughout. Combined into one task with sub-steps because each edit is small.

**Files:**
- Modify: `architecture.md` (§9 eval, §10 tech stack, §11 out of scope, §12 Q&A table, §13 references)

- [ ] **Step 1: §9 — Eval framework**

```bash
grep -n "vectors-on\|vectors-off\|vectors_on\|vectors_off\|VectorConfig\|control vector" architecture.md | head -30
```

In §9, find references to "vectors-on minus vectors-off" framing. Replace with "Strategy A minus Strategy B" generalization.

In §9.3 Baselines, delete the entire "Experimental controls (the thesis-defining comparisons)" subsection (4 bullets: vectors OFF / random / orthogonal / same agent). The null + classical + onchain baselines survive intact.

In §9.4 Structured traces, delete `xianvec.vectors.*` and `xianvec.gating.*` from the OTel attribute list (if these are mentioned outside §7 — check).

- [ ] **Step 2: §10 — Tech stack**

Locate the "Control vectors" sub-bullet (around `Storage: FAISS-compatible .index files via faiss-rs...`). Delete the entire `Control vectors:` block (3-4 lines covering Extraction, Storage, Application, Gating).

In the "Inference" sub-bullet, drop the line about hidden-state hooks needed for steering.

In "Introspection (opt-in, per §7.5.1)" sub-block: delete entirely.

In the "Tracing & observability" sub-bullet, drop the line about Python extractor emitting OTel spans.

- [ ] **Step 3: §10.1 Cargo workspace layout**

In the workspace ASCII tree, delete the three lines:

```
│   ├── xianvec-inference/        # candle wrapper + steering hooks + inline FAISS load
│   ├── xianvec-gating/           # entropy gating, alpha schedule
│   ├── xianvec-introspect/       # OPTIONAL layer analytics (Phase 0.3 spike requires)
```

In `tools/`, delete:
```
│   └── extract_vectors/          # Python: repeng-based contrast extractor
```

In `data/`, delete:
```
│   └── vectors/                  # FAISS .index files + manifests
```

- [ ] **Step 4: §10.2 — Lodestar boundary**

Delete §10.2 entirely (it described an extraction boundary for CV substrate that no longer exists).

- [ ] **Step 5: §11 — Out of scope**

Delete CV-specific items:
- "Karpathy self-improvement loop (vector training from agent's own trades)" → reframe as "Karpathy autoresearch loop (LLM-proposed strategy mutations from per-strategy trade ledgers)" — the line stays but the framing shifts to strategy-level.

Delete the entire "v1 scope cuts (added 2026-05-03)" block referencing:
- "lodestar / xianvec subtree split"
- "3 of 4 disposition axes active — v1 ships **Conviction only**"
- "Regime-conditioned vector configs (§7.4 hand-set magnitudes per regime)"
- "Async vector substrate as a separate crate"
- "Full contract layer crate with `Vector<L, M>` generics"
- "Geometry crate with first-class corridor abstractions"

The "Multi-asset basket — v1 is BTC only", "xStocks", "Telegram demo bot", "mantle-risk-evaluator LLM pre-flight gate" items survive (unrelated to CV).

- [ ] **Step 6: §12 — Open architectural questions resolved**

Delete the rows:
- "Confidence gating?"
- "Local model for Stage 2?"
- "Implementation language?" (or rewrite — it justifies Rust on the basis of CV runtime; reword to drop CV)
- "Vector extraction language?"
- "Inference framework?" (CV motivation)
- "Vector file format?"
- "Adopt Glamin directly?"
- "Active disposition axes in v1?"

Add a new row at the top:

| Question | Resolution |
|---|---|
| CV substrate location? | **Moved to xianvec-play** per ADR 0011. xianvec is multistrategy + marketplace; CV research continues in xianvec-play with the full development trail preserved. |

- [ ] **Step 7: §13 — References**

Delete CV-specific reference blocks:
- "Steering Vector Fields (SVF)"
- "SEAL — reasoning steering"
- "Mitra. Activation Steering in 2026"
- "Steer2Adapt"
- "From Steering Vectors to Conceptors"
- "Reliable Control-Point Selection"
- "Geometric / corridor framing inspiration: Glamin"
- "Vector extraction (Python, offline)" entire block (repeng, dialz, transformers)
- "ERC-8004" block: keep
- "Rust substrate" block: drop `faiss-rs` only

The remaining references (apca, alloy, ta, ERC-8004, observability, Rust substrate minus faiss) survive.

- [ ] **Step 8: Verify whole-file CV-cleanliness**

```bash
grep -in "vector\|FAISS\|repeng\|steering\|control[ -]vector\|introspect" architecture.md
```

Expected: only matches inside ADR-cross-references (e.g., "ADR 0011" mentions). If substantive prose still references CV, edit until clean.

- [ ] **Step 9: Build doesn't depend on this** — but spot-check the doc renders.

Open `architecture.md` in a markdown previewer if available; check that section numbering still reads cleanly.

- [ ] **Step 10: Commit**

```bash
git add architecture.md
git commit -m "docs(architecture): scrub CV from §9 (eval), §10 (stack), §11 (oos), §12 (Q&A), §13 (refs)

ADR 0011 cleanup. Strategy-level reframe of Karpathy autoresearch.
faiss-rs / repeng / SEAL / SVF / Glamin references removed."
```

### Task 27: Regenerate architecture-diagram.mermaid

**Files:**
- Modify: `architecture-diagram.mermaid`

The diagram has yellow `vectorOn` blocks (S2 Trader, CV) that need recoloring + the CV node deleted.

- [ ] **Step 1: Read current diagram**

```bash
cat architecture-diagram.mermaid | head -100
```

- [ ] **Step 2: Edit**

- Delete the `CV[<b>Control Vectors</b>...]` node definition.
- Delete the edge `CV -.->|injected at<br/>mid-late layers| S2`.
- In the `S2[...]` definition, drop `VECTORS ACTIVE` from its label.
- In the class definitions, remove `class S2,CV vectorOn`. The `S2` node now uses the default class (orange / orchestrator) or a new class — operator preference. Recommend: `class S2 orchestrator` or its own neutral class.
- Drop the `classDef vectorOn ...` line if no other nodes use it.

- [ ] **Step 3: Render verification (optional)**

If `mermaid-cli` is installed, render to confirm no syntax errors:

```bash
which mmdc 2>/dev/null && mmdc -i architecture-diagram.mermaid -o /tmp/arch.svg 2>&1 | tail -5
```

If mermaid-cli is not installed, skip — review the source manually for mismatched braces / arrows.

- [ ] **Step 4: Update the inline mermaid block in architecture.md §2.1**

`architecture.md` §2.1 (around lines 59–141 in the original file) embeds the same diagram inline. Update the inline copy to match the standalone file. (Or: delete the inline copy and reference the file.)

- [ ] **Step 5: Commit**

```bash
git add architecture-diagram.mermaid architecture.md
git commit -m "docs(architecture): regenerate diagram without CV nodes"
```

### Task 28: Reconcile FOLLOWUPS.md

**Files:**
- Modify: `FOLLOWUPS.md`

- [ ] **Step 1: Read current state**

```bash
cat FOLLOWUPS.md | head -50
```

The file has track classification table (SLF / CVF / Shared) and per-track queues.

- [ ] **Step 2: Edit the track classification table**

Delete the CVF row entirely. The table becomes:

```
| Track | Items | Lives on |
|---|---|---|
| **SLF — Strategy Loom** | new SLF1–16 (below); supersedes F4, F14, F15, F17, F23 | `main` (post-merge of pivot/cv-extract) |
| **Shared** | F5, F6, F7, F8, F18, F19, F20, F21 (landed partial), F22, F24, F25 | `main` |
```

Update the navigation links at the top: drop the CVF link.

- [ ] **Step 3: Replace the CVF queue section**

Find the entire `## Control Vector queue (CVF)` section (or however it's titled in the doc — `grep -n "^## " FOLLOWUPS.md` to find the header).

Replace it with a brief epitaph:

```markdown
## Control Vector queue — closed (2026-05-07)

Per ADR 0011, the CV substrate moved to xianvec-play. The following CVF
items are closed in xianvec; their live state continues in xianvec-play if
applicable: F1, F2, F3 (partial — TraderArm survives without VectorConfig),
F9, F10, F11, F12, F13, F16, F26, F27, F28, F29, F30, F31, F32.

See `decisions/0011-cv-extraction.md`.
```

- [ ] **Step 4: Update SLF3**

Find SLF3 ("Mint per-strategy NFT on `ab_compare` startup").

Edit the "Decision" line:

Before:
> **Decision:** each `VectorConfig` mode of TraderArm gets its own NFT (TraderArm-Off, -On, -Random, -Orth = four NFTs). The leaderboard view needs them as separate units.

After:
> **Decision:** TraderArm gets one NFT (vectors-on/off/random/orth no longer apply post-ADR-0011). The leaderboard view treats it as a single unit alongside other Strategy implementations.

- [ ] **Step 5: Update F3 status**

If F3 is listed elsewhere (likely in a "landed" section), append a note:

> **2026-05-07 update:** Vectors-on / random / orthogonal arms removed per ADR 0011. The TraderArm `Strategy` adapter survives as LLM-without-steering.

- [ ] **Step 6: Commit**

```bash
git add FOLLOWUPS.md
git commit -m "docs(followups): close CVF queue, simplify SLF3, update F3 (ADR 0011)"
```

### Task 29: Reconcile implementation-plan.md

**Files:**
- Modify: `implementation-plan.md`

- [ ] **Step 1: Survey the structure**

```bash
grep -n "^## \|^# " implementation-plan.md | head -50
```

Identify the phase structure. Likely Phase 0–N with CV-dense phases (0 spike, 4 vector ops, 8 probes per the spec).

- [ ] **Step 2: Delete CV-dense phases**

For each CV-dense phase (per the spec: Phase 0 spike validation, Phase 4 vector ops / steering hook installation, Phase 8 probe runner with introspection): delete the phase's entire section.

Use `sed -n '<start>,<end>p'` to inspect ranges, then editor or `sed -i ''` to delete.

- [ ] **Step 3: Generalize remaining CV references**

In phases that survive but mention vectors: edit to remove vector-specific framing. Phase 9 (eval) per the spec: vectors-on/off arms removed, generalized to multi-strategy.

- [ ] **Step 4: Add a header note at the top**

Add immediately after the document title:

```markdown
> **2026-05-07: Plan reshaped per ADR 0011.** Original CV-driven phases
> (Phase 0 spike, Phase 4 vector ops, Phase 8 probe runner with
> introspection) have been removed. Strategy Loom + ERC-8004 marketplace
> work continues per the SLF queue in FOLLOWUPS.md and the Strategy
> Loom phase structure documented below.
```

- [ ] **Step 5: Verify**

```bash
grep -in "vector\|FAISS\|repeng\|steering\|introspect" implementation-plan.md
```

Expected: only ADR-cross-reference matches (mentions of "ADR 0011" or "CV extraction"). If substantive plan content still references CV, edit.

- [ ] **Step 6: Commit**

```bash
git add implementation-plan.md
git commit -m "docs(plan): drop CV phases, add ADR 0011 reshape note"
```

### Task 30: Reconcile decisions/0001 and decisions/0010

**Files:**
- Modify: `decisions/0001-inference-backend.md`
- Modify: `decisions/0010-hackathon-pivot-strategy-loom.md`

- [ ] **Step 1: Edit ADR 0001**

```bash
grep -n "vector\|hidden state\|steering\|FAISS\|repeng" decisions/0001-inference-backend.md
```

Identify CV-driven justifications. Edit:

- Sections motivating candle on the basis of "hidden-state hooks needed for steering" → reword: candle is retained as a local-inference option for the Trader. Steering-hook flexibility is no longer the primary justification.
- Drop references to FAISS / repeng if any.
- Add a header note at the top:

```markdown
> **2026-05-07 revision:** Per ADR 0011, CV substrate moved to xianvec-play.
> This ADR retains candle as a local-inference option for the Trader, but
> steering-hook flexibility is no longer the load-bearing justification.
```

- [ ] **Step 2: Edit ADR 0010**

Add a header note at the top of `decisions/0010-hackathon-pivot-strategy-loom.md`:

```markdown
> **2026-05-07 partial supersession (ADR 0011):** the `--features control-vectors`
> cargo gate described below is obsolete — CV substrate moved to xianvec-play
> entirely. TraderArm survives without `VectorConfig`. Strategy Loom +
> ERC-8004 Marketplace + Karpathy autoresearch framing are otherwise
> unchanged.
```

Do not delete the body — ADR 0010 stays as historical record.

- [ ] **Step 3: Commit**

```bash
git add decisions/0001-inference-backend.md decisions/0010-hackathon-pivot-strategy-loom.md
git commit -m "docs(decisions): ADR 0001 + 0010 revision notes per ADR 0011"
```

### Task 31: Reconcile MANUAL.md, v1-build-steps.md, scripts/setup_runpod.sh

**Files:**
- Modify: `MANUAL.md`
- Modify: `v1-build-steps.md`
- Modify: `scripts/setup_runpod.sh`

- [ ] **Step 1: Edit MANUAL.md**

```bash
grep -n "vector\|FAISS\|repeng\|extract_vectors\|inspect_vector" MANUAL.md
```

For each match: delete the relevant operator instruction. Common targets:
- Vector extraction setup (Python venv + repeng install)
- FAISS index management
- Layer introspection / `inspect_vector.py` usage
- `xvn explain-vectors` examples

If a section becomes empty after deletions, remove its header too.

- [ ] **Step 2: Edit v1-build-steps.md**

```bash
grep -n "vector\|FAISS\|repeng\|extract_vectors" v1-build-steps.md
```

Drop CV-specific build steps. Add a brief header note: `> 2026-05-07: CV build steps removed per ADR 0011.`

- [ ] **Step 3: Edit scripts/setup_runpod.sh**

```bash
grep -n "FP16\|repeng\|FAISS\|extract_vectors\|huggingface.*Qwen.*[Bb]f16\|model_size_gb=64" scripts/setup_runpod.sh
```

Delete lines that:
- Download FP16 weights for vector extraction (~64GB)
- Install `repeng` Python package
- Install FAISS Python bindings
- Set up the `tools/extract_vectors/` Python venv

The script may become much shorter or even mostly redundant with the rest of the build. Add a header comment:

```bash
# Updated 2026-05-07 per ADR 0011: CV setup removed. RunPod is now
# only needed if the Trader runs local candle inference.
```

- [ ] **Step 4: Commit**

```bash
git add MANUAL.md v1-build-steps.md scripts/setup_runpod.sh
git commit -m "docs(ops): reconcile operator docs + RunPod script (ADR 0011)"
```

### Task 32: Final whole-tree CV-cleanliness check

**Files:**
- (None modified; verification gate)

- [ ] **Step 1: Whole-tree grep**

```bash
cd /Users/edkennedy/Code/xianvec
grep -rin "VectorConfig\|vectors_on\|vectors_off\|active_vector\|FAISS\|faiss-rs\|faiss_rs\|repeng\|steering[ -]vector\|control[ -]vector\|extract_vectors\|inspect_vector\|VectorConfigSummary\|--features control-vectors" \
    . \
    --include="*.rs" --include="*.toml" --include="*.md" --include="*.json" --include="*.py" --include="*.sh" --include="*.mermaid" \
    --exclude-dir=target --exclude-dir=.git --exclude-dir=node_modules \
    2>/dev/null
```

Expected matches (acceptable):
- `decisions/0011-cv-extraction.md` (the ADR itself)
- `decisions/0010-hackathon-pivot-strategy-loom.md` (historical record + revision note)
- `decisions/0001-inference-backend.md` (revision note)
- `docs/superpowers/specs/2026-05-07-cv-extraction-design.md` (the spec)
- `docs/superpowers/plans/2026-05-07-cv-extraction.md` (this plan)
- `FOLLOWUPS.md` (CVF closed-queue epitaph)
- `implementation-plan.md` (reshape note)
- `MANUAL.md`, `v1-build-steps.md` (revision notes)

NOT acceptable: any match in `crates/`, `tools/`, `scripts/setup_runpod.sh` substantive lines, `architecture.md` outside ADR cross-refs, or `architecture-diagram.mermaid`.

If unexpected matches surface: open the file and reconcile. Do not proceed to Task 33 until clean.

- [ ] **Step 2: Final cargo build + test**

```bash
cargo clean
cargo build --workspace 2>&1 | tail -20
cargo test --workspace 2>&1 | tail -30
```

Expected: green build, green tests.

- [ ] **Step 3: CLI sanity**

```bash
cargo run -p xianvec-cli --bin xvn -- --help 2>&1 | head -40
```

Expected: help reads cleanly with no CV subcommands.

### Task 33: Push pivot/cv-extract and open PR

**Files:**
- Push: `origin/pivot/cv-extract`

- [ ] **Step 1: Push the branch**

```bash
git push origin pivot/cv-extract
```

- [ ] **Step 2: Open PR via gh**

```bash
gh pr create --base main --head pivot/cv-extract \
    --title "ADR 0011: CV extraction (xianvec → xianvec-play)" \
    --body "$(cat <<'EOF'
## Summary
- Phase 1 (xianvec-play, separate repo): full-history merge of xianvec/main to host CV substrate
- Phase 2 (this branch): remove CV crates (-inference / -introspect / -gating), tools/extract_vectors, data/vectors, vector notebooks, and identity manifests; modify trader/eval/cli/identity to drop VectorConfig; reconcile architecture.md, FOLLOWUPS, ADRs, operator docs

Spec: docs/superpowers/specs/2026-05-07-cv-extraction-design.md
ADR: decisions/0011-cv-extraction.md
Plan: docs/superpowers/plans/2026-05-07-cv-extraction.md

## Test plan
- [ ] `cargo build --workspace` is green
- [ ] `cargo test --workspace` is green
- [ ] `xvn --help` lists no CV subcommands
- [ ] Whole-tree grep for CV terms matches only ADRs + docs (Task 32 pass)
- [ ] xianvec-play has full xianvec history merged (Phase 1 verified)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Capture the PR URL**

The `gh pr create` command outputs the PR URL. Note it for the merge step.

### Task 34: Merge pivot/cv-extract to main and delete hackathon/turing

**Files:**
- Update: `main` (merge commit)
- Delete: `hackathon/turing` branch (local + remote)

After PR review (operator decision; CI must be green):

- [ ] **Step 1: Merge the PR**

Use `gh pr merge` or merge via GitHub UI. Operator preference. Default: squash or merge commit per repo convention; ADR 0010 was a merge commit, suggesting that's the repo's pattern.

```bash
gh pr merge --merge   # or --squash; operator's call
```

- [ ] **Step 2: Switch to main and pull**

```bash
git checkout main
git pull
```

- [ ] **Step 3: Delete pivot/cv-extract locally**

```bash
git branch -d pivot/cv-extract
```

(Use `-d` not `-D` — if the branch isn't fully merged, fix that before deleting.)

- [ ] **Step 4: Delete hackathon/turing locally and remotely**

```bash
git branch -d hackathon/turing
git push origin --delete hackathon/turing
```

Per ADR 0011, the branch's premise (CV stays under feature flag) is obsolete; `main` is now the hackathon submission surface.

- [ ] **Step 5: Verify final state**

```bash
git branch -a
git log --oneline -5
```

Expected: branches `main`, `phase-0-1`, `origin/main`, `origin/phase-0-1`. No `hackathon/turing`. Latest commit on `main` is the merge of the pivot branch.

- [ ] **Step 6: Sanity rebuild**

```bash
cargo clean
cargo build --workspace 2>&1 | tail -10
cargo test --workspace 2>&1 | tail -20
```

Expected: green.

Phase 2 complete. xianvec is CV-free. Hackathon work resumes from `main`.

---

## Self-Review

**Spec coverage:**
- Phase 1 (full-history merge): Tasks 1–6 ✓
- Phase 2a code removed (3 crates + tooling + data + notebooks + identity manifests + steering arch doc + python archive): Tasks 9, 10, 11, 12, 13, 19, 21 ✓
- Phase 2b code modified (eval, cli, identity, trader Cargo.toml): Tasks 14, 15, 16, 17, 18 ✓
- Phase 2c doc reconciliation (architecture, diagram, FOLLOWUPS, implementation-plan, ADR 0001, ADR 0010, MANUAL, v1-build-steps, setup_runpod): Tasks 22, 23, 24, 25, 26, 27, 28, 29, 30, 31 ✓
- Phase 2d branch reconciliation (pivot/cv-extract creation, merge, hackathon/turing deletion): Tasks 7, 33, 34 ✓
- Spec's "Risks / open questions": pre-flight scan addresses lurking CV deps (Task 8), final whole-tree check (Task 32). Doc-rewrite scope handled by per-doc tasks. Identity manifest schema migration: Task 18 covers the code change; if persistent prod manifests exist, that's surfaced as a sub-flag in Task 18 step 5.
- Future re-integration sketch: not implemented (it's a future possibility, not a deliverable).

**Placeholder scan:** No `TBD`, `TODO`, `implement later`. The plan does say "operator preference" in a few places (e.g., empty `tools/` dir cleanup, branch protection, merge style) — these are genuine human decisions, not unfilled placeholders. Each is paired with a sensible default.

**Type / label consistency:** New label `"trader_arm"` and the second strategy label `"buy_hold"` are introduced in Task 15 and used identically in Tasks 17, 18. `AgentManifest` schema in Task 18 is illustrative; the actual struct shape is whatever is in the file at the time of the edit.

**File-state preconditions:** Each task that touches a previously-modified file references the prior task that produced its current state.

---

*Plan: `docs/superpowers/plans/2026-05-07-cv-extraction.md`*
*Spec: `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`*
*ADR: `decisions/0011-cv-extraction.md`*
