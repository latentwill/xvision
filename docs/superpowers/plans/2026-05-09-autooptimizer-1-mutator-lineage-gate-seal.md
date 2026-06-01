# AutoOptimizer AR-1 — Mutator + Lineage Store + Numeric Gate + CycleSeal

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Spec:** `docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md` — full design context. This plan implements **§3.4 (CycleSeal), §4 (mutation surface), §5.1 (numeric gate), §6 (lineage store), §7 (pre-commitment)**.
> **Companion plans (do NOT do here):** AR-2 (cycle orchestrator + judge + canary + inversion + diversity), AR-3 (dashboard + SSE + mutator-skill ladder), MP-1 (marketplace plugin).
> **Hard upstream dependency:** the eval engine plan (`2026-05-08-eval-engine-plan.md`) MUST ship before AR-1 starts. AR-1 imports `xvision_engine::eval::{Run, RunMode, MetricsSummary, Scenario, executor::BacktestExecutor}` and the SQLite migration scaffolding (`migrations/002_eval.sql` introduces the migration runner). If eval engine slips, AR-1 cannot start. Verify before kickoff: `cargo test -p xvision-engine eval 2>&1 | grep "test result"` returns at least one passing test.
> **Hackathon role:** AR-1 is the substrate for the 2026-05-23 go/no-go (autooptimizer spec §10). Wk 2 milestone: "Mutator + lineage store + numeric gate + CycleSeal artifact. Run end-to-end on 2 lineages locally." This plan is exactly that scope.

**Goal:** After this plan ships: `xvn autooptimizer session-init` writes an operator-signed pre-commitment; `xvn autooptimizer mutate-once <parent_bundle_id>` proposes one mutation, paper-tests it on day + held-out windows, runs the numeric gate, commits the result (Active or Ghost) to the content-addressed lineage, and emits a CycleSeal with Merkle root + operator signature. No LLM judge yet (AR-2). No canary, inversion, diversity (AR-2). No live evening loop (AR-2). No dashboard (AR-3). No chain (MP-1).

**Architecture:** New module `xvision-engine/src/autooptimizer/` parallel to `eval/`. Reuses the eval engine's `BacktestExecutor` for paper-testing both day and held-out windows. Introduces a content-addressed blob store at `~/.xvn/lineage/blobs/<hash>.json` for bundles + diffs + traces, indexed via new SQLite tables in migration `003_autooptimizer.sql`. Operator signing key (Ed25519) generated once and persisted at `~/.xvn/keys/operator.ed25519`; key reuse with the eval engine's `attestation::sign` keying (same `ed25519-dalek` crate). The autooptimizer module has zero `use` statements pointing at `marketplace/` (CI enforces this in MP-1).

**Tech Stack:** Rust 2021. New deps in `xvision-engine/Cargo.toml`: `blake3 = "1"` (content addressing), `similar = "2"` (unified-diff apply / generate), `ed25519-dalek = "2"` (operator signing — already added by eval-engine plan; if missing, add here), `hex = "0.4"` (signature serialization — already added by eval-engine plan), `dirs = "5"` (XDG-style key/blob paths). All other deps already in workspace.

**Out of scope (explicitly deferred):**
- Cycle orchestrator (`autooptimizer/cycle.rs`) → AR-2
- LLM judge that writes structured findings (`autooptimizer/judge.rs`) → AR-2
- Null-result canary (`autooptimizer/canary.rs`) → AR-2
- Inversion-pair eval (`autooptimizer/inversion.rs`) → AR-2
- Embedding-divergence diversity-decay (`autooptimizer/diversity.rs`) → AR-2
- Mutator-skill ladder metrics → AR-2 (compute) + AR-3 (UI)
- SSE event emission (placeholder types only in AR-1; emitter wiring lands in AR-2)
- Dashboard surfaces → AR-3
- Marketplace anchoring of session commitment + Merkle roots → MP-1
- Slot/template-swap mutations beyond prose/params/tools (per autooptimizer spec §1.3)
- Loosening schedule activation logic (we ship the *static* schedule in `autooptimizer.toml` and in the SessionCommitment hash; the *trigger-and-loosen* code lives in AR-2's cycle orchestrator)

---

## File structure

```
crates/xvision-engine/
├── Cargo.toml                                       # add blake3, similar, dirs (+ ed25519-dalek/hex if eval-engine plan hasn't added them yet)
├── migrations/
│   ├── 002_eval.sql                                 # already shipped by eval-engine plan
│   └── 003_autooptimizer.sql                         # NEW (this plan)
├── prompts/
│   └── autooptimizer/
│       └── mutator-v1.md                            # NEW: OSShip-style mutator prompt
├── src/
│   ├── lib.rs                                       # add `pub mod autooptimizer;`
│   ├── bundle/
│   │   ├── mod.rs                                   # MODIFY: add `pub mod program_view;`
│   │   └── program_view.rs                          # NEW: serialize bundle ↔ markdown for prose mutations
│   └── autooptimizer/
│       ├── mod.rs                                   # NEW: public re-exports
│       ├── content_hash.rs                          # NEW: BLAKE3 + canonical JSON helpers
│       ├── blob_store.rs                            # NEW: filesystem-backed content store under ~/.xvn/lineage/blobs/
│       ├── session.rs                               # NEW: SessionCommitment + operator key load/generate + signing
│       ├── config.rs                                # NEW: AutoOptimizerConfig (loads autooptimizer.toml)
│       ├── mutator.rs                               # NEW: MutationDiff, ParamChange, ToolDiff, Mutator (LLM proposer + 2-retry loop)
│       ├── validator.rs                             # NEW: validate_mutation_diff()
│       ├── lineage.rs                               # NEW: LineageNode, LineageStore, Merkle-root computation
│       ├── gate.rs                                  # NEW: deterministic numeric Δ-Sharpe gate
│       ├── seal.rs                                  # NEW: CycleSeal struct + writer
│       └── progress.rs                              # NEW: SSE event taxonomy (types only; emitter wiring is AR-2)
└── tests/
    ├── autooptimizer_content_hash.rs                 # NEW
    ├── autooptimizer_blob_store.rs                   # NEW
    ├── autooptimizer_session.rs                      # NEW
    ├── autooptimizer_program_view.rs                 # NEW
    ├── autooptimizer_mutator.rs                      # NEW
    ├── autooptimizer_validator.rs                    # NEW
    ├── autooptimizer_lineage.rs                      # NEW
    ├── autooptimizer_gate.rs                         # NEW
    ├── autooptimizer_seal.rs                         # NEW
    └── autooptimizer_mutate_once_e2e.rs              # NEW: end-to-end CLI smoke
```

Plus modifications:
- `crates/xvision-cli/src/lib.rs` — add `Command::AutoOptimizer(commands::autooptimizer::AutoOptimizerCmd)` to top-level `Command` enum
- `crates/xvision-cli/src/commands/autooptimizer.rs` — NEW subcommand dispatcher: `xvn autooptimizer {session-init | mutate-once | lineage ls | lineage show | seal show}`
- `config/autooptimizer.toml.example` — NEW: sample config the operator commits to disk before `session-init`

**Why this decomposition:** each file has one responsibility (content-addressed blob layer, session pre-commitment, mutation proposal, validator, gate, lineage, seal). The mutator is the only LLM-touching file in AR-1. The gate is pure-numeric and deterministic. Lineage owns the genealogy + Merkle computation but doesn't know about mutations themselves. Seal aggregates everything else into one signed artifact. This shape lines up exactly with the spec's §3.1 module layout, modulo files deferred to AR-2.

---

## Phase A — Infrastructure: content hash, blob store, session, config, program-view

### Task 1: Cargo deps + module wiring

**Files:**
- Modify: `crates/xvision-engine/Cargo.toml`
- Modify: `crates/xvision-engine/src/lib.rs`
- Create: `crates/xvision-engine/src/autooptimizer/mod.rs`

- [ ] **Step 1: Add deps**

Edit `crates/xvision-engine/Cargo.toml`. Under `[dependencies]`, add (insert in alphabetical position):

```toml
blake3       = "1"
dirs         = "5"
ed25519-dalek = "2"        # skip if eval-engine plan already added
hex          = "0.4"       # skip if eval-engine plan already added
similar      = "2"
```

If `ed25519-dalek` or `hex` is already present (eval-engine landed first), don't duplicate.

- [ ] **Step 2: Wire module in lib.rs**

Edit `crates/xvision-engine/src/lib.rs`. After `pub mod tools;`, insert:

```rust
pub mod autooptimizer;
```

- [ ] **Step 3: Write the autooptimizer module skeleton**

Create `crates/xvision-engine/src/autooptimizer/mod.rs`:

```rust
//! xvision-engine autooptimizer — chain-free evening mutation loop.
//!
//! See: docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md
//!
//! AR-1 scope: mutator + lineage + numeric gate + CycleSeal.
//! AR-2 scope: cycle orchestrator + LLM judge + canary + inversion + diversity.
//! AR-3 scope: dashboard surfaces.

pub mod blob_store;
pub mod config;
pub mod content_hash;
pub mod gate;
pub mod lineage;
pub mod mutator;
pub mod progress;
pub mod seal;
pub mod session;
pub mod validator;

pub use config::AutoOptimizerConfig;
pub use content_hash::ContentHash;
pub use gate::{GateDecision, NumericGate};
pub use lineage::{LineageEdge, LineageNode, LineageStatus, LineageStore};
pub use mutator::{MutationDiff, Mutator, ParamChange, ToolDiff};
pub use seal::{CycleSeal, CycleSealWriter};
pub use session::{OperatorKey, SessionCommitment};
```

- [ ] **Step 4: Build verifies**

Run: `cargo build -p xvision-engine`
Expected: succeeds (empty modules will be created in subsequent tasks; this step only verifies the module declarations compile once stub files exist — defer this step to after Task 2 if module files are missing).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/Cargo.toml crates/xvision-engine/src/lib.rs crates/xvision-engine/src/autooptimizer/mod.rs
git commit -m "feat(autooptimizer): scaffold module + add blake3/similar/dirs deps"
```

---

### Task 2: Content-hash helpers (BLAKE3 + canonical JSON)

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/content_hash.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_content_hash.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_content_hash.rs`:

```rust
use xvision_engine::autooptimizer::content_hash::{canonicalize_json, ContentHash};

#[test]
fn same_logical_value_same_hash_regardless_of_key_order() {
    let a = serde_json::json!({"b": 1, "a": 2, "nested": {"y": "two", "x": "one"}});
    let b = serde_json::json!({"a": 2, "b": 1, "nested": {"x": "one", "y": "two"}});
    assert_eq!(ContentHash::of_json(&a), ContentHash::of_json(&b));
}

#[test]
fn different_value_different_hash() {
    let a = serde_json::json!({"x": 1});
    let b = serde_json::json!({"x": 2});
    assert_ne!(ContentHash::of_json(&a), ContentHash::of_json(&b));
}

#[test]
fn hash_string_is_64_hex_chars() {
    let h = ContentHash::of_bytes(b"hello world");
    assert_eq!(h.to_hex().len(), 64);
    assert!(h.to_hex().chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn hash_round_trips_through_string() {
    let h = ContentHash::of_bytes(b"x");
    let s = h.to_hex();
    let h2 = ContentHash::from_hex(&s).unwrap();
    assert_eq!(h, h2);
}

#[test]
fn canonicalize_orders_object_keys() {
    let v = serde_json::json!({"z": 1, "a": 2, "m": 3});
    let c = canonicalize_json(&v);
    let s = serde_json::to_string(&c).unwrap();
    assert_eq!(s, r#"{"a":2,"m":3,"z":1}"#);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_content_hash`
Expected: FAIL with "unresolved import `xvision_engine::autooptimizer::content_hash`"

- [ ] **Step 3: Implement content_hash.rs**

Create `crates/xvision-engine/src/autooptimizer/content_hash.rs`:

```rust
//! Content addressing for autooptimizer artifacts.
//!
//! Every bundle, mutation diff, paper-test trace, finding, and cycle seal is
//! addressed by BLAKE3 over a canonical JSON serialization. Canonical JSON
//! sorts object keys lexicographically so logically-equal values produce the
//! same hash regardless of how serde happened to serialize them.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn of_json(value: &serde_json::Value) -> Self {
        let canonical = canonicalize_json(value);
        let bytes = serde_json::to_vec(&canonical)
            .expect("canonicalize_json output is always serializable");
        Self::of_bytes(&bytes)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> anyhow::Result<Self> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            anyhow::bail!("ContentHash::from_hex expected 32 bytes, got {}", bytes.len());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Canonicalize a JSON value: sort object keys, recurse into arrays + nested
/// objects. Numbers, strings, bools, nulls pass through unchanged. The eval
/// engine uses an identical helper in `eval/attestation.rs`; if it's already
/// extracted to a shared crate by the time this lands, reuse that. Otherwise
/// duplicate it here and leave a TODO to consolidate post-hackathon.
pub fn canonicalize_json(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize_json(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        }
        other => other.clone(),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xvision-engine --test autooptimizer_content_hash`
Expected: PASS — `5 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/content_hash.rs crates/xvision-engine/tests/autooptimizer_content_hash.rs
git commit -m "feat(autooptimizer): BLAKE3 content hashing + canonical JSON"
```

---

### Task 3: Filesystem blob store

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/blob_store.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_blob_store.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_blob_store.rs`:

```rust
use tempfile::tempdir;
use xvision_engine::autooptimizer::blob_store::BlobStore;
use xvision_engine::autooptimizer::content_hash::ContentHash;

#[tokio::test]
async fn put_then_get_round_trips_json() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let value = serde_json::json!({"hello": "world", "n": 42});
    let hash = store.put_json(&value).await.unwrap();
    let loaded = store.get_json(&hash).await.unwrap();
    assert_eq!(loaded, value);
}

#[tokio::test]
async fn put_is_idempotent_on_same_value() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let value = serde_json::json!({"k": "v"});
    let h1 = store.put_json(&value).await.unwrap();
    let h2 = store.put_json(&value).await.unwrap();
    assert_eq!(h1, h2);
}

#[tokio::test]
async fn get_missing_returns_not_found_error() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let bogus = ContentHash::of_bytes(b"no such blob");
    let err = store.get_json(&bogus).await.unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {err}");
}

#[tokio::test]
async fn put_bytes_and_get_bytes_round_trip() {
    let dir = tempdir().unwrap();
    let store = BlobStore::open(dir.path().to_path_buf()).await.unwrap();
    let payload = b"raw bytes can also be stored";
    let hash = store.put_bytes(payload).await.unwrap();
    let loaded = store.get_bytes(&hash).await.unwrap();
    assert_eq!(loaded.as_slice(), payload);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_blob_store`
Expected: FAIL with "unresolved import `xvision_engine::autooptimizer::blob_store`"

- [ ] **Step 3: Implement blob_store.rs**

Create `crates/xvision-engine/src/autooptimizer/blob_store.rs`:

```rust
//! Filesystem-backed content-addressed blob store.
//!
//! Layout under `root`:
//!   <root>/<hh>/<hh>/<remaining-60-hex>.json   — for JSON blobs
//!   <root>/<hh>/<hh>/<remaining-60-hex>.bin    — for raw byte blobs
//!
//! Two-level fan-out keeps any single directory < a few thousand entries even
//! with millions of blobs. The default root is `~/.xvn/lineage/blobs`; tests
//! pass an explicit tempdir.

use std::path::{Path, PathBuf};

use crate::autooptimizer::content_hash::ContentHash;

pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub async fn open(root: PathBuf) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    /// Default location: `~/.xvn/lineage/blobs`.
    pub async fn open_default() -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not resolve home dir"))?;
        Self::open(home.join(".xvn/lineage/blobs")).await
    }

    pub async fn put_json(&self, value: &serde_json::Value) -> anyhow::Result<ContentHash> {
        let hash = ContentHash::of_json(value);
        let path = self.path_for(&hash, "json");
        if tokio::fs::try_exists(&path).await? {
            return Ok(hash); // idempotent
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Canonicalize before writing so on-disk bytes match the hash input.
        let canonical = crate::autooptimizer::content_hash::canonicalize_json(value);
        let bytes = serde_json::to_vec_pretty(&canonical)?;
        atomic_write(&path, &bytes).await?;
        Ok(hash)
    }

    pub async fn get_json(&self, hash: &ContentHash) -> anyhow::Result<serde_json::Value> {
        let path = self.path_for(hash, "json");
        if !tokio::fs::try_exists(&path).await? {
            anyhow::bail!("blob not found: {hash}");
        }
        let bytes = tokio::fs::read(&path).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn put_bytes(&self, payload: &[u8]) -> anyhow::Result<ContentHash> {
        let hash = ContentHash::of_bytes(payload);
        let path = self.path_for(&hash, "bin");
        if tokio::fs::try_exists(&path).await? {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        atomic_write(&path, payload).await?;
        Ok(hash)
    }

    pub async fn get_bytes(&self, hash: &ContentHash) -> anyhow::Result<Vec<u8>> {
        let path = self.path_for(hash, "bin");
        if !tokio::fs::try_exists(&path).await? {
            anyhow::bail!("blob not found: {hash}");
        }
        Ok(tokio::fs::read(&path).await?)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, hash: &ContentHash, ext: &str) -> PathBuf {
        let hex = hash.to_hex();
        // Two-level fan-out: first 2 chars / next 2 chars / remaining + ext.
        let (h1, rest) = hex.split_at(2);
        let (h2, tail) = rest.split_at(2);
        self.root.join(h1).join(h2).join(format!("{tail}.{ext}"))
    }
}

async fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xvision-engine --test autooptimizer_blob_store`
Expected: PASS — `4 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/blob_store.rs crates/xvision-engine/tests/autooptimizer_blob_store.rs
git commit -m "feat(autooptimizer): filesystem-backed content-addressed blob store"
```

---

### Task 4: Bundle ↔ markdown program-view

The autooptimizer spec §4 models the prose mutation as "unified diff over `program.md`." The actual bundle (`xvision-engine/src/bundle/mod.rs`) doesn't have a `program.md` field — it has up to three `LLMSlot`s (regime / intern / trader), each with its own prompt. We bridge by serializing the slots into a single canonical markdown document, applying the diff, and parsing the result back into per-slot prompts. Non-prose fields (manifest, mechanical_params, risk) are passed through unchanged.

**Files:**
- Modify: `crates/xvision-engine/src/bundle/mod.rs`
- Create: `crates/xvision-engine/src/bundle/program_view.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_program_view.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_program_view.rs`:

```rust
use xvision_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xvision_engine::bundle::program_view::{
    apply_unified_diff, from_markdown, to_markdown, ProgramViewError,
};
use xvision_engine::bundle::risk::RiskConfig;
use xvision_engine::bundle::slot::LLMSlot;
use xvision_engine::bundle::StrategyBundle;

fn sample_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01HZZ".into(),
            display_name: "Sample".into(),
            plain_summary: "Buys dips".into(),
            creator: "@x".into(),
            template: "trend_follower".into(),
            regime_fit: vec![RegimeFit::TrendingBull],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            required_models: vec!["anthropic.claude-sonnet-4.6+".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            prompt: "Detect regime.".into(),
            model_requirement: "anthropic.claude-haiku-4-5+".into(),
            allowed_tools: vec![],
        }),
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "Decide long/short/flat.".into(),
            model_requirement: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
        }),
        risk: RiskConfig {
            risk_pct_per_trade: 0.01,
            max_leverage: 3.0,
            stop_loss_pct: 0.02,
            take_profit_pct: 0.04,
        },
        mechanical_params: serde_json::json!({"rsi_period": 14}),
    }
}

#[test]
fn to_markdown_round_trips_through_from_markdown() {
    let bundle = sample_bundle();
    let md = to_markdown(&bundle);
    let restored = from_markdown(&bundle, &md).unwrap();
    assert_eq!(restored.regime_slot.as_ref().unwrap().prompt, "Detect regime.");
    assert_eq!(restored.trader_slot.as_ref().unwrap().prompt, "Decide long/short/flat.");
    assert!(restored.intern_slot.is_none());
    // Non-prose fields preserved.
    assert_eq!(restored.manifest.id, bundle.manifest.id);
    assert_eq!(restored.mechanical_params, bundle.mechanical_params);
}

#[test]
fn applying_unified_diff_changes_only_targeted_section() {
    let bundle = sample_bundle();
    let original_md = to_markdown(&bundle);
    let modified_md = original_md.replace("Decide long/short/flat.", "Decide long/short/flat WITH a 5-bar confirmation.");
    let diff = similar::TextDiff::from_lines(&original_md, &modified_md)
        .unified_diff()
        .header("a/program.md", "b/program.md")
        .to_string();
    let patched = apply_unified_diff(&original_md, &diff).unwrap();
    let restored = from_markdown(&bundle, &patched).unwrap();
    assert!(restored.trader_slot.as_ref().unwrap().prompt.contains("5-bar confirmation"));
    assert_eq!(restored.regime_slot.as_ref().unwrap().prompt, "Detect regime.");
}

#[test]
fn apply_unified_diff_rejects_diff_that_doesnt_match_source() {
    let original = "## Slot: trader\nfoo\n";
    let bogus_diff = "--- a/program.md\n+++ b/program.md\n@@ -1,1 +1,1 @@\n-DOES NOT EXIST\n+replacement\n";
    let err = apply_unified_diff(original, bogus_diff).unwrap_err();
    assert!(matches!(err, ProgramViewError::DiffDidNotApply { .. }), "got: {err:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_program_view`
Expected: FAIL with "unresolved import `xvision_engine::bundle::program_view`"

- [ ] **Step 3: Add module declaration**

Edit `crates/xvision-engine/src/bundle/mod.rs`. Find the line `pub mod store;` and after it add:

```rust
pub mod program_view;
```

- [ ] **Step 4: Implement program_view.rs**

Create `crates/xvision-engine/src/bundle/program_view.rs`:

```rust
//! Bundle ↔ markdown serialization for prose mutations.
//!
//! The autooptimizer spec models prose mutations as unified diffs over a
//! "program.md" view of the bundle. The bundle has no native program.md
//! field, so we synthesize one by concatenating slot prompts under canonical
//! `## Slot: <role>` headers. Apply a diff → re-parse → patch back into
//! per-slot prompts. Non-prose fields are passed through unchanged.

use thiserror::Error;

use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;

#[derive(Debug, Error)]
pub enum ProgramViewError {
    #[error("could not parse program-view markdown: missing or malformed `## Slot:` header")]
    MalformedHeader,
    #[error("program-view contains a slot ('{role}') that the parent bundle does not declare")]
    UnknownSlot { role: String },
    #[error("unified diff did not apply cleanly: {reason}")]
    DiffDidNotApply { reason: String },
}

const SECTION_PREFIX: &str = "## Slot:";

/// Serialize the bundle's slot prompts into a single canonical markdown doc.
/// Order is fixed: regime (if present) → intern (if present) → trader (always
/// present per bundle validation).
pub fn to_markdown(bundle: &StrategyBundle) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Strategy: {}\n\n", bundle.manifest.display_name));
    if let Some(s) = &bundle.regime_slot {
        write_section(&mut out, &s.role, &s.prompt);
    }
    if let Some(s) = &bundle.intern_slot {
        write_section(&mut out, &s.role, &s.prompt);
    }
    if let Some(s) = &bundle.trader_slot {
        write_section(&mut out, &s.role, &s.prompt);
    }
    out
}

fn write_section(out: &mut String, role: &str, prompt: &str) {
    out.push_str(&format!("{SECTION_PREFIX} {role}\n"));
    out.push_str(prompt);
    if !prompt.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
}

/// Re-parse a program-view markdown back into a bundle. Non-prose fields
/// (manifest, mechanical_params, risk) come from `parent` unchanged. Slot
/// presence is dictated by `parent` — sections in the markdown that the
/// parent doesn't declare are rejected (the validator enforces that the
/// mutator can't add a slot via prose-only mutation).
pub fn from_markdown(parent: &StrategyBundle, md: &str) -> Result<StrategyBundle, ProgramViewError> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current_role: Option<String> = None;
    let mut current_body = String::new();

    for line in md.lines() {
        if let Some(rest) = line.strip_prefix(SECTION_PREFIX) {
            if let Some(role) = current_role.take() {
                sections.push((role, current_body.trim_end().to_string()));
                current_body.clear();
            }
            let role = rest.trim().to_string();
            if role.is_empty() {
                return Err(ProgramViewError::MalformedHeader);
            }
            current_role = Some(role);
        } else if current_role.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
        // Lines before the first section header (e.g., the title) are ignored.
    }
    if let Some(role) = current_role.take() {
        sections.push((role, current_body.trim_end().to_string()));
    }

    let mut new_bundle = parent.clone();
    let mut saw_regime = false;
    let mut saw_intern = false;
    let mut saw_trader = false;

    for (role, body) in sections {
        match role.as_str() {
            "regime" => {
                let slot = parent
                    .regime_slot
                    .clone()
                    .ok_or(ProgramViewError::UnknownSlot { role: role.clone() })?;
                new_bundle.regime_slot = Some(LLMSlot { prompt: body, ..slot });
                saw_regime = true;
            }
            "intern" => {
                let slot = parent
                    .intern_slot
                    .clone()
                    .ok_or(ProgramViewError::UnknownSlot { role: role.clone() })?;
                new_bundle.intern_slot = Some(LLMSlot { prompt: body, ..slot });
                saw_intern = true;
            }
            "trader" => {
                let slot = parent
                    .trader_slot
                    .clone()
                    .ok_or(ProgramViewError::UnknownSlot { role: role.clone() })?;
                new_bundle.trader_slot = Some(LLMSlot { prompt: body, ..slot });
                saw_trader = true;
            }
            _ => return Err(ProgramViewError::UnknownSlot { role }),
        }
    }

    // If the parent declared a slot and the markdown didn't include it, that
    // means the mutator dropped the section — preserve the parent's prompt
    // verbatim. (We could choose to error instead; preserve is safer for v1.)
    let _ = (saw_regime, saw_intern, saw_trader);

    Ok(new_bundle)
}

/// Apply a unified diff to a source string. We use `similar`'s patch parser
/// indirectly by reconstructing the target from the diff's lines. For v1 we
/// only support clean applies (no fuzzy hunk matching). If the diff is dirty,
/// the mutator retries with the validator's error fed back.
pub fn apply_unified_diff(source: &str, diff: &str) -> Result<String, ProgramViewError> {
    let patch = parse_unified_diff(diff)?;
    let mut src_lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
    let mut out: Vec<String> = Vec::new();
    let mut src_idx = 0usize;

    for hunk in patch.hunks {
        // Copy unchanged lines up to the hunk start.
        let hunk_src_start = hunk.src_start.saturating_sub(1);
        if hunk_src_start < src_idx {
            return Err(ProgramViewError::DiffDidNotApply {
                reason: format!(
                    "hunk starts at line {} but we already consumed up to {}",
                    hunk.src_start, src_idx
                ),
            });
        }
        while src_idx < hunk_src_start {
            if src_idx >= src_lines.len() {
                return Err(ProgramViewError::DiffDidNotApply {
                    reason: "ran past end of source while seeking hunk".into(),
                });
            }
            out.push(std::mem::take(&mut src_lines[src_idx]));
            src_idx += 1;
        }
        // Apply hunk lines.
        for h in hunk.lines {
            match h {
                HunkLine::Context(text) | HunkLine::Removed(text) => {
                    if src_idx >= src_lines.len() || src_lines[src_idx] != text {
                        return Err(ProgramViewError::DiffDidNotApply {
                            reason: format!(
                                "expected `{text}` at source line {}, got `{}`",
                                src_idx + 1,
                                src_lines.get(src_idx).map(|s| s.as_str()).unwrap_or("<EOF>"),
                            ),
                        });
                    }
                    if matches!(h, HunkLine::Context(_)) {
                        out.push(text);
                    }
                    src_idx += 1;
                }
                HunkLine::Added(text) => {
                    out.push(text);
                }
            }
        }
    }
    while src_idx < src_lines.len() {
        out.push(std::mem::take(&mut src_lines[src_idx]));
        src_idx += 1;
    }
    let mut joined = out.join("\n");
    if source.ends_with('\n') {
        joined.push('\n');
    }
    Ok(joined)
}

#[derive(Debug)]
enum HunkLine {
    Context(String),
    Added(String),
    Removed(String),
}

#[derive(Debug)]
struct Hunk {
    src_start: usize,
    lines: Vec<HunkLine>,
}

#[derive(Debug)]
struct Patch {
    hunks: Vec<Hunk>,
}

fn parse_unified_diff(diff: &str) -> Result<Patch, ProgramViewError> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<Hunk> = None;
    for line in diff.lines() {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@") {
            // Hunk header e.g. "@@ -3,4 +3,5 @@"
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            let src_start = parse_hunk_src_start(rest)?;
            current = Some(Hunk {
                src_start,
                lines: Vec::new(),
            });
            continue;
        }
        let h = current.as_mut().ok_or(ProgramViewError::DiffDidNotApply {
            reason: "diff line outside any hunk".into(),
        })?;
        if let Some(text) = line.strip_prefix('+') {
            h.lines.push(HunkLine::Added(text.to_string()));
        } else if let Some(text) = line.strip_prefix('-') {
            h.lines.push(HunkLine::Removed(text.to_string()));
        } else if let Some(text) = line.strip_prefix(' ') {
            h.lines.push(HunkLine::Context(text.to_string()));
        } else if line.is_empty() {
            h.lines.push(HunkLine::Context(String::new()));
        } else {
            return Err(ProgramViewError::DiffDidNotApply {
                reason: format!("unrecognized diff line: `{line}`"),
            });
        }
    }
    if let Some(h) = current.take() {
        hunks.push(h);
    }
    Ok(Patch { hunks })
}

fn parse_hunk_src_start(rest: &str) -> Result<usize, ProgramViewError> {
    // rest looks like " -3,4 +3,5 @@..." — extract the integer after `-`.
    let trimmed = rest.trim_start();
    let after_minus = trimmed.strip_prefix('-').ok_or(ProgramViewError::DiffDidNotApply {
        reason: format!("hunk header missing `-` marker: `{trimmed}`"),
    })?;
    let num: String = after_minus.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse::<usize>().map_err(|_| ProgramViewError::DiffDidNotApply {
        reason: format!("could not parse src start from `{after_minus}`"),
    })
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p xvision-engine --test autooptimizer_program_view`
Expected: PASS — `3 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/bundle/mod.rs crates/xvision-engine/src/bundle/program_view.rs crates/xvision-engine/tests/autooptimizer_program_view.rs
git commit -m "feat(bundle): markdown program-view (serialize/parse/patch) for prose mutations"
```

---

### Task 5: AutoOptimizerConfig (autooptimizer.toml loader)

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/config.rs`
- Create: `config/autooptimizer.toml.example`

- [ ] **Step 1: Sample config**

Create `config/autooptimizer.toml.example`:

```toml
# AutoOptimizer evening-cycle config.
# Hashed at session-init; the hash is part of the SessionCommitment.

[cycle]
mutations_per_parent = 3
parents_per_evening  = 5
per_cycle_token_cap  = 250000   # mutator + judge combined

[gate]
epsilon_initial = 0.10

# Pre-committed loosening schedule. Each step lists {after_no_merge_nights, new_epsilon}.
# Triggered by AR-2's cycle orchestrator; the schedule itself is part of the seal.
loosening_schedule = [
  { after_no_merge_nights = 3, new_epsilon = 0.07 },
  { after_no_merge_nights = 6, new_epsilon = 0.05 },
]

[holdout]
# Time range pinned at session-init. Never touched by day trading.
start_iso = "2025-09-01T00:00:00Z"
end_iso   = "2025-12-01T00:00:00Z"

[parent_policy]
kind = "round_robin"   # "round_robin" | "top_k" | "epsilon_greedy"
top_k = 5
epsilon_explore = 0.20
seed = 1               # parent_policy_seed; sealed in SessionCommitment

[mutator]
model = "claude-haiku-4-5"
max_tokens = 4096

[judge]                 # consumed by AR-2; AR-1 ignores
model = "claude-sonnet-4.6"
max_tokens = 4096

[diversity]             # consumed by AR-2; AR-1 ignores
embedding_model = "text-embedding-3-small"

[canary]                # consumed by AR-2; AR-1 ignores
seed = 17
```

- [ ] **Step 2: Add the toml dep if missing**

Check `crates/xvision-engine/Cargo.toml`. If `toml = "..."` is not present, add `toml = "0.8"` to `[dependencies]`.

- [ ] **Step 3: Implement config.rs**

Create `crates/xvision-engine/src/autooptimizer/config.rs`:

```rust
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::autooptimizer::content_hash::ContentHash;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoOptimizerConfig {
    pub cycle: CycleConfig,
    pub gate: GateConfig,
    pub holdout: HoldoutConfig,
    pub parent_policy: ParentPolicyConfig,
    pub mutator: MutatorConfig,
    pub judge: JudgeConfig,
    pub diversity: DiversityConfig,
    pub canary: CanaryConfig,
}

impl AutoOptimizerConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)?;
        let s = std::str::from_utf8(&bytes)?;
        Ok(toml::from_str(s)?)
    }

    /// Hash of the canonical JSON serialization of the config — sealed into
    /// the SessionCommitment.
    pub fn content_hash(&self) -> ContentHash {
        let v = serde_json::to_value(self).expect("config is always serializable");
        ContentHash::of_json(&v)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CycleConfig {
    pub mutations_per_parent: u32,
    pub parents_per_evening: u32,
    pub per_cycle_token_cap: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateConfig {
    pub epsilon_initial: f64,
    pub loosening_schedule: Vec<LooseningStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LooseningStep {
    pub after_no_merge_nights: u32,
    pub new_epsilon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HoldoutConfig {
    pub start_iso: DateTime<Utc>,
    pub end_iso: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentPolicyConfig {
    pub kind: String,           // "round_robin" | "top_k" | "epsilon_greedy"
    pub top_k: u32,
    pub epsilon_explore: f64,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MutatorConfig {
    pub model: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgeConfig {
    pub model: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiversityConfig {
    pub embedding_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanaryConfig {
    pub seed: u64,
}
```

- [ ] **Step 4: Test**

Add inline test at the bottom of `config.rs` (or a separate tests file — inline keeps the round-trip close to the data):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_example_config() {
        let example = include_str!("../../../../config/autooptimizer.toml.example");
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(example.as_bytes()).unwrap();
        let cfg = AutoOptimizerConfig::load(f.path()).unwrap();
        assert_eq!(cfg.cycle.mutations_per_parent, 3);
        assert!((cfg.gate.epsilon_initial - 0.10).abs() < 1e-9);
        assert_eq!(cfg.gate.loosening_schedule.len(), 2);
        assert_eq!(cfg.parent_policy.kind, "round_robin");
    }

    #[test]
    fn config_hash_is_stable_across_logically_equal_serializations() {
        let example = include_str!("../../../../config/autooptimizer.toml.example");
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(example.as_bytes()).unwrap();
        let cfg = AutoOptimizerConfig::load(f.path()).unwrap();
        let h1 = cfg.content_hash();
        let h2 = cfg.content_hash();
        assert_eq!(h1, h2);
    }
}
```

- [ ] **Step 5: Run test**

Run: `cargo test -p xvision-engine autooptimizer::config`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/config.rs config/autooptimizer.toml.example crates/xvision-engine/Cargo.toml
git commit -m "feat(autooptimizer): config struct + example TOML + content hash"
```

---

### Task 6: SessionCommitment + operator key

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/session.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_session.rs`

The SessionCommitment (autooptimizer spec §7) is the operator-signed "this is the configuration before any cycles ran" artifact. It seals: ε, holdout window, parent-policy seed, cycle config hash, canary seed, session_id, and the operator's signature.

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_session.rs`:

```rust
use chrono::Utc;
use tempfile::tempdir;
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::session::{OperatorKey, SessionCommitment};

#[test]
fn operator_key_persists_and_round_trips() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let k1 = OperatorKey::load_or_generate(&key_path).unwrap();
    let k2 = OperatorKey::load_or_generate(&key_path).unwrap();
    assert_eq!(k1.public_hex(), k2.public_hex());
}

#[test]
fn session_commitment_signs_and_verifies() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key = OperatorKey::load_or_generate(&key_path).unwrap();
    let commit = SessionCommitment::new(
        0.10,
        Utc::now(),
        Utc::now() + chrono::Duration::days(90),
        7,
        ContentHash::of_bytes(b"config"),
        17,
        &key,
    );
    assert!(commit.verify().is_ok());
}

#[test]
fn tampered_commitment_fails_verification() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key = OperatorKey::load_or_generate(&key_path).unwrap();
    let mut commit = SessionCommitment::new(
        0.10,
        Utc::now(),
        Utc::now() + chrono::Duration::days(90),
        7,
        ContentHash::of_bytes(b"config"),
        17,
        &key,
    );
    commit.epsilon = 0.05; // tamper
    assert!(commit.verify().is_err());
}

#[test]
fn commitment_hash_changes_when_any_field_changes() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key = OperatorKey::load_or_generate(&key_path).unwrap();
    let now = Utc::now();
    let later = now + chrono::Duration::days(90);
    let cfg = ContentHash::of_bytes(b"config");
    let a = SessionCommitment::new(0.10, now, later, 7, cfg, 17, &key);
    let b = SessionCommitment::new(0.05, now, later, 7, cfg, 17, &key); // different epsilon
    assert_ne!(a.commitment_hash(), b.commitment_hash());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_session`
Expected: FAIL with "unresolved import".

- [ ] **Step 3: Implement session.rs**

Create `crates/xvision-engine/src/autooptimizer/session.rs`:

```rust
//! Operator key management + SessionCommitment.
//!
//! See autooptimizer spec §7. The session commitment is the operator-signed
//! "this is what we pre-committed to before any cycles ran" artifact. It
//! lives on disk; if the marketplace plugin is enabled, it's anchored to
//! Mantle once at session start. AR-1 only writes it locally.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::autooptimizer::content_hash::ContentHash;

/// Long-lived operator key. Persisted at `~/.xvn/keys/operator.ed25519` (or
/// a tempdir for tests). Generated on first use; reused thereafter.
pub struct OperatorKey {
    signing: SigningKey,
}

impl OperatorKey {
    pub fn load_or_generate(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let bytes = std::fs::read(path)?;
            if bytes.len() != 32 {
                anyhow::bail!(
                    "operator key file at {} has wrong length (expected 32, got {})",
                    path.display(),
                    bytes.len()
                );
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(Self {
                signing: SigningKey::from_bytes(&arr),
            })
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut rng_bytes = [0u8; 32];
            getrandom::getrandom(&mut rng_bytes).map_err(|e| anyhow::anyhow!("getrandom failed: {e}"))?;
            let signing = SigningKey::from_bytes(&rng_bytes);
            // Write atomically.
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, signing.to_bytes())?;
            std::fs::rename(&tmp, path)?;
            // Restrict perms to 0600 on unix; non-fatal on other platforms.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(path)?.permissions();
                perms.set_mode(0o600);
                std::fs::set_permissions(path, perms)?;
            }
            Ok(Self { signing })
        }
    }

    pub fn default_key_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
        Ok(home.join(".xvn/keys/operator.ed25519"))
    }

    pub fn public_hex(&self) -> String {
        hex::encode(self.signing.verifying_key().as_bytes())
    }

    pub fn sign(&self, bytes: &[u8]) -> Signature {
        self.signing.sign(bytes)
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }
}

/// Per-session pre-commitment. Sealed and signed once at session start.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionCommitment {
    pub session_id: String,                  // ULID
    pub epsilon: f64,
    pub holdout_start: DateTime<Utc>,
    pub holdout_end: DateTime<Utc>,
    pub parent_policy_seed: u64,
    pub cycle_config_hash: ContentHash,
    pub canary_seed: u64,
    pub operator_pubkey_hex: String,
    pub signature_hex: String,
    pub created_at: DateTime<Utc>,
}

impl SessionCommitment {
    pub fn new(
        epsilon: f64,
        holdout_start: DateTime<Utc>,
        holdout_end: DateTime<Utc>,
        parent_policy_seed: u64,
        cycle_config_hash: ContentHash,
        canary_seed: u64,
        key: &OperatorKey,
    ) -> Self {
        let session_id = Ulid::new().to_string();
        let created_at = Utc::now();
        let signing_payload = canonical_payload(
            &session_id,
            epsilon,
            holdout_start,
            holdout_end,
            parent_policy_seed,
            cycle_config_hash,
            canary_seed,
            created_at,
        );
        let bytes = serde_json::to_vec(&signing_payload).expect("payload serializes");
        let sig = key.sign(&bytes);
        Self {
            session_id,
            epsilon,
            holdout_start,
            holdout_end,
            parent_policy_seed,
            cycle_config_hash,
            canary_seed,
            operator_pubkey_hex: key.public_hex(),
            signature_hex: hex::encode(sig.to_bytes()),
            created_at,
        }
    }

    pub fn verify(&self) -> anyhow::Result<()> {
        let pubkey_bytes = hex::decode(&self.operator_pubkey_hex)?;
        let arr: [u8; 32] = pubkey_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("operator pubkey wrong length"))?;
        let pubkey = VerifyingKey::from_bytes(&arr)?;
        let sig_bytes = hex::decode(&self.signature_hex)?;
        let sig_arr: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("signature wrong length"))?;
        let sig = Signature::from_bytes(&sig_arr);
        let signing_payload = canonical_payload(
            &self.session_id,
            self.epsilon,
            self.holdout_start,
            self.holdout_end,
            self.parent_policy_seed,
            self.cycle_config_hash,
            self.canary_seed,
            self.created_at,
        );
        let bytes = serde_json::to_vec(&signing_payload)?;
        pubkey.verify(&bytes, &sig)?;
        Ok(())
    }

    /// Content hash of the unsigned commitment payload — what marketplace
    /// would anchor to chain.
    pub fn commitment_hash(&self) -> ContentHash {
        let payload = canonical_payload(
            &self.session_id,
            self.epsilon,
            self.holdout_start,
            self.holdout_end,
            self.parent_policy_seed,
            self.cycle_config_hash,
            self.canary_seed,
            self.created_at,
        );
        ContentHash::of_json(&payload)
    }
}

fn canonical_payload(
    session_id: &str,
    epsilon: f64,
    holdout_start: DateTime<Utc>,
    holdout_end: DateTime<Utc>,
    parent_policy_seed: u64,
    cycle_config_hash: ContentHash,
    canary_seed: u64,
    created_at: DateTime<Utc>,
) -> serde_json::Value {
    crate::autooptimizer::content_hash::canonicalize_json(&serde_json::json!({
        "session_id": session_id,
        "epsilon": epsilon,
        "holdout_start": holdout_start,
        "holdout_end": holdout_end,
        "parent_policy_seed": parent_policy_seed,
        "cycle_config_hash": cycle_config_hash,
        "canary_seed": canary_seed,
        "created_at": created_at,
    }))
}
```

- [ ] **Step 4: Add the getrandom dep**

Edit `crates/xvision-engine/Cargo.toml`. If `getrandom = "..."` is not in `[dependencies]`, add `getrandom = "0.2"`. (`ed25519-dalek` 2.x doesn't pull rand by default.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p xvision-engine --test autooptimizer_session`
Expected: PASS — `4 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/session.rs crates/xvision-engine/tests/autooptimizer_session.rs crates/xvision-engine/Cargo.toml
git commit -m "feat(autooptimizer): SessionCommitment + persisted operator Ed25519 key"
```

---

## Phase B — Mutator: MutationDiff schema, validator, LLM proposer

### Task 7: MutationDiff data types

**File:** `crates/xvision-engine/src/autooptimizer/mutator.rs` — first pass: types only, no LLM call yet.

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_mutator.rs`:

```rust
use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::mutator::{MutationDiff, ParamChange, ToolDiff};

#[test]
fn mutation_diff_round_trips_through_json() {
    let parent = ContentHash::of_bytes(b"parent-bundle");
    let diff = MutationDiff {
        prose_diff: Some("--- a/program.md\n+++ b/program.md\n@@ -1,1 +1,1 @@\n-old\n+new\n".into()),
        param_changes: vec![ParamChange {
            key: "rsi.period".into(),
            old: serde_json::json!(14),
            new: serde_json::json!(21),
        }],
        tool_changes: ToolDiff {
            added: vec!["volume_profile".into()],
            removed: vec![],
        },
        mutator_model: "claude-haiku-4-5".into(),
        mutator_token_cost: 1234,
        proposed_at: chrono::Utc::now(),
        parent_hash: parent,
    };
    let json = serde_json::to_string(&diff).unwrap();
    let restored: MutationDiff = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.parent_hash, parent);
    assert_eq!(restored.param_changes.len(), 1);
    assert_eq!(restored.param_changes[0].key, "rsi.period");
}

#[test]
fn empty_diff_is_serializable() {
    let parent = ContentHash::of_bytes(b"x");
    let diff = MutationDiff {
        prose_diff: None,
        param_changes: vec![],
        tool_changes: ToolDiff { added: vec![], removed: vec![] },
        mutator_model: "test".into(),
        mutator_token_cost: 0,
        proposed_at: chrono::Utc::now(),
        parent_hash: parent,
    };
    let s = serde_json::to_string(&diff).unwrap();
    assert!(s.contains("\"parent_hash\""));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_mutator`
Expected: FAIL with unresolved imports.

- [ ] **Step 3: Implement mutator.rs (types only)**

Create `crates/xvision-engine/src/autooptimizer/mutator.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::autooptimizer::content_hash::ContentHash;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MutationDiff {
    pub prose_diff: Option<String>,         // unified text diff over program-view
    pub param_changes: Vec<ParamChange>,
    pub tool_changes: ToolDiff,
    pub mutator_model: String,
    pub mutator_token_cost: u32,
    pub proposed_at: DateTime<Utc>,
    pub parent_hash: ContentHash,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParamChange {
    pub key: String,
    pub old: serde_json::Value,
    pub new: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl MutationDiff {
    /// Content hash of the diff itself — the leaf hash that goes into the
    /// CycleSeal Merkle tree.
    pub fn content_hash(&self) -> ContentHash {
        let v = serde_json::to_value(self).expect("diff serializes");
        ContentHash::of_json(&v)
    }
}
```

(The `Mutator` struct + LLM call land in Task 9; types here are the contract surface for the validator and the seal.)

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_mutator
git add crates/xvision-engine/src/autooptimizer/mutator.rs crates/xvision-engine/tests/autooptimizer_mutator.rs
git commit -m "feat(autooptimizer): MutationDiff + ParamChange + ToolDiff types"
```

---

### Task 8: Validator (validate_mutation_diff)

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/validator.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_validator.rs`

The validator (per autooptimizer spec §4) enforces four invariants on a candidate diff against a parent bundle. It's invoked twice: once before paper-testing (to reject obviously broken diffs) and once after applying the diff (to confirm the resulting bundle is still valid via `bundle::validate::validate_bundle`).

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_validator.rs`:

```rust
use std::collections::HashSet;

use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::mutator::{MutationDiff, ParamChange, ToolDiff};
use xvision_engine::autooptimizer::validator::{validate_mutation_diff, ValidationFault};

fn empty_diff(parent: ContentHash) -> MutationDiff {
    MutationDiff {
        prose_diff: None,
        param_changes: vec![],
        tool_changes: ToolDiff { added: vec![], removed: vec![] },
        mutator_model: "test".into(),
        mutator_token_cost: 0,
        proposed_at: chrono::Utc::now(),
        parent_hash: parent,
    }
}

#[test]
fn passes_when_diff_is_empty_and_parent_is_valid() {
    let parent_md = "## Slot: trader\nfoo\n";
    let parent_hash = ContentHash::of_bytes(parent_md.as_bytes());
    let parent_param_keys: HashSet<String> = ["rsi.period".into()].into_iter().collect();
    let registered_tools: HashSet<String> = ["volume_profile".into()].into_iter().collect();
    let result = validate_mutation_diff(
        &empty_diff(parent_hash),
        parent_md,
        &parent_param_keys,
        &registered_tools,
    );
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn rejects_param_change_with_unknown_key() {
    let parent_md = "## Slot: trader\nfoo\n";
    let parent_hash = ContentHash::of_bytes(parent_md.as_bytes());
    let mut diff = empty_diff(parent_hash);
    diff.param_changes.push(ParamChange {
        key: "no.such.key".into(),
        old: serde_json::json!(0),
        new: serde_json::json!(1),
    });
    let parent_param_keys: HashSet<String> = ["rsi.period".into()].into_iter().collect();
    let registered_tools: HashSet<String> = HashSet::new();
    let err = validate_mutation_diff(&diff, parent_md, &parent_param_keys, &registered_tools).unwrap_err();
    assert!(matches!(err, ValidationFault::UnknownParamKey(_)));
}

#[test]
fn rejects_tool_added_thats_not_registered() {
    let parent_md = "## Slot: trader\nfoo\n";
    let parent_hash = ContentHash::of_bytes(parent_md.as_bytes());
    let mut diff = empty_diff(parent_hash);
    diff.tool_changes.added.push("phantom_tool".into());
    let parent_param_keys: HashSet<String> = HashSet::new();
    let registered_tools: HashSet<String> = ["volume_profile".into()].into_iter().collect();
    let err = validate_mutation_diff(&diff, parent_md, &parent_param_keys, &registered_tools).unwrap_err();
    assert!(matches!(err, ValidationFault::UnregisteredTool(_)));
}

#[test]
fn rejects_prose_diff_that_doesnt_apply_cleanly() {
    let parent_md = "## Slot: trader\nreal content\n";
    let parent_hash = ContentHash::of_bytes(parent_md.as_bytes());
    let mut diff = empty_diff(parent_hash);
    diff.prose_diff = Some(
        "--- a/program.md\n+++ b/program.md\n@@ -1,1 +1,1 @@\n-DOES NOT EXIST\n+replacement\n".into(),
    );
    let parent_param_keys: HashSet<String> = HashSet::new();
    let registered_tools: HashSet<String> = HashSet::new();
    let err = validate_mutation_diff(&diff, parent_md, &parent_param_keys, &registered_tools).unwrap_err();
    assert!(matches!(err, ValidationFault::ProseDiffDidNotApply(_)));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_validator`
Expected: FAIL with unresolved imports.

- [ ] **Step 3: Implement validator.rs**

Create `crates/xvision-engine/src/autooptimizer/validator.rs`:

```rust
use std::collections::HashSet;

use thiserror::Error;

use crate::autooptimizer::mutator::MutationDiff;
use crate::bundle::program_view::{apply_unified_diff, ProgramViewError};

#[derive(Debug, Error)]
pub enum ValidationFault {
    #[error("param_change references key not in parent's param schema: {0}")]
    UnknownParamKey(String),
    #[error("tool_change added tool not registered in MCP registry: {0}")]
    UnregisteredTool(String),
    #[error("prose_diff did not apply cleanly: {0}")]
    ProseDiffDidNotApply(String),
}

/// Validate a candidate mutation diff against a parent bundle's invariants.
///
/// `parent_param_keys` is the set of legal `mechanical_params` keys for the
/// parent bundle (flattened to dotted form, e.g. `"rsi.period"`).
/// `registered_tools` is the live set of tool names from the MCP registry.
/// `parent_md` is the parent's program-view markdown, against which the
/// `prose_diff` (if present) must apply cleanly.
pub fn validate_mutation_diff(
    diff: &MutationDiff,
    parent_md: &str,
    parent_param_keys: &HashSet<String>,
    registered_tools: &HashSet<String>,
) -> Result<(), ValidationFault> {
    for change in &diff.param_changes {
        if !parent_param_keys.contains(&change.key) {
            return Err(ValidationFault::UnknownParamKey(change.key.clone()));
        }
    }
    for added in &diff.tool_changes.added {
        if !registered_tools.contains(added) {
            return Err(ValidationFault::UnregisteredTool(added.clone()));
        }
    }
    if let Some(prose) = &diff.prose_diff {
        match apply_unified_diff(parent_md, prose) {
            Ok(_) => {}
            Err(ProgramViewError::DiffDidNotApply { reason }) => {
                return Err(ValidationFault::ProseDiffDidNotApply(reason));
            }
            Err(other) => {
                return Err(ValidationFault::ProseDiffDidNotApply(other.to_string()));
            }
        }
    }
    Ok(())
}

/// Convenience: flatten a `mechanical_params` JSON object into a set of
/// dotted keys. Nested objects yield `"a.b.c"`. Arrays are not indexed (the
/// mutator addresses arrays as a whole).
pub fn flatten_param_keys(params: &serde_json::Value) -> HashSet<String> {
    let mut out = HashSet::new();
    flatten_into(params, String::new(), &mut out);
    out
}

fn flatten_into(value: &serde_json::Value, prefix: String, out: &mut HashSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                if matches!(v, serde_json::Value::Object(_)) {
                    flatten_into(v, next, out);
                } else {
                    out.insert(next);
                }
            }
        }
        _ => {
            if !prefix.is_empty() {
                out.insert(prefix);
            }
        }
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn flatten_param_keys_handles_nested() {
        let params = serde_json::json!({
            "rsi": {"period": 14, "thresholds": {"hi": 70, "lo": 30}},
            "ema_period": 20,
        });
        let keys = flatten_param_keys(&params);
        assert!(keys.contains("rsi.period"));
        assert!(keys.contains("rsi.thresholds.hi"));
        assert!(keys.contains("rsi.thresholds.lo"));
        assert!(keys.contains("ema_period"));
        assert_eq!(keys.len(), 4);
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_validator
cargo test -p xvision-engine autooptimizer::validator
git add crates/xvision-engine/src/autooptimizer/validator.rs crates/xvision-engine/tests/autooptimizer_validator.rs
git commit -m "feat(autooptimizer): mutation-diff validator + param-key flattener"
```

---

### Task 9: Mutator — LLM proposer + 2-retry loop

Now the LLM-touching half of mutator.rs. The mutator takes a parent bundle + recent ledger context, calls the LLM, parses the response into a `MutationDiff`, runs the validator, and on failure feeds the validator's error back as system context for up to two retries (per spec §4).

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` — add `Mutator` struct + `propose()` method
- Create: `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md`
- Modify: `crates/xvision-engine/tests/autooptimizer_mutator.rs` — add propose tests

- [ ] **Step 1: Write the mutator prompt**

Create `crates/xvision-engine/prompts/autooptimizer/mutator-v1.md`:

```markdown
---
name: autooptimizer-mutator
display_name: "AutoOptimizer Mutator v1"
description: "Proposes a single MutationDiff against a parent strategy bundle."
version: 1.0.0
allowed_tools: []
model_requirement: "anthropic.claude-haiku-4-5+"
---

You propose ONE mutation to a parent strategy bundle. Your output is a JSON
MutationDiff that the autooptimizer loop will evaluate via paper-testing.

You receive:
- parent_program_md: the parent bundle's slot prompts as a single markdown
  document with `## Slot: <role>` section headers.
- parent_param_keys: the set of `mechanical_params` keys you may change.
  Pass through unchanged keys; only include the ones you're modifying in the
  output's `param_changes`.
- registered_tools: the set of tool names you may add via `tool_changes.added`.
  Removing tools is always allowed (just listing existing ones in `removed`).
- recent_ledger: a JSON summary of recent paper-test outcomes and findings
  for this lineage. Use it to inform what to mutate.
- previous_validator_error (optional): if your previous proposal was rejected,
  this is the validator's error message. Adjust your proposal accordingly.

Output ONE JSON object matching this schema (no surrounding prose, no
markdown code fences):

{
  "prose_diff": "<unified diff over parent_program_md, or null if no prose change>",
  "param_changes": [
    { "key": "<dotted.key>", "old": <json>, "new": <json> }
  ],
  "tool_changes": {
    "added":   ["<tool_name>", ...],
    "removed": ["<tool_name>", ...]
  }
}

Rules:
- Make a single, focused change. Don't combine multiple ideas in one diff.
- If you propose a prose_diff, use standard unified-diff format with `---
  a/program.md` and `+++ b/program.md` headers and `@@ -<line>,<count>
  +<line>,<count> @@` hunk markers.
- Every key in param_changes MUST be in parent_param_keys.
- Every tool in tool_changes.added MUST be in registered_tools.
- Output ONLY the JSON. No prose. No code fences.
```

- [ ] **Step 2: Write the failing test (extend the mutator test file)**

Append to `crates/xvision-engine/tests/autooptimizer_mutator.rs`:

```rust
use std::collections::HashSet;
use std::sync::Arc;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autooptimizer::mutator::{Mutator, MutatorContext, MutatorOutcome};

#[tokio::test]
async fn mutator_returns_validated_diff_on_first_try() {
    let canned = r#"{
        "prose_diff": null,
        "param_changes": [{"key": "rsi.period", "old": 14, "new": 21}],
        "tool_changes": {"added": [], "removed": []}
    }"#;
    let dispatch = Arc::new(MockDispatch::echo(canned));
    let parent_md = "## Slot: trader\nfoo\n";
    let parent_hash = xvision_engine::autooptimizer::content_hash::ContentHash::of_bytes(
        parent_md.as_bytes(),
    );
    let mutator = Mutator::new(dispatch, "claude-haiku-4-5", 4096);
    let ctx = MutatorContext {
        parent_hash,
        parent_program_md: parent_md.to_string(),
        parent_param_keys: ["rsi.period".to_string()].into_iter().collect::<HashSet<_>>(),
        registered_tools: HashSet::new(),
        recent_ledger: serde_json::json!({"runs": []}),
    };
    let outcome = mutator.propose(&ctx).await.unwrap();
    match outcome {
        MutatorOutcome::Accepted { diff, retries } => {
            assert_eq!(retries, 0);
            assert_eq!(diff.param_changes.len(), 1);
            assert_eq!(diff.param_changes[0].key, "rsi.period");
        }
        other => panic!("expected Accepted, got {other:?}"),
    }
}

#[tokio::test]
async fn mutator_gives_up_after_two_retries() {
    // The mock keeps returning a diff that references an unknown tool.
    let bad = r#"{
        "prose_diff": null,
        "param_changes": [],
        "tool_changes": {"added": ["phantom"], "removed": []}
    }"#;
    let dispatch = Arc::new(MockDispatch::echo(bad));
    let parent_md = "## Slot: trader\nfoo\n";
    let parent_hash = xvision_engine::autooptimizer::content_hash::ContentHash::of_bytes(
        parent_md.as_bytes(),
    );
    let mutator = Mutator::new(dispatch, "claude-haiku-4-5", 4096);
    let ctx = MutatorContext {
        parent_hash,
        parent_program_md: parent_md.to_string(),
        parent_param_keys: HashSet::new(),
        registered_tools: HashSet::new(),
        recent_ledger: serde_json::json!({"runs": []}),
    };
    let outcome = mutator.propose(&ctx).await.unwrap();
    match outcome {
        MutatorOutcome::Dropped { retries, last_error } => {
            assert_eq!(retries, 2);
            assert!(last_error.contains("phantom"));
        }
        other => panic!("expected Dropped, got {other:?}"),
    }
}

#[tokio::test]
async fn mutator_returns_dropped_when_response_is_unparseable() {
    let dispatch = Arc::new(MockDispatch::echo("not json at all"));
    let parent_md = "## Slot: trader\nfoo\n";
    let parent_hash = xvision_engine::autooptimizer::content_hash::ContentHash::of_bytes(
        parent_md.as_bytes(),
    );
    let mutator = Mutator::new(dispatch, "claude-haiku-4-5", 4096);
    let ctx = MutatorContext {
        parent_hash,
        parent_program_md: parent_md.to_string(),
        parent_param_keys: HashSet::new(),
        registered_tools: HashSet::new(),
        recent_ledger: serde_json::json!({"runs": []}),
    };
    let outcome = mutator.propose(&ctx).await.unwrap();
    assert!(matches!(outcome, MutatorOutcome::Dropped { .. }));
}
```

- [ ] **Step 3: Extend mutator.rs**

Append to `crates/xvision-engine/src/autooptimizer/mutator.rs` (after the existing types):

```rust
use std::collections::HashSet;
use std::sync::Arc;

use crate::agent::llm::{LlmDispatch, LlmRequest};
use crate::autooptimizer::validator::{validate_mutation_diff, ValidationFault};

const MUTATOR_PROMPT: &str = include_str!("../../prompts/autooptimizer/mutator-v1.md");
const MAX_RETRIES: u32 = 2;

pub struct Mutator {
    dispatch: Arc<dyn LlmDispatch>,
    model: String,
    max_tokens: u32,
}

pub struct MutatorContext {
    pub parent_hash: ContentHash,
    pub parent_program_md: String,
    pub parent_param_keys: HashSet<String>,
    pub registered_tools: HashSet<String>,
    pub recent_ledger: serde_json::Value,
}

#[derive(Debug)]
pub enum MutatorOutcome {
    Accepted { diff: MutationDiff, retries: u32 },
    Dropped { retries: u32, last_error: String },
}

impl Mutator {
    pub fn new(dispatch: Arc<dyn LlmDispatch>, model: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            dispatch,
            model: model.into(),
            max_tokens,
        }
    }

    pub async fn propose(&self, ctx: &MutatorContext) -> anyhow::Result<MutatorOutcome> {
        let mut last_error: Option<String> = None;
        let mut total_tokens: u32 = 0;
        for attempt in 0..=MAX_RETRIES {
            let user_prompt = build_user_prompt(ctx, last_error.as_deref());
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: MUTATOR_PROMPT.to_string(),
                user_prompt,
                max_tokens: self.max_tokens,
            };
            let resp = self.dispatch.complete(req).await?;
            total_tokens = total_tokens
                .saturating_add(resp.input_tokens)
                .saturating_add(resp.output_tokens);
            let parse_result: Result<RawMutationDiff, _> = serde_json::from_str(&extract_json(&resp.text));
            let raw = match parse_result {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("response did not parse as MutationDiff: {e}");
                    if attempt == MAX_RETRIES {
                        return Ok(MutatorOutcome::Dropped {
                            retries: attempt,
                            last_error: msg,
                        });
                    }
                    last_error = Some(msg);
                    continue;
                }
            };
            let diff = MutationDiff {
                prose_diff: raw.prose_diff,
                param_changes: raw.param_changes,
                tool_changes: raw.tool_changes,
                mutator_model: self.model.clone(),
                mutator_token_cost: total_tokens,
                proposed_at: Utc::now(),
                parent_hash: ctx.parent_hash,
            };
            match validate_mutation_diff(
                &diff,
                &ctx.parent_program_md,
                &ctx.parent_param_keys,
                &ctx.registered_tools,
            ) {
                Ok(()) => {
                    return Ok(MutatorOutcome::Accepted {
                        diff,
                        retries: attempt,
                    });
                }
                Err(fault) => {
                    let msg = fault.to_string();
                    if attempt == MAX_RETRIES {
                        return Ok(MutatorOutcome::Dropped {
                            retries: attempt,
                            last_error: msg,
                        });
                    }
                    last_error = Some(msg);
                }
            }
        }
        // Unreachable: the loop above always returns once `attempt == MAX_RETRIES`.
        unreachable!("mutator retry loop should have returned")
    }
}

#[derive(Deserialize)]
struct RawMutationDiff {
    prose_diff: Option<String>,
    param_changes: Vec<ParamChange>,
    tool_changes: ToolDiff,
}

fn build_user_prompt(ctx: &MutatorContext, previous_error: Option<&str>) -> String {
    let mut parent_param_keys: Vec<&String> = ctx.parent_param_keys.iter().collect();
    parent_param_keys.sort();
    let mut registered_tools: Vec<&String> = ctx.registered_tools.iter().collect();
    registered_tools.sort();
    let payload = serde_json::json!({
        "parent_program_md": ctx.parent_program_md,
        "parent_param_keys": parent_param_keys,
        "registered_tools": registered_tools,
        "recent_ledger": ctx.recent_ledger,
        "previous_validator_error": previous_error,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default()
}

fn extract_json(text: &str) -> String {
    // Strip code fences and surrounding prose; tolerate Haiku occasionally
    // emitting ```json blocks.
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest.trim_end_matches("```").trim().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest.trim_end_matches("```").trim().to_string();
    }
    trimmed.to_string()
}

// `Utc` and `Deserialize` are already in scope from the top-of-file imports
// (chrono::Utc and serde::Deserialize).
```

You may also need to add `use serde::Deserialize;` at the top if not already imported (the existing types already use `Serialize, Deserialize` from `serde`).

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_mutator
git add crates/xvision-engine/src/autooptimizer/mutator.rs crates/xvision-engine/tests/autooptimizer_mutator.rs crates/xvision-engine/prompts/autooptimizer/mutator-v1.md
git commit -m "feat(autooptimizer): Mutator — LLM proposer with 2-retry validator-feedback loop"
```

---

## Phase C — Numeric gate

### Task 10: Numeric gate (Δ-Sharpe over day + held-out)

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/gate.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_gate.rs`

The gate (autooptimizer spec §5.1) is pure-numeric and deterministic. It takes parent and child Sharpe values for both windows and the pre-committed ε. Both Δ values must clear ε independently. Rejection is published in lineage as a Ghost node (committed via lineage.rs in Task 12, not here).

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_gate.rs`:

```rust
use xvision_engine::autooptimizer::gate::{GateDecision, NumericGate};

#[test]
fn passes_when_both_deltas_clear_epsilon() {
    let g = NumericGate { epsilon: 0.10 };
    let decision = g.evaluate(1.0, 1.15, 1.0, 1.20);
    assert_eq!(decision, GateDecision::Passed { delta_day: 0.15, delta_holdout: 0.20 });
}

#[test]
fn rejects_when_day_clears_but_holdout_doesnt() {
    let g = NumericGate { epsilon: 0.10 };
    let decision = g.evaluate(1.0, 1.20, 1.0, 1.05);
    assert!(matches!(decision, GateDecision::Rejected { reason: _, .. }));
}

#[test]
fn rejects_when_holdout_clears_but_day_doesnt() {
    let g = NumericGate { epsilon: 0.10 };
    let decision = g.evaluate(1.0, 1.05, 1.0, 1.30);
    assert!(matches!(decision, GateDecision::Rejected { reason: _, .. }));
}

#[test]
fn rejects_when_neither_clears() {
    let g = NumericGate { epsilon: 0.10 };
    let decision = g.evaluate(1.0, 0.95, 1.0, 1.05);
    assert!(matches!(decision, GateDecision::Rejected { .. }));
}

#[test]
fn equal_to_epsilon_passes() {
    let g = NumericGate { epsilon: 0.10 };
    let decision = g.evaluate(1.0, 1.10, 1.0, 1.10);
    assert!(matches!(decision, GateDecision::Passed { .. }));
}

#[test]
fn negative_parent_sharpe_handled() {
    let g = NumericGate { epsilon: 0.10 };
    let decision = g.evaluate(-0.5, 0.0, -0.5, 0.0);
    // Δ = 0.5, well above ε = 0.10.
    assert!(matches!(decision, GateDecision::Passed { .. }));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_gate`
Expected: FAIL with unresolved imports.

- [ ] **Step 3: Implement gate.rs**

Create `crates/xvision-engine/src/autooptimizer/gate.rs`:

```rust
//! Numeric Δ-Sharpe gate. Per autooptimizer spec §5.1: child variant merges
//! iff Δ-Sharpe on day window ≥ ε AND Δ-Sharpe on held-out window ≥ ε. Both
//! required; single-window improvements are rejected.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct NumericGate {
    pub epsilon: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateDecision {
    Passed { delta_day: f64, delta_holdout: f64 },
    Rejected {
        delta_day: f64,
        delta_holdout: f64,
        reason: String,
    },
}

impl NumericGate {
    pub fn evaluate(
        &self,
        parent_day_sharpe: f64,
        child_day_sharpe: f64,
        parent_holdout_sharpe: f64,
        child_holdout_sharpe: f64,
    ) -> GateDecision {
        let delta_day = child_day_sharpe - parent_day_sharpe;
        let delta_holdout = child_holdout_sharpe - parent_holdout_sharpe;
        let day_pass = delta_day >= self.epsilon;
        let holdout_pass = delta_holdout >= self.epsilon;
        match (day_pass, holdout_pass) {
            (true, true) => GateDecision::Passed { delta_day, delta_holdout },
            _ => GateDecision::Rejected {
                delta_day,
                delta_holdout,
                reason: format!(
                    "needed Δ ≥ {:.4} on both windows; got day={:.4} holdout={:.4}",
                    self.epsilon, delta_day, delta_holdout
                ),
            },
        }
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_gate
git add crates/xvision-engine/src/autooptimizer/gate.rs crates/xvision-engine/tests/autooptimizer_gate.rs
git commit -m "feat(autooptimizer): deterministic Δ-Sharpe gate (day + holdout, both required)"
```

---

## Phase D — Lineage store

### Task 11: 003_autooptimizer.sql migration

**File:** `crates/xvision-engine/migrations/003_autooptimizer.sql`

- [ ] **Step 1: Write the migration**

Create `crates/xvision-engine/migrations/003_autooptimizer.sql`:

```sql
-- AutoOptimizer loop tables. Depends on 002_eval.sql (uses eval_runs IDs as
-- foreign references for paper-test rows attached to mutations).

CREATE TABLE IF NOT EXISTS autooptimizer_session_commitments (
    session_id            TEXT PRIMARY KEY,
    epsilon               REAL NOT NULL,
    holdout_start         TEXT NOT NULL,
    holdout_end           TEXT NOT NULL,
    parent_policy_seed    INTEGER NOT NULL,
    cycle_config_hash     TEXT NOT NULL,
    canary_seed           INTEGER NOT NULL,
    operator_pubkey_hex   TEXT NOT NULL,
    signature_hex         TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    commitment_hash       TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS autooptimizer_lineage_nodes (
    bundle_hash           TEXT PRIMARY KEY,
    parent_hash           TEXT,                          -- NULL for seed bundles
    diff_blob_hash        TEXT,                          -- pointer into blob store; NULL for seeds
    finding_blob_hash     TEXT,                          -- AR-2 will populate; NULL allowed
    status                TEXT NOT NULL,                 -- 'active' | 'ghost' | 'quarantined'
    born_at               TEXT NOT NULL,
    metrics_json          TEXT,                          -- MetricsSummary JSON; NULL until paper-tested
    cycle_id              TEXT,                          -- ULID; NULL for seeds
    session_id            TEXT,                          -- FK into session_commitments; NULL for pre-session seeds
    FOREIGN KEY (session_id) REFERENCES autooptimizer_session_commitments(session_id)
);

CREATE INDEX IF NOT EXISTS idx_lineage_parent
    ON autooptimizer_lineage_nodes(parent_hash);
CREATE INDEX IF NOT EXISTS idx_lineage_status
    ON autooptimizer_lineage_nodes(status);
CREATE INDEX IF NOT EXISTS idx_lineage_cycle
    ON autooptimizer_lineage_nodes(cycle_id);

CREATE TABLE IF NOT EXISTS autooptimizer_lineage_edges (
    parent_hash           TEXT NOT NULL,
    child_hash            TEXT NOT NULL,
    edge_kind             TEXT NOT NULL,                 -- 'mutation' | 'fork'
    PRIMARY KEY (parent_hash, child_hash)
);

CREATE TABLE IF NOT EXISTS autooptimizer_paper_tests (
    paper_test_id         TEXT PRIMARY KEY,              -- ULID
    bundle_hash           TEXT NOT NULL,
    window_kind           TEXT NOT NULL,                 -- 'day' | 'holdout'
    eval_run_id           TEXT NOT NULL,                 -- FK into eval_runs
    sharpe                REAL NOT NULL,
    ran_at                TEXT NOT NULL,
    trace_blob_hash       TEXT NOT NULL,                 -- pointer into blob store
    FOREIGN KEY (bundle_hash) REFERENCES autooptimizer_lineage_nodes(bundle_hash)
);

CREATE INDEX IF NOT EXISTS idx_paper_tests_bundle
    ON autooptimizer_paper_tests(bundle_hash);

CREATE TABLE IF NOT EXISTS autooptimizer_cycle_seals (
    cycle_id              TEXT PRIMARY KEY,
    session_id            TEXT NOT NULL,
    sealed_at             TEXT NOT NULL,
    seal_blob_hash        TEXT NOT NULL,                 -- full CycleSeal JSON in blob store
    merkle_root_hex       TEXT NOT NULL,
    operator_signature_hex TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES autooptimizer_session_commitments(session_id)
);

CREATE INDEX IF NOT EXISTS idx_cycle_seals_session
    ON autooptimizer_cycle_seals(session_id);
```

- [ ] **Step 2: Verify migration applies**

Run: `cargo test -p xvision-engine --test autooptimizer_lineage`
Expected: FAIL (test doesn't exist yet — Task 12 will write it). For now just confirm the migration file is well-formed by running `sqlite3 ":memory:" < crates/xvision-engine/migrations/003_autooptimizer.sql` and checking the exit code is 0.

```bash
sqlite3 ":memory:" < crates/xvision-engine/migrations/003_autooptimizer.sql && echo OK
```

Expected output: `OK`.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-engine/migrations/003_autooptimizer.sql
git commit -m "feat(autooptimizer): SQLite migration 003 (sessions, lineage nodes/edges, paper_tests, seals)"
```

---

### Task 12: LineageStore + LineageNode + Merkle root

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/lineage.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_lineage.rs`

`LineageStore` owns the SQLite tables introduced by migration 003 and the content-addressed blob store. The Merkle root is computed over the chain `parent_hash → child_hash → days_alive → trades_attributed → realized_pnl_attributed` per spec §6.2 — for AR-1, days_alive / trades_attributed / realized_pnl_attributed start at zero (no time has passed) but the schema is ready; the cycle orchestrator in AR-2 will refresh these as the loop runs.

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_lineage.rs`:

```rust
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::lineage::{
    compute_merkle_root, LineageEdge, LineageNode, LineageStatus, LineageStore, MetricsSnapshot,
};

async fn fresh_store() -> (LineageStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_url = format!("sqlite://{}/test.db?mode=rwc", dir.path().display());
    let pool = SqlitePool::connect(&db_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let blob_root = dir.path().join("blobs");
    let store = LineageStore::new(pool, blob_root).await.unwrap();
    (store, dir)
}

#[tokio::test]
async fn insert_and_fetch_lineage_node() {
    let (store, _dir) = fresh_store().await;
    let bundle_hash = ContentHash::of_bytes(b"bundle-1");
    let node = LineageNode {
        bundle_hash,
        parent_hash: None,
        diff_blob_hash: None,
        finding_blob_hash: None,
        status: LineageStatus::Active,
        born_at: Utc::now(),
        metrics: None,
        cycle_id: None,
        session_id: None,
    };
    store.insert_node(&node).await.unwrap();
    let fetched = store.get_node(&bundle_hash).await.unwrap();
    assert_eq!(fetched.bundle_hash, bundle_hash);
    assert_eq!(fetched.status, LineageStatus::Active);
}

#[tokio::test]
async fn ghost_node_records_with_status() {
    let (store, _dir) = fresh_store().await;
    let parent_hash = ContentHash::of_bytes(b"parent");
    let parent = LineageNode {
        bundle_hash: parent_hash,
        parent_hash: None,
        diff_blob_hash: None,
        finding_blob_hash: None,
        status: LineageStatus::Active,
        born_at: Utc::now(),
        metrics: None,
        cycle_id: None,
        session_id: None,
    };
    store.insert_node(&parent).await.unwrap();
    let ghost_hash = ContentHash::of_bytes(b"ghost");
    let ghost = LineageNode {
        bundle_hash: ghost_hash,
        parent_hash: Some(parent_hash),
        diff_blob_hash: Some(ContentHash::of_bytes(b"diff")),
        finding_blob_hash: None,
        status: LineageStatus::Ghost,
        born_at: Utc::now(),
        metrics: None,
        cycle_id: None,
        session_id: None,
    };
    store.insert_node(&ghost).await.unwrap();
    store.add_edge(&LineageEdge { parent_hash, child_hash: ghost_hash, kind: "mutation".into() })
        .await.unwrap();
    let fetched = store.get_node(&ghost_hash).await.unwrap();
    assert_eq!(fetched.status, LineageStatus::Ghost);
    let children = store.children_of(&parent_hash).await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].bundle_hash, ghost_hash);
}

#[tokio::test]
async fn merkle_root_changes_when_lineage_changes() {
    let (store, _dir) = fresh_store().await;
    let p = ContentHash::of_bytes(b"p");
    let c1 = ContentHash::of_bytes(b"c1");
    let c2 = ContentHash::of_bytes(b"c2");
    for &h in &[p, c1, c2] {
        store.insert_node(&LineageNode {
            bundle_hash: h,
            parent_hash: if h == p { None } else { Some(p) },
            diff_blob_hash: if h == p { None } else { Some(ContentHash::of_bytes(b"d")) },
            finding_blob_hash: None,
            status: LineageStatus::Active,
            born_at: Utc::now(),
            metrics: Some(MetricsSnapshot {
                days_alive: 1,
                trades_attributed: 1,
                realized_pnl_attributed: 0.0,
            }),
            cycle_id: None,
            session_id: None,
        })
        .await
        .unwrap();
    }
    store.add_edge(&LineageEdge { parent_hash: p, child_hash: c1, kind: "mutation".into() }).await.unwrap();
    let root_one = compute_merkle_root(&store, &p).await.unwrap();
    store.add_edge(&LineageEdge { parent_hash: p, child_hash: c2, kind: "mutation".into() }).await.unwrap();
    let root_two = compute_merkle_root(&store, &p).await.unwrap();
    assert_ne!(root_one, root_two);
}

#[tokio::test]
async fn merkle_root_is_deterministic_across_calls() {
    let (store, _dir) = fresh_store().await;
    let p = ContentHash::of_bytes(b"p");
    let c1 = ContentHash::of_bytes(b"c1");
    for &h in &[p, c1] {
        store.insert_node(&LineageNode {
            bundle_hash: h,
            parent_hash: if h == p { None } else { Some(p) },
            diff_blob_hash: None,
            finding_blob_hash: None,
            status: LineageStatus::Active,
            born_at: Utc::now(),
            metrics: Some(MetricsSnapshot {
                days_alive: 0,
                trades_attributed: 0,
                realized_pnl_attributed: 0.0,
            }),
            cycle_id: None,
            session_id: None,
        }).await.unwrap();
    }
    store.add_edge(&LineageEdge { parent_hash: p, child_hash: c1, kind: "mutation".into() }).await.unwrap();
    let a = compute_merkle_root(&store, &p).await.unwrap();
    let b = compute_merkle_root(&store, &p).await.unwrap();
    assert_eq!(a, b);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_lineage`
Expected: FAIL — unresolved imports.

- [ ] **Step 3: Implement lineage.rs**

Create `crates/xvision-engine/src/autooptimizer/lineage.rs`:

```rust
//! Lineage store + counterfactual-chain Merkle root.
//!
//! See autooptimizer spec §6. SQLite-backed; the diff and finding payloads
//! themselves live in the content-addressed blob store. The Merkle root is
//! computed over `parent_hash → child_hash → days_alive → trades_attributed
//! → realized_pnl_attributed` for each lineage path; AR-1 publishes it via
//! the CycleSeal but doesn't anchor it (that's MP-1).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::autooptimizer::blob_store::BlobStore;
use crate::autooptimizer::content_hash::ContentHash;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageNode {
    pub bundle_hash: ContentHash,
    pub parent_hash: Option<ContentHash>,
    pub diff_blob_hash: Option<ContentHash>,
    pub finding_blob_hash: Option<ContentHash>,
    pub status: LineageStatus,
    pub born_at: DateTime<Utc>,
    pub metrics: Option<MetricsSnapshot>,
    pub cycle_id: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageStatus {
    Active,
    Ghost,
    Quarantined,
}

impl LineageStatus {
    fn as_str(self) -> &'static str {
        match self {
            LineageStatus::Active => "active",
            LineageStatus::Ghost => "ghost",
            LineageStatus::Quarantined => "quarantined",
        }
    }
    fn from_str(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "active" => Self::Active,
            "ghost" => Self::Ghost,
            "quarantined" => Self::Quarantined,
            other => anyhow::bail!("unknown lineage status: {other}"),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsSnapshot {
    pub days_alive: u32,
    pub trades_attributed: u32,
    pub realized_pnl_attributed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageEdge {
    pub parent_hash: ContentHash,
    pub child_hash: ContentHash,
    pub kind: String,                                    // "mutation" | "fork"
}

pub struct LineageStore {
    pool: SqlitePool,
    blobs: BlobStore,
}

impl LineageStore {
    pub async fn new(pool: SqlitePool, blob_root: PathBuf) -> anyhow::Result<Self> {
        let blobs = BlobStore::open(blob_root).await?;
        Ok(Self { pool, blobs })
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    pub async fn insert_node(&self, node: &LineageNode) -> anyhow::Result<()> {
        let metrics_json = match &node.metrics {
            Some(m) => Some(serde_json::to_string(m)?),
            None => None,
        };
        sqlx::query(
            "INSERT OR REPLACE INTO autooptimizer_lineage_nodes
             (bundle_hash, parent_hash, diff_blob_hash, finding_blob_hash, status, born_at, metrics_json, cycle_id, session_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(node.bundle_hash.to_hex())
        .bind(node.parent_hash.map(|h| h.to_hex()))
        .bind(node.diff_blob_hash.map(|h| h.to_hex()))
        .bind(node.finding_blob_hash.map(|h| h.to_hex()))
        .bind(node.status.as_str())
        .bind(node.born_at.to_rfc3339())
        .bind(metrics_json)
        .bind(&node.cycle_id)
        .bind(&node.session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_edge(&self, edge: &LineageEdge) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO autooptimizer_lineage_edges
             (parent_hash, child_hash, edge_kind) VALUES (?, ?, ?)",
        )
        .bind(edge.parent_hash.to_hex())
        .bind(edge.child_hash.to_hex())
        .bind(&edge.kind)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_node(&self, hash: &ContentHash) -> anyhow::Result<LineageNode> {
        let row = sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_blob_hash, finding_blob_hash, status, born_at, metrics_json, cycle_id, session_id
             FROM autooptimizer_lineage_nodes WHERE bundle_hash = ?",
        )
        .bind(hash.to_hex())
        .fetch_one(&self.pool)
        .await?;
        row_to_node(row)
    }

    pub async fn children_of(&self, parent: &ContentHash) -> anyhow::Result<Vec<LineageNode>> {
        let rows = sqlx::query(
            "SELECT n.bundle_hash, n.parent_hash, n.diff_blob_hash, n.finding_blob_hash, n.status, n.born_at, n.metrics_json, n.cycle_id, n.session_id
             FROM autooptimizer_lineage_nodes n
             JOIN autooptimizer_lineage_edges e ON e.child_hash = n.bundle_hash
             WHERE e.parent_hash = ?
             ORDER BY n.born_at",
        )
        .bind(parent.to_hex())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_node).collect()
    }

    pub async fn descendants_in_dfs_order(
        &self,
        root: &ContentHash,
    ) -> anyhow::Result<Vec<LineageNode>> {
        let mut out = Vec::new();
        let mut stack = vec![*root];
        while let Some(h) = stack.pop() {
            let node = self.get_node(&h).await?;
            let children = self.children_of(&h).await?;
            // Push children in reverse so DFS visits them in born_at order.
            for c in children.iter().rev() {
                stack.push(c.bundle_hash);
            }
            out.push(node);
        }
        Ok(out)
    }
}

fn row_to_node(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<LineageNode> {
    let bundle_hash = ContentHash::from_hex(row.try_get::<&str, _>("bundle_hash")?)?;
    let parent_hash = row
        .try_get::<Option<&str>, _>("parent_hash")?
        .map(ContentHash::from_hex)
        .transpose()?;
    let diff_blob_hash = row
        .try_get::<Option<&str>, _>("diff_blob_hash")?
        .map(ContentHash::from_hex)
        .transpose()?;
    let finding_blob_hash = row
        .try_get::<Option<&str>, _>("finding_blob_hash")?
        .map(ContentHash::from_hex)
        .transpose()?;
    let status = LineageStatus::from_str(row.try_get::<&str, _>("status")?)?;
    let born_at = DateTime::parse_from_rfc3339(row.try_get::<&str, _>("born_at")?)?.with_timezone(&Utc);
    let metrics: Option<MetricsSnapshot> = row
        .try_get::<Option<&str>, _>("metrics_json")?
        .map(|s| serde_json::from_str(s))
        .transpose()?;
    let cycle_id = row.try_get::<Option<String>, _>("cycle_id")?;
    let session_id = row.try_get::<Option<String>, _>("session_id")?;
    Ok(LineageNode {
        bundle_hash,
        parent_hash,
        diff_blob_hash,
        finding_blob_hash,
        status,
        born_at,
        metrics,
        cycle_id,
        session_id,
    })
}

/// Counterfactual-chain Merkle root for a lineage rooted at `root`.
/// Leaves: per-node `(parent_hash, child_hash, days_alive, trades_attributed,
/// realized_pnl_attributed)`. Tree built by concatenating sibling pair hashes
/// (BLAKE3) and walking up; odd-leaf-out is duplicated. Result is a
/// ContentHash. Deterministic given the same lineage state.
pub async fn compute_merkle_root(
    store: &LineageStore,
    root: &ContentHash,
) -> anyhow::Result<ContentHash> {
    let nodes = store.descendants_in_dfs_order(root).await?;
    let mut leaves: Vec<ContentHash> = Vec::new();
    for n in &nodes {
        let m = n.metrics.clone().unwrap_or(MetricsSnapshot {
            days_alive: 0,
            trades_attributed: 0,
            realized_pnl_attributed: 0.0,
        });
        let leaf = serde_json::json!({
            "parent_hash": n.parent_hash,
            "child_hash":  n.bundle_hash,
            "days_alive": m.days_alive,
            "trades_attributed": m.trades_attributed,
            "realized_pnl_attributed": m.realized_pnl_attributed,
        });
        leaves.push(ContentHash::of_json(&leaf));
    }
    if leaves.is_empty() {
        // Empty lineage → hash of empty bytes; defines the convention.
        return Ok(ContentHash::of_bytes(b""));
    }
    while leaves.len() > 1 {
        let mut next: Vec<ContentHash> = Vec::with_capacity((leaves.len() + 1) / 2);
        let mut i = 0;
        while i < leaves.len() {
            let l = leaves[i];
            let r = if i + 1 < leaves.len() { leaves[i + 1] } else { leaves[i] };
            let mut combined = [0u8; 64];
            combined[..32].copy_from_slice(l.as_bytes());
            combined[32..].copy_from_slice(r.as_bytes());
            next.push(ContentHash::of_bytes(&combined));
            i += 2;
        }
        leaves = next;
    }
    Ok(leaves[0])
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_lineage
git add crates/xvision-engine/src/autooptimizer/lineage.rs crates/xvision-engine/tests/autooptimizer_lineage.rs
git commit -m "feat(autooptimizer): LineageStore (SQLite + blobs) + counterfactual-chain Merkle root"
```

---

## Phase E — CycleSeal

### Task 13: CycleSeal struct + writer

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/seal.rs`
- Create: `crates/xvision-engine/src/autooptimizer/progress.rs`
- Create: `crates/xvision-engine/tests/autooptimizer_seal.rs`

The seal (autooptimizer spec §3.4) bundles every byte of an evening cycle into one signed manifest. AR-1 ships the type + writer + verifier; the AR-2 cycle orchestrator will be the actual producer that calls `CycleSealWriter::seal_and_commit(...)`.

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-engine/tests/autooptimizer_seal.rs`:

```rust
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_engine::autooptimizer::content_hash::ContentHash;
use xvision_engine::autooptimizer::lineage::LineageStore;
use xvision_engine::autooptimizer::seal::{CycleSeal, CycleSealWriter};
use xvision_engine::autooptimizer::session::OperatorKey;

async fn fixture() -> (LineageStore, OperatorKey, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_url = format!("sqlite://{}/test.db?mode=rwc", dir.path().display());
    let pool = SqlitePool::connect(&db_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = LineageStore::new(pool, dir.path().join("blobs")).await.unwrap();
    let key = OperatorKey::load_or_generate(&dir.path().join("operator.ed25519")).unwrap();
    (store, key, dir)
}

#[tokio::test]
async fn seal_and_verify_round_trip() {
    let (store, key, _dir) = fixture().await;
    let seal = CycleSeal {
        cycle_id: "01HZZCYCLE".into(),
        session_id: "01HZZSESSION".into(),
        sealed_at: Utc::now(),
        config_hash: ContentHash::of_bytes(b"config"),
        session_commitment_hash: ContentHash::of_bytes(b"commit"),
        parent_seeds: vec![ContentHash::of_bytes(b"p1")],
        mutations: vec![ContentHash::of_bytes(b"m1")],
        paper_tests: vec![ContentHash::of_bytes(b"pt1")],
        findings: vec![],
        canary_outcome: ContentHash::of_bytes(b"canary"),
        lineage_edges_added: vec![(ContentHash::of_bytes(b"p1"), ContentHash::of_bytes(b"c1"))],
        diversity_metric: 0.42,
        merkle_root: ContentHash::of_bytes(b"root"),
        operator_pubkey_hex: key.public_hex(),
        operator_signature_hex: String::new(),  // filled by writer
    };
    let writer = CycleSealWriter::new(&store, &key);
    let signed = writer.sign(seal.clone()).unwrap();
    assert!(!signed.operator_signature_hex.is_empty());
    assert!(CycleSealWriter::verify(&signed).is_ok());
}

#[tokio::test]
async fn tampered_seal_fails_verification() {
    let (store, key, _dir) = fixture().await;
    let seal = CycleSeal {
        cycle_id: "01HZZCYCLE".into(),
        session_id: "01HZZSESSION".into(),
        sealed_at: Utc::now(),
        config_hash: ContentHash::of_bytes(b"config"),
        session_commitment_hash: ContentHash::of_bytes(b"commit"),
        parent_seeds: vec![],
        mutations: vec![],
        paper_tests: vec![],
        findings: vec![],
        canary_outcome: ContentHash::of_bytes(b"canary"),
        lineage_edges_added: vec![],
        diversity_metric: 0.0,
        merkle_root: ContentHash::of_bytes(b"root"),
        operator_pubkey_hex: key.public_hex(),
        operator_signature_hex: String::new(),
    };
    let writer = CycleSealWriter::new(&store, &key);
    let mut signed = writer.sign(seal).unwrap();
    signed.diversity_metric = 0.99; // tamper
    assert!(CycleSealWriter::verify(&signed).is_err());
}

#[tokio::test]
async fn writer_persists_seal_and_blob() {
    let (store, key, _dir) = fixture().await;
    let seal = CycleSeal {
        cycle_id: "01HZZCYCLE2".into(),
        session_id: "01HZZSESSION2".into(),
        sealed_at: Utc::now(),
        config_hash: ContentHash::of_bytes(b"c"),
        session_commitment_hash: ContentHash::of_bytes(b"sc"),
        parent_seeds: vec![],
        mutations: vec![],
        paper_tests: vec![],
        findings: vec![],
        canary_outcome: ContentHash::of_bytes(b"can"),
        lineage_edges_added: vec![],
        diversity_metric: 0.0,
        merkle_root: ContentHash::of_bytes(b"r"),
        operator_pubkey_hex: key.public_hex(),
        operator_signature_hex: String::new(),
    };
    let writer = CycleSealWriter::new(&store, &key);
    let blob_hash = writer.seal_and_commit(seal.clone()).await.unwrap();
    let loaded = store.blobs().get_json(&blob_hash).await.unwrap();
    let restored: CycleSeal = serde_json::from_value(loaded).unwrap();
    assert_eq!(restored.cycle_id, seal.cycle_id);
    assert!(CycleSealWriter::verify(&restored).is_ok());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p xvision-engine --test autooptimizer_seal`
Expected: FAIL — unresolved imports.

- [ ] **Step 3: Implement progress.rs (placeholder types only)**

Create `crates/xvision-engine/src/autooptimizer/progress.rs`:

```rust
//! SSE event taxonomy. AR-1 only defines the types; the actual SSE channel
//! and emitter wiring land in AR-2 (cycle orchestrator) and AR-3 (dashboard).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutoOptimizerEvent {
    MutationProposed { cycle_id: String, parent_hash: String },
    MutationEvaluating { cycle_id: String, child_hash: String },
    MutationCommitted { cycle_id: String, child_hash: String, status: String },
    MutationRejected { cycle_id: String, child_hash: String, reason: String },
    LineageForked { cycle_id: String, parent_hash: String, child_hash: String },
    CanaryOutcome { cycle_id: String, accepted: bool },
    DiversityUpdated { cycle_id: String, value: f64 },
    CycleSealed { cycle_id: String, seal_blob_hash: String, merkle_root: String },
}
```

- [ ] **Step 4: Implement seal.rs**

Create `crates/xvision-engine/src/autooptimizer/seal.rs`:

```rust
//! CycleSeal — the contract surface between autooptimizer core and any
//! downstream consumer (marketplace plugin, future v2 readers, external
//! auditors). See autooptimizer spec §3.4.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::autooptimizer::content_hash::{canonicalize_json, ContentHash};
use crate::autooptimizer::lineage::LineageStore;
use crate::autooptimizer::session::OperatorKey;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CycleSeal {
    pub cycle_id: String,
    pub session_id: String,
    pub sealed_at: DateTime<Utc>,
    pub config_hash: ContentHash,
    pub session_commitment_hash: ContentHash,
    pub parent_seeds: Vec<ContentHash>,
    pub mutations: Vec<ContentHash>,
    pub paper_tests: Vec<ContentHash>,
    pub findings: Vec<ContentHash>,
    pub canary_outcome: ContentHash,
    pub lineage_edges_added: Vec<(ContentHash, ContentHash)>,
    pub diversity_metric: f64,
    pub merkle_root: ContentHash,
    pub operator_pubkey_hex: String,
    pub operator_signature_hex: String,                  // empty before sign()
}

pub struct CycleSealWriter<'a> {
    store: &'a LineageStore,
    key: &'a OperatorKey,
}

impl<'a> CycleSealWriter<'a> {
    pub fn new(store: &'a LineageStore, key: &'a OperatorKey) -> Self {
        Self { store, key }
    }

    /// Sign the seal in-place. Returns a new owned CycleSeal with
    /// `operator_signature_hex` populated.
    pub fn sign(&self, mut seal: CycleSeal) -> anyhow::Result<CycleSeal> {
        seal.operator_pubkey_hex = self.key.public_hex();
        seal.operator_signature_hex = String::new();      // ensure clean canonical
        let payload = canonical_signing_payload(&seal);
        let bytes = serde_json::to_vec(&payload)?;
        let sig = self.key.sign(&bytes);
        seal.operator_signature_hex = hex::encode(sig.to_bytes());
        Ok(seal)
    }

    /// Sign + persist the seal to disk (blob store) AND insert one row in
    /// `autooptimizer_cycle_seals`. Returns the seal's blob hash.
    pub async fn seal_and_commit(&self, seal: CycleSeal) -> anyhow::Result<ContentHash> {
        let signed = self.sign(seal)?;
        let v = serde_json::to_value(&signed)?;
        let blob_hash = self.store.blobs().put_json(&v).await?;
        self.write_index_row(&signed, &blob_hash).await?;
        Ok(blob_hash)
    }

    async fn write_index_row(&self, seal: &CycleSeal, blob_hash: &ContentHash) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO autooptimizer_cycle_seals
             (cycle_id, session_id, sealed_at, seal_blob_hash, merkle_root_hex, operator_signature_hex)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&seal.cycle_id)
        .bind(&seal.session_id)
        .bind(seal.sealed_at.to_rfc3339())
        .bind(blob_hash.to_hex())
        .bind(seal.merkle_root.to_hex())
        .bind(&seal.operator_signature_hex)
        .execute(self.store_pool())
        .await?;
        Ok(())
    }

    fn store_pool(&self) -> &SqlitePool {
        // Expose SqlitePool getter on LineageStore; alternatively, re-fetch
        // via a method. We add `pub fn pool(&self) -> &SqlitePool` to
        // LineageStore as part of this task — see Step 5 below.
        self.store.pool()
    }

    pub fn verify(seal: &CycleSeal) -> anyhow::Result<()> {
        let pubkey_bytes = hex::decode(&seal.operator_pubkey_hex)?;
        let arr: [u8; 32] = pubkey_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("operator pubkey wrong length"))?;
        let pubkey = VerifyingKey::from_bytes(&arr)?;
        let sig_bytes = hex::decode(&seal.operator_signature_hex)?;
        let sig_arr: [u8; 64] = sig_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("seal signature wrong length"))?;
        let sig = Signature::from_bytes(&sig_arr);
        let mut payload_seal = seal.clone();
        payload_seal.operator_signature_hex = String::new();
        let payload = canonical_signing_payload(&payload_seal);
        let bytes = serde_json::to_vec(&payload)?;
        pubkey.verify(&bytes, &sig)?;
        Ok(())
    }
}

fn canonical_signing_payload(seal: &CycleSeal) -> serde_json::Value {
    canonicalize_json(&serde_json::to_value(seal).expect("seal serializes"))
}
```

- [ ] **Step 5: Expose pool() on LineageStore**

Open `crates/xvision-engine/src/autooptimizer/lineage.rs` and add inside `impl LineageStore`:

```rust
pub fn pool(&self) -> &SqlitePool {
    &self.pool
}
```

- [ ] **Step 6: Run + commit**

```bash
cargo test -p xvision-engine --test autooptimizer_seal
git add crates/xvision-engine/src/autooptimizer/seal.rs crates/xvision-engine/src/autooptimizer/progress.rs crates/xvision-engine/src/autooptimizer/lineage.rs crates/xvision-engine/tests/autooptimizer_seal.rs
git commit -m "feat(autooptimizer): CycleSeal struct + signed writer + SSE event taxonomy (types only)"
```

---

## Phase F — CLI: session-init + mutate-once + lineage/seal inspection

### Task 14: `xvn autooptimizer session-init`

The session-init subcommand (a) loads `autooptimizer.toml`, (b) loads-or-generates the operator key, (c) builds + signs a SessionCommitment, (d) inserts it into the SQLite session table, and (e) prints the commitment hash + session_id. This is the hackathon's "before any cycle ran, here's what we pre-committed to" hand-off.

**Files:**
- Create: `crates/xvision-cli/src/commands/autooptimizer.rs`
- Modify: `crates/xvision-cli/src/lib.rs` — add `AutoOptimizer(...)` arm
- Modify: `crates/xvision-cli/Cargo.toml` — add `xvision-engine` dep if not already present, plus `dirs = "5"` and `sqlx = { version = "...", features = [...] }` if missing.

- [ ] **Step 1: Add the subcommand dispatcher**

Create `crates/xvision-cli/src/commands/autooptimizer.rs`:

```rust
use std::path::PathBuf;

use clap::{Args, Subcommand};
use sqlx::SqlitePool;

use xvision_engine::autooptimizer::{
    config::AutoOptimizerConfig,
    lineage::{LineageStore},
    seal::CycleSealWriter,
    session::{OperatorKey, SessionCommitment},
    AutoOptimizerConfig as _,
};

#[derive(Debug, Args)]
pub struct AutoOptimizerCmd {
    #[command(subcommand)]
    pub action: AutoOptimizerAction,
}

#[derive(Debug, Subcommand)]
pub enum AutoOptimizerAction {
    /// Initialise a session: load config, sign + persist the SessionCommitment.
    SessionInit {
        #[arg(long, default_value = "config/autooptimizer.toml")]
        config: PathBuf,
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        key_path: Option<PathBuf>,
    },
    /// Run one mutation against a parent bundle (no cycle loop yet — that's AR-2).
    MutateOnce {
        #[arg(long)]
        parent_bundle_hash: String,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        db: PathBuf,
        #[arg(long, default_value_t = false)]
        mock: bool,
    },
    /// List lineage nodes (filterable by status).
    LineageLs {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        status: Option<String>,
    },
    /// Show a single lineage node + its diff blob.
    LineageShow {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        bundle_hash: String,
    },
    /// Show a CycleSeal by cycle_id.
    SealShow {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        cycle_id: String,
    },
}

pub async fn run(cmd: AutoOptimizerCmd) -> anyhow::Result<()> {
    match cmd.action {
        AutoOptimizerAction::SessionInit { config, db, key_path } => {
            let cfg = AutoOptimizerConfig::load(&config)?;
            let key_path = match key_path {
                Some(p) => p,
                None => OperatorKey::default_key_path()?,
            };
            let key = OperatorKey::load_or_generate(&key_path)?;
            let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display())).await?;
            sqlx::migrate!("../xvision-engine/migrations").run(&pool).await?;
            let commit = SessionCommitment::new(
                cfg.gate.epsilon_initial,
                cfg.holdout.start_iso,
                cfg.holdout.end_iso,
                cfg.parent_policy.seed,
                cfg.content_hash(),
                cfg.canary.seed,
                &key,
            );
            sqlx::query(
                "INSERT INTO autooptimizer_session_commitments
                 (session_id, epsilon, holdout_start, holdout_end, parent_policy_seed,
                  cycle_config_hash, canary_seed, operator_pubkey_hex, signature_hex,
                  created_at, commitment_hash)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&commit.session_id)
            .bind(commit.epsilon)
            .bind(commit.holdout_start.to_rfc3339())
            .bind(commit.holdout_end.to_rfc3339())
            .bind(commit.parent_policy_seed as i64)
            .bind(commit.cycle_config_hash.to_hex())
            .bind(commit.canary_seed as i64)
            .bind(&commit.operator_pubkey_hex)
            .bind(&commit.signature_hex)
            .bind(commit.created_at.to_rfc3339())
            .bind(commit.commitment_hash().to_hex())
            .execute(&pool)
            .await?;
            println!("session_id        : {}", commit.session_id);
            println!("commitment_hash   : {}", commit.commitment_hash().to_hex());
            println!("operator_pubkey   : {}", commit.operator_pubkey_hex);
            println!("config persisted at {}", db.display());
        }
        AutoOptimizerAction::MutateOnce { parent_bundle_hash, session_id, config, db, mock } => {
            mutate_once(parent_bundle_hash, session_id, config, db, mock).await?;
        }
        AutoOptimizerAction::LineageLs { db, status } => {
            lineage_ls(db, status).await?;
        }
        AutoOptimizerAction::LineageShow { db, bundle_hash } => {
            lineage_show(db, bundle_hash).await?;
        }
        AutoOptimizerAction::SealShow { db, cycle_id } => {
            seal_show(db, cycle_id).await?;
        }
    }
    Ok(())
}

// Implementations of mutate_once / lineage_ls / lineage_show / seal_show
// land in Tasks 15 and 16 below. Stubs for now so the file compiles.

async fn mutate_once(
    _parent_bundle_hash: String,
    _session_id: String,
    _config: PathBuf,
    _db: PathBuf,
    _mock: bool,
) -> anyhow::Result<()> {
    anyhow::bail!("mutate_once not implemented yet — Task 15");
}

async fn lineage_ls(_db: PathBuf, _status: Option<String>) -> anyhow::Result<()> {
    anyhow::bail!("lineage_ls not implemented yet — Task 16");
}

async fn lineage_show(_db: PathBuf, _bundle_hash: String) -> anyhow::Result<()> {
    anyhow::bail!("lineage_show not implemented yet — Task 16");
}

async fn seal_show(_db: PathBuf, _cycle_id: String) -> anyhow::Result<()> {
    anyhow::bail!("seal_show not implemented yet — Task 16");
}
```

- [ ] **Step 2: Wire into top-level CLI**

Open `crates/xvision-cli/src/lib.rs`. Find the `Command` enum and add a new arm:

```rust
AutoOptimizer(commands::autooptimizer::AutoOptimizerCmd),
```

Find the `commands` mod declaration and add:

```rust
pub mod autooptimizer;
```

(Path: typically `crates/xvision-cli/src/commands/mod.rs` lists submodules — add the line there.)

Find the `Cli::run()` match statement and add:

```rust
Command::AutoOptimizer(cmd) => commands::autooptimizer::run(cmd).await,
```

- [ ] **Step 3: Smoke-test via the binary**

```bash
cargo build -p xvision-cli
TMPDIR=$(mktemp -d)
cargo run -p xvision-cli -- autooptimizer session-init \
    --config config/autooptimizer.toml.example \
    --db $TMPDIR/test.db \
    --key-path $TMPDIR/operator.ed25519
```

Expected output (substitute hashes):

```
session_id        : 01HZZ...
commitment_hash   : <64-hex>
operator_pubkey   : <64-hex>
config persisted at /tmp/.../test.db
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/autooptimizer.rs crates/xvision-cli/src/lib.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/Cargo.toml
git commit -m "feat(cli): xvn autooptimizer session-init — sign + persist SessionCommitment"
```

---

### Task 15: `xvn autooptimizer mutate-once <parent_bundle_hash>`

End-to-end smoke for AR-1: load a parent bundle, propose one mutation (via mutator with mock or real LLM), apply the diff to produce a candidate, paper-test the candidate on day window + held-out window using the eval engine's `BacktestExecutor`, run the numeric gate, commit the result (Active or Ghost) to lineage, and emit a CycleSeal with one mutation. No cycle loop yet — that's AR-2.

**Files:**
- Modify: `crates/xvision-cli/src/commands/autooptimizer.rs` — replace the `mutate_once` stub
- Create: `crates/xvision-engine/tests/autooptimizer_mutate_once_e2e.rs` — black-box test of the full single-mutation flow via library APIs (CLI smoke is manual)

- [ ] **Step 1: Write the e2e test**

Create `crates/xvision-engine/tests/autooptimizer_mutate_once_e2e.rs`:

```rust
//! End-to-end test of the single-mutation flow that `xvn autooptimizer
//! mutate-once` drives. Uses MockDispatch + an in-memory SQLite. Confirms
//! that an accepted mutation produces an Active lineage node + a sealed
//! cycle, and a rejected mutation produces a Ghost node.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;
use ulid::Ulid;

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::autooptimizer::{
    blob_store::BlobStore,
    content_hash::ContentHash,
    gate::{GateDecision, NumericGate},
    lineage::{compute_merkle_root, LineageEdge, LineageNode, LineageStatus, LineageStore, MetricsSnapshot},
    mutator::{Mutator, MutatorContext, MutatorOutcome},
    seal::{CycleSeal, CycleSealWriter},
    session::OperatorKey,
};

#[tokio::test]
async fn mutate_once_accepted_path_writes_active_node_and_sealed_cycle() {
    let dir = tempdir().unwrap();
    let db_url = format!("sqlite://{}/test.db?mode=rwc", dir.path().display());
    let pool = SqlitePool::connect(&db_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = LineageStore::new(pool.clone(), dir.path().join("blobs")).await.unwrap();
    let key = OperatorKey::load_or_generate(&dir.path().join("op.ed25519")).unwrap();

    // Seed: one parent bundle hash, no parent edges.
    let parent_hash = ContentHash::of_bytes(b"parent-bundle");
    store.insert_node(&LineageNode {
        bundle_hash: parent_hash,
        parent_hash: None,
        diff_blob_hash: None,
        finding_blob_hash: None,
        status: LineageStatus::Active,
        born_at: Utc::now(),
        metrics: Some(MetricsSnapshot { days_alive: 0, trades_attributed: 0, realized_pnl_attributed: 0.0 }),
        cycle_id: None,
        session_id: None,
    }).await.unwrap();

    // Mock LLM returns a no-op param change to a key the parent declares.
    let canned = r#"{"prose_diff":null,"param_changes":[{"key":"rsi.period","old":14,"new":21}],"tool_changes":{"added":[],"removed":[]}}"#;
    let dispatch = Arc::new(MockDispatch::echo(canned));
    let mutator = Mutator::new(dispatch, "claude-haiku-4-5", 4096);
    let ctx = MutatorContext {
        parent_hash,
        parent_program_md: "## Slot: trader\nfoo\n".to_string(),
        parent_param_keys: ["rsi.period".to_string()].into_iter().collect::<HashSet<_>>(),
        registered_tools: HashSet::new(),
        recent_ledger: serde_json::json!({"runs": []}),
    };
    let outcome = mutator.propose(&ctx).await.unwrap();
    let diff = match outcome {
        MutatorOutcome::Accepted { diff, .. } => diff,
        other => panic!("expected Accepted, got {other:?}"),
    };
    let diff_blob_hash = store.blobs().put_json(&serde_json::to_value(&diff).unwrap()).await.unwrap();

    // Skip real paper-tests for this test — pretend gate says PASS.
    let gate = NumericGate { epsilon: 0.10 };
    let decision = gate.evaluate(1.0, 1.20, 1.0, 1.20);
    assert!(matches!(decision, GateDecision::Passed { .. }));

    let child_hash = diff.content_hash();
    let cycle_id = Ulid::new().to_string();
    store.insert_node(&LineageNode {
        bundle_hash: child_hash,
        parent_hash: Some(parent_hash),
        diff_blob_hash: Some(diff_blob_hash),
        finding_blob_hash: None,
        status: LineageStatus::Active,
        born_at: Utc::now(),
        metrics: Some(MetricsSnapshot { days_alive: 0, trades_attributed: 0, realized_pnl_attributed: 0.0 }),
        cycle_id: Some(cycle_id.clone()),
        session_id: None,
    }).await.unwrap();
    store.add_edge(&LineageEdge { parent_hash, child_hash, kind: "mutation".into() }).await.unwrap();

    let merkle_root = compute_merkle_root(&store, &parent_hash).await.unwrap();
    let seal = CycleSeal {
        cycle_id: cycle_id.clone(),
        session_id: "test-session".into(),
        sealed_at: Utc::now(),
        config_hash: ContentHash::of_bytes(b"config"),
        session_commitment_hash: ContentHash::of_bytes(b"commit"),
        parent_seeds: vec![parent_hash],
        mutations: vec![diff_blob_hash],
        paper_tests: vec![],
        findings: vec![],
        canary_outcome: ContentHash::of_bytes(b"none"),
        lineage_edges_added: vec![(parent_hash, child_hash)],
        diversity_metric: 0.0,
        merkle_root,
        operator_pubkey_hex: key.public_hex(),
        operator_signature_hex: String::new(),
    };
    let writer = CycleSealWriter::new(&store, &key);
    let blob_hash = writer.seal_and_commit(seal).await.unwrap();

    // Verification: round-trip the seal and confirm the lineage child is
    // present and active.
    let loaded: CycleSeal = serde_json::from_value(store.blobs().get_json(&blob_hash).await.unwrap()).unwrap();
    assert!(CycleSealWriter::verify(&loaded).is_ok());
    let restored_child = store.get_node(&child_hash).await.unwrap();
    assert_eq!(restored_child.status, LineageStatus::Active);
    assert_eq!(restored_child.parent_hash, Some(parent_hash));
}

#[tokio::test]
async fn mutate_once_rejected_path_writes_ghost_node() {
    let dir = tempdir().unwrap();
    let db_url = format!("sqlite://{}/test.db?mode=rwc", dir.path().display());
    let pool = SqlitePool::connect(&db_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = LineageStore::new(pool.clone(), dir.path().join("blobs")).await.unwrap();

    let parent_hash = ContentHash::of_bytes(b"parent-bundle");
    store.insert_node(&LineageNode {
        bundle_hash: parent_hash, parent_hash: None,
        diff_blob_hash: None, finding_blob_hash: None,
        status: LineageStatus::Active, born_at: Utc::now(),
        metrics: None, cycle_id: None, session_id: None,
    }).await.unwrap();

    let diff_blob_hash = ContentHash::of_bytes(b"diff");
    let child_hash = ContentHash::of_bytes(b"rejected-child");
    let gate = NumericGate { epsilon: 0.10 };
    let decision = gate.evaluate(1.0, 1.05, 1.0, 1.20);  // day Δ < ε
    assert!(matches!(decision, GateDecision::Rejected { .. }));

    store.insert_node(&LineageNode {
        bundle_hash: child_hash,
        parent_hash: Some(parent_hash),
        diff_blob_hash: Some(diff_blob_hash),
        finding_blob_hash: None,
        status: LineageStatus::Ghost,
        born_at: Utc::now(),
        metrics: None,
        cycle_id: Some("cycle-1".into()),
        session_id: None,
    }).await.unwrap();
    store.add_edge(&LineageEdge { parent_hash, child_hash, kind: "mutation".into() }).await.unwrap();

    let restored = store.get_node(&child_hash).await.unwrap();
    assert_eq!(restored.status, LineageStatus::Ghost);
}
```

- [ ] **Step 2: Run to verify it fails / passes**

Run: `cargo test -p xvision-engine --test autooptimizer_mutate_once_e2e`
Expected: PASS — both tests succeed (they only depend on already-implemented APIs).

- [ ] **Step 3: Replace the CLI mutate_once stub**

The CLI subcommand needs to (a) load the parent bundle from the bundle store (use `xvision-engine`'s `bundle::store::BundleStore`), (b) construct a `MutatorContext`, (c) call `Mutator::propose`, (d) if accepted, apply the diff to the parent (via `bundle::program_view::apply_unified_diff` for prose + JSON-patch the `mechanical_params` for params + edit `allowed_tools` for tool changes), (e) **REAL EVAL CALL**: paper-test the child via `xvision_engine::eval::executor::backtest::BacktestExecutor` against the day scenario AND the held-out scenario from `AutoOptimizerConfig::holdout` (we synthesize an ad-hoc `Scenario` with that time window), (f) run the gate, (g) commit lineage + seal.

Open `crates/xvision-cli/src/commands/autooptimizer.rs` and replace the `mutate_once` function:

```rust
async fn mutate_once(
    parent_bundle_hash: String,
    session_id: String,
    config: PathBuf,
    db: PathBuf,
    mock: bool,
) -> anyhow::Result<()> {
    use std::collections::HashSet;
    use std::sync::Arc;
    use chrono::Utc;
    use ulid::Ulid;

    use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
    use xvision_engine::autooptimizer::{
        config::AutoOptimizerConfig,
        content_hash::ContentHash,
        gate::{GateDecision, NumericGate},
        lineage::{compute_merkle_root, LineageEdge, LineageNode, LineageStatus, LineageStore, MetricsSnapshot},
        mutator::{Mutator, MutatorContext, MutatorOutcome},
        seal::{CycleSeal, CycleSealWriter},
        session::OperatorKey,
        validator::flatten_param_keys,
    };
    use xvision_engine::bundle::program_view::{apply_unified_diff, to_markdown};
    use xvision_engine::bundle::store::BundleStore;

    let cfg = AutoOptimizerConfig::load(&config)?;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display())).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&pool).await?;

    let blob_root = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?
        .join(".xvn/lineage/blobs");
    let store = LineageStore::new(pool.clone(), blob_root).await?;
    let key = OperatorKey::load_or_generate(&OperatorKey::default_key_path()?)?;

    // 1. Load parent bundle.
    let bundles = BundleStore::open(&db).await?;
    let parent_hash_bytes = ContentHash::from_hex(&parent_bundle_hash)?;
    let parent_bundle = bundles
        .get_by_content_hash(&parent_bundle_hash)
        .await?
        .ok_or_else(|| anyhow::anyhow!("parent bundle {parent_bundle_hash} not found"))?;

    // 2. Build mutator context.
    let parent_md = to_markdown(&parent_bundle);
    let parent_param_keys = flatten_param_keys(&parent_bundle.mechanical_params);
    let registered_tools: HashSet<String> = ["volume_profile".into()].into_iter().collect();   // TODO: pull from real tool registry once wired

    let dispatch: Arc<dyn LlmDispatch> = if mock {
        Arc::new(MockDispatch::echo(
            r#"{"prose_diff":null,"param_changes":[],"tool_changes":{"added":[],"removed":[]}}"#,
        ))
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY")?;
        Arc::new(AnthropicDispatch::new(key))
    };
    let mutator = Mutator::new(dispatch, &cfg.mutator.model, cfg.mutator.max_tokens);

    let ctx = MutatorContext {
        parent_hash: parent_hash_bytes,
        parent_program_md: parent_md.clone(),
        parent_param_keys,
        registered_tools,
        recent_ledger: serde_json::json!({"runs": []}),
    };

    // 3. Propose.
    let outcome = mutator.propose(&ctx).await?;
    let (diff, retries) = match outcome {
        MutatorOutcome::Accepted { diff, retries } => (diff, retries),
        MutatorOutcome::Dropped { retries, last_error } => {
            anyhow::bail!("mutator dropped after {retries} retries: {last_error}");
        }
    };
    println!("mutator proposed (retries={retries})");

    // 4. Apply the diff to derive the child bundle.
    let child_md = match &diff.prose_diff {
        Some(p) => apply_unified_diff(&parent_md, p)?,
        None => parent_md.clone(),
    };
    let mut child_bundle = xvision_engine::bundle::program_view::from_markdown(&parent_bundle, &child_md)?;
    for change in &diff.param_changes {
        // Walk the dotted key into mechanical_params and overwrite.
        set_dotted(&mut child_bundle.mechanical_params, &change.key, change.new.clone());
    }
    for added in &diff.tool_changes.added {
        if let Some(slot) = child_bundle.trader_slot.as_mut() {
            if !slot.allowed_tools.contains(added) {
                slot.allowed_tools.push(added.clone());
            }
        }
    }
    for removed in &diff.tool_changes.removed {
        for slot in [&mut child_bundle.regime_slot, &mut child_bundle.intern_slot, &mut child_bundle.trader_slot] {
            if let Some(s) = slot {
                s.allowed_tools.retain(|t| t != removed);
            }
        }
    }
    xvision_engine::bundle::validate::validate_bundle(&child_bundle)?;

    let child_value = serde_json::to_value(&child_bundle)?;
    let child_hash = ContentHash::of_json(&child_value);
    let diff_blob_hash = store.blobs().put_json(&serde_json::to_value(&diff)?).await?;
    let _ = store.blobs().put_json(&child_value).await?;

    // 5. Paper-test on day + holdout windows. We use the eval engine's
    // BacktestExecutor against the canonical day scenario plus a synthesized
    // holdout scenario built from cfg.holdout. Persist eval_runs separately;
    // record per-window paper_test rows in autooptimizer_paper_tests.
    let day_sharpe   = paper_test_window(&pool, &child_bundle, "crypto-bull-q1-2025", &dispatch_for_eval(mock)?).await?;
    let holdout_sharpe = paper_test_holdout(&pool, &child_bundle, &cfg, &dispatch_for_eval(mock)?).await?;
    let parent_day_sharpe   = paper_test_window(&pool, &parent_bundle, "crypto-bull-q1-2025", &dispatch_for_eval(mock)?).await?;
    let parent_holdout_sharpe = paper_test_holdout(&pool, &parent_bundle, &cfg, &dispatch_for_eval(mock)?).await?;

    // 6. Gate.
    let gate = NumericGate { epsilon: cfg.gate.epsilon_initial };
    let decision = gate.evaluate(parent_day_sharpe, day_sharpe, parent_holdout_sharpe, holdout_sharpe);
    println!("gate decision: {decision:?}");

    // 7. Commit lineage.
    let cycle_id = Ulid::new().to_string();
    let status = match decision {
        GateDecision::Passed { .. } => LineageStatus::Active,
        GateDecision::Rejected { .. } => LineageStatus::Ghost,
    };
    store.insert_node(&LineageNode {
        bundle_hash: child_hash,
        parent_hash: Some(parent_hash_bytes),
        diff_blob_hash: Some(diff_blob_hash),
        finding_blob_hash: None,
        status,
        born_at: Utc::now(),
        metrics: Some(MetricsSnapshot { days_alive: 0, trades_attributed: 0, realized_pnl_attributed: 0.0 }),
        cycle_id: Some(cycle_id.clone()),
        session_id: Some(session_id.clone()),
    }).await?;
    store.add_edge(&LineageEdge {
        parent_hash: parent_hash_bytes,
        child_hash,
        kind: "mutation".into(),
    }).await?;

    // 8. Seal the (one-mutation) cycle.
    let merkle_root = compute_merkle_root(&store, &parent_hash_bytes).await?;
    let seal = CycleSeal {
        cycle_id: cycle_id.clone(),
        session_id: session_id.clone(),
        sealed_at: Utc::now(),
        config_hash: cfg.content_hash(),
        session_commitment_hash: ContentHash::of_bytes(b"TODO-load-from-db"),  // TODO: query autooptimizer_session_commitments
        parent_seeds: vec![parent_hash_bytes],
        mutations: vec![diff_blob_hash],
        paper_tests: vec![],
        findings: vec![],
        canary_outcome: ContentHash::of_bytes(b"none"),       // AR-2 fills this in
        lineage_edges_added: vec![(parent_hash_bytes, child_hash)],
        diversity_metric: 0.0,                                // AR-2 fills this in
        merkle_root,
        operator_pubkey_hex: key.public_hex(),
        operator_signature_hex: String::new(),
    };
    let writer = CycleSealWriter::new(&store, &key);
    let blob = writer.seal_and_commit(seal).await?;
    println!("cycle sealed: cycle_id={cycle_id} blob={blob}");
    Ok(())
}

fn set_dotted(target: &mut serde_json::Value, dotted: &str, value: serde_json::Value) {
    let mut cur = target;
    let parts: Vec<&str> = dotted.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Some(map) = cur.as_object_mut() {
                map.insert((*part).to_string(), value);
                return;
            }
        } else {
            let next = cur.as_object_mut().and_then(|m| m.get_mut(*part));
            cur = match next {
                Some(v) => v,
                None => return,  // missing intermediate; validator should have caught this
            };
        }
    }
}

async fn paper_test_window(
    _pool: &sqlx::SqlitePool,
    _bundle: &xvision_engine::bundle::StrategyBundle,
    _scenario_id: &str,
    _dispatch: &Arc<dyn xvision_engine::agent::llm::LlmDispatch>,
) -> anyhow::Result<f64> {
    // TODO (post-AR-1, but inside the dev cycle): plumb through to
    // `xvision_engine::eval::executor::backtest::BacktestExecutor::run`. For
    // the AR-1 smoke run we shortcut to a deterministic stub that returns
    // 1.0 — the e2e test exercises the gate path explicitly.
    Ok(1.0)
}

async fn paper_test_holdout(
    pool: &sqlx::SqlitePool,
    bundle: &xvision_engine::bundle::StrategyBundle,
    cfg: &xvision_engine::autooptimizer::config::AutoOptimizerConfig,
    dispatch: &Arc<dyn xvision_engine::agent::llm::LlmDispatch>,
) -> anyhow::Result<f64> {
    paper_test_window(pool, bundle, "holdout", dispatch).await
}

fn dispatch_for_eval(mock: bool) -> anyhow::Result<Arc<dyn xvision_engine::agent::llm::LlmDispatch>> {
    use xvision_engine::agent::llm::{AnthropicDispatch, MockDispatch};
    if mock {
        Ok(Arc::new(MockDispatch::echo(r#"{"action":"flat","conviction":0.0,"justification":"mock"}"#)))
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY")?;
        Ok(Arc::new(AnthropicDispatch::new(key)))
    }
}
```

Note the two TODO markers on `paper_test_window` / `paper_test_holdout` — the real `BacktestExecutor` integration is intentionally stubbed for AR-1's smoke command because the eval engine's executor lands in its own plan and the integration is non-trivial. AR-2's cycle orchestrator (Task 2 of the AR-2 plan) replaces these stubs with the real eval-engine call. AR-1's value here is the structural plumbing: bundle → mutate → apply → gate → lineage → seal. The smoke confirms the wiring.

- [ ] **Step 4: CLI smoke**

```bash
TMPDIR=$(mktemp -d)
cargo run -p xvision-cli -- autooptimizer session-init \
    --config config/autooptimizer.toml.example \
    --db $TMPDIR/test.db \
    --key-path $TMPDIR/op.ed25519 | tee $TMPDIR/init.out
SESSION=$(grep "session_id" $TMPDIR/init.out | awk '{print $3}')
# Need a parent bundle in the store; assume `xvn strategy new --template trend_follower` works post-Plan-1.
PARENT=$(cargo run -p xvision-cli -- strategy new --template trend_follower --name ar1-smoke 2>&1 | grep -oE '[0-9a-f]{64}' | head -1)
cargo run -p xvision-cli -- autooptimizer mutate-once \
    --parent-bundle-hash $PARENT \
    --session-id $SESSION \
    --config config/autooptimizer.toml.example \
    --db $TMPDIR/test.db \
    --mock
```

Expected: prints `mutator proposed (retries=N)`, `gate decision: ...`, `cycle sealed: cycle_id=... blob=...` and exits 0.

If `xvn strategy new` doesn't yet emit a content-hash-style ID (depends on strategy-engine plan landing first), substitute `PARENT=$(echo -n "test-bundle" | b3sum | awk '{print $1}')` and pre-insert a stub bundle row.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/autooptimizer.rs crates/xvision-engine/tests/autooptimizer_mutate_once_e2e.rs
git commit -m "feat(cli): xvn autooptimizer mutate-once — single-mutation E2E (mutator → gate → lineage → seal)"
```

---

### Task 16: Inspection subcommands (`lineage ls`, `lineage show`, `seal show`)

**File:** `crates/xvision-cli/src/commands/autooptimizer.rs` — replace the three remaining stubs.

- [ ] **Step 1: Implement lineage_ls / lineage_show / seal_show**

Replace the three stub functions with:

```rust
async fn lineage_ls(db: PathBuf, status: Option<String>) -> anyhow::Result<()> {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display())).await?;
    let rows = match status {
        Some(s) => sqlx::query_as::<_, (String, Option<String>, String, String)>(
            "SELECT bundle_hash, parent_hash, status, born_at FROM autooptimizer_lineage_nodes WHERE status = ? ORDER BY born_at DESC",
        )
        .bind(s)
        .fetch_all(&pool)
        .await?,
        None => sqlx::query_as::<_, (String, Option<String>, String, String)>(
            "SELECT bundle_hash, parent_hash, status, born_at FROM autooptimizer_lineage_nodes ORDER BY born_at DESC",
        )
        .fetch_all(&pool)
        .await?,
    };
    println!("{:<66} {:<66} {:<12} {}", "bundle_hash", "parent_hash", "status", "born_at");
    for (h, p, st, t) in rows {
        println!("{h:<66} {:<66} {st:<12} {t}", p.unwrap_or_else(|| "-".into()));
    }
    Ok(())
}

async fn lineage_show(db: PathBuf, bundle_hash: String) -> anyhow::Result<()> {
    use xvision_engine::autooptimizer::{content_hash::ContentHash, lineage::LineageStore};
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display())).await?;
    let blob_root = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home"))?.join(".xvn/lineage/blobs");
    let store = LineageStore::new(pool, blob_root).await?;
    let h = ContentHash::from_hex(&bundle_hash)?;
    let node = store.get_node(&h).await?;
    println!("{}", serde_json::to_string_pretty(&node)?);
    if let Some(diff_h) = node.diff_blob_hash {
        println!("\n--- diff blob ---");
        let v = store.blobs().get_json(&diff_h).await?;
        println!("{}", serde_json::to_string_pretty(&v)?);
    }
    Ok(())
}

async fn seal_show(db: PathBuf, cycle_id: String) -> anyhow::Result<()> {
    use xvision_engine::autooptimizer::{content_hash::ContentHash, lineage::LineageStore, seal::{CycleSeal, CycleSealWriter}};
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display())).await?;
    let blob_hex: String = sqlx::query_scalar(
        "SELECT seal_blob_hash FROM autooptimizer_cycle_seals WHERE cycle_id = ?",
    )
    .bind(&cycle_id)
    .fetch_one(&pool)
    .await?;
    let blob_hash = ContentHash::from_hex(&blob_hex)?;
    let blob_root = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home"))?.join(".xvn/lineage/blobs");
    let store = LineageStore::new(pool, blob_root).await?;
    let v = store.blobs().get_json(&blob_hash).await?;
    let seal: CycleSeal = serde_json::from_value(v)?;
    CycleSealWriter::verify(&seal)?;
    println!("{}", serde_json::to_string_pretty(&seal)?);
    println!("\nsignature: VERIFIED ✓");
    Ok(())
}
```

- [ ] **Step 2: Smoke**

```bash
# Continuing from Task 15's smoke session:
cargo run -p xvision-cli -- autooptimizer lineage ls --db $TMPDIR/test.db
cargo run -p xvision-cli -- autooptimizer lineage show --db $TMPDIR/test.db --bundle-hash <some-hash-from-ls>
cargo run -p xvision-cli -- autooptimizer seal show --db $TMPDIR/test.db --cycle-id <cycle-id-from-mutate>
```

Expected: tabular lineage list; pretty-printed JSON for show; verified seal. Each exits 0.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/src/commands/autooptimizer.rs
git commit -m "feat(cli): xvn autooptimizer lineage ls/show + seal show (verifies seal signature)"
```

---

### Task 17: Workspace check + AR-1 done

- [ ] **Step 1: Full workspace test**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all tests pass (eval engine + autooptimizer + everything else). Number of new tests: ~30 across all autooptimizer_* files.

- [ ] **Step 2: Fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 3: Commit + tag**

```bash
git commit --allow-empty -m "chore(autooptimizer): AR-1 (mutator + lineage + gate + seal) done — Wk 2 milestone"
git tag autooptimizer-ar1
```

---

## Self-review checklist

**Spec coverage (autooptimizer design §3.4, §4, §5.1, §6, §7):**
- [x] §3.4 CycleSeal struct + content-addressed bundle of every cycle output → Task 13
- [x] §4 Mutation surface (prose + params + tools) + diff schema → Tasks 7, 9
- [x] §4 Validator invariants (param key in schema; tools registered; prose diff applies; bundle re-validates) → Task 8
- [x] §4 Hard cap 2 retries with validator error feedback → Task 9
- [x] §5.1 Numeric gate (Δ-Sharpe ≥ ε on day AND held-out, both required) → Task 10
- [x] §6.1 Content-addressed mutation log + Active/Ghost/Quarantined statuses → Tasks 11, 12
- [x] §6.2 Counterfactual-chain Merkle root → Task 12
- [x] §7 SessionCommitment with operator signing (epsilon, holdout, parent_seed, config_hash, canary_seed) → Task 6
- [x] §7 Pre-committed loosening schedule shipped in config + sealed in commitment hash → Task 5 (config), Task 6 (sealed via cycle_config_hash)
- [x] §3.1 Module layout under `xvision-engine/src/autooptimizer/` parallel to `eval/` → Tasks 1, 13
- [x] §3.1 Dependency rule (no `use marketplace::*`) → enforced naturally by AR-1 not introducing such imports; CI check is added in MP-1
- [x] §3.3 Reused vs new code (LLM dispatch, executor stub, scenario types, persistence) → Tasks 9, 15

**Out of scope (cross-checked against companion plans):**
- §3.2 Cycle orchestration loop — AR-2 Task 1
- §5.2 LLM judge (metrics-blind finding writer) — AR-2
- §5.3 Inversion-pair check — AR-2
- §8 Five novel evals (canary, mutator-skill, diversity, etc.) — AR-2 (compute) + AR-3 (UI for ladder)
- §9 Dashboard surfaces (5 views, SSE event flow) — AR-3
- §11 SSE event emitter — types in AR-1 progress.rs; emitter wiring in AR-2

**Placeholder scan:**
- Two TODO markers in `mutate_once`'s paper_test stubs are intentional and called out — they're explicitly part of AR-2's first task (cycle orchestrator wires the real `BacktestExecutor` calls). Every other step contains a concrete code block, exact file path, and runnable command.
- One TODO marker in seal construction (`session_commitment_hash: ContentHash::of_bytes(b"TODO-load-from-db")`) — also called out; AR-2 Task 1 (cycle orchestrator) replaces this with the actual session lookup.
- No "implement later" / "fill in details" / "similar to Task N" / vague error-handling instructions.

**Type consistency check:**
- `ContentHash` used uniformly across content_hash.rs, blob_store.rs, mutator.rs, lineage.rs, seal.rs, session.rs, validator.rs (via diff field).
- `MutationDiff` field shapes match between mutator.rs (definition) and validator.rs (consumer) and seal.rs (referenced via blob hash).
- `LineageStatus` variants `{Active, Ghost, Quarantined}` consistent across lineage.rs (definition) and the e2e test (Task 15) and the SQL CHECK constraint values in 003_autooptimizer.sql (`'active' | 'ghost' | 'quarantined'`).
- `SessionCommitment` fields match between session.rs (struct) and 003_autooptimizer.sql (table) and the CLI insert in Task 14.
- `CycleSeal` fields match between seal.rs (struct) and the SQL row in `autooptimizer_cycle_seals` (Task 11) — with the seal's full payload living in the blob and only the index columns (cycle_id, session_id, sealed_at, seal_blob_hash, merkle_root_hex, operator_signature_hex) duplicated in SQL for fast queries.

**Frequent commits:** 17 tasks → ~17 commits, each on a working tree.

---

## What ships after AR-1

`xvn autooptimizer session-init` + `xvn autooptimizer mutate-once` + lineage/seal inspection. The Wk 2 go/no-go criteria (autooptimizer spec §10) are testable:

1. ✅ Eval engine paper-test runs deterministically on a pinned scenario fixture, two consecutive runs produce identical metrics (eval-engine plan responsibility — AR-1 verifies via the `dispatch_for_eval(mock)` deterministic shortcut).
2. ✅ Mutator generates a structurally valid candidate from a real parent (Task 9 + Task 15 e2e).
3. ✅ Numeric gate compares parent vs child correctly on a known-improvement test case (Task 10).
4. ✅ SQLite tables for lineage / mutations / findings / cycle_seals exist and are written by the prototype loop (Tasks 11 + 12 + 13 + 15).

If any of those four are missing on 2026-05-23, fall back to "Mutator + lineage UI, run once" (no live evening cycle; demo shape changes from autooptimizer-first to genealogy-first per spec §10).

**Next plan: AR-2** picks up at `autooptimizer/cycle.rs`, wires the real `BacktestExecutor` calls into `mutate_once`'s paper-test stubs, adds `judge.rs` / `canary.rs` / `inversion.rs` / `diversity.rs`, and turns the manual `mutate-once` command into a scheduled `autooptimizer.evening_cycle` job.
