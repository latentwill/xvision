# Cortex Memory Integration Plan (V2D)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each `AgentSlot` an optional persistent memory (off / global / agent-scoped) backed by an embedded SQLite vector store, with automatic recall before dispatch and automatic recording after each decision, and surface memory activity in eval review.

**Architecture:** A new `xvision-memory` crate owns a SQLite-backed cosine-top-k store at `~/.xvn/memory.db`. `AgentSlot` gains a `memory_mode: MemoryMode` field (engine migration 026). The `execute_slot` dispatcher seam in `crates/xvision-engine/src/agent/execute.rs` recalls top-k matches and prepends them to `system_prompt` for non-off slots, then a post-dispatch recorder writes the decision back to the slot's namespace. Two new `events.jsonl` event kinds (`memory_recall` / `memory_write`) drive the eval-review UI. Sidecar / cortex-http boundary is deferred to v2 per `team/intake/2026-05-21-v2d-agent-memory.md` Decision 1.

**Tech Stack:** Rust workspace (`sqlx::SqlitePool`, `serde`, `thiserror`, `chrono`, `sha2`); existing `xvision-intern` provider clients for embedding calls (OpenAI / Voyage); ts-rs for the frontend type surface; React (`AgentForm.tsx`) for the Memory selector.

---

## Source context (read before starting)

- Intake: `team/intake/2026-05-21-v2d-agent-memory.md` — the 10 locked decisions and per-track scope. Every phase below maps to one of the five intake tracks.
- Existing AgentSlot field pattern: `crates/xvision-engine/src/agents/model.rs:96` (struct), `crates/xvision-engine/migrations/025_agent_slot_cache_and_window.sql` (migration template that already follows the "NULL-default safe migration" pattern).
- Dispatcher seam: `crates/xvision-engine/src/agent/execute.rs:1` — `execute_slot` builds and dispatches one `LlmRequest` per loop iteration. `LlmRequest.system_prompt` is at `crates/xvision-engine/src/agent/llm.rs:109`.
- Frontend type surface: `frontend/web/src/api/types.gen/AgentSlot.ts` is ts-rs generated; the codegen runs via the existing `ts-export` feature on the engine crate. Hand edits there are overwritten.
- Migration registry: `team/MANIFEST.md`; next available number is **026** and is claimed by Phase 3 of this plan.

## Phase 0: `v2d-cortex-memory-plan` (this document)

This phase ships as a doc-only PR — adding this plan file plus a one-line entry in `docs/README.md` (or equivalent index) if one exists. No code changes.

**Files:**
- Create: `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md` (this file)
- Create: `team/intake/2026-05-21-v2d-agent-memory.md` (companion intake)

- [ ] **Step 0.1: Commit the plan + intake**

```bash
git add docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md \
        team/intake/2026-05-21-v2d-agent-memory.md
git commit -m "v2d: write cortex memory integration plan + intake"
```

---

## Phase 1: `v2d-xvision-memory-crate`

New workspace crate. Standalone — no engine edits in this phase. Ships its own tests; cannot break the engine.

**Files:**
- Create: `crates/xvision-memory/Cargo.toml`
- Create: `crates/xvision-memory/src/lib.rs`
- Create: `crates/xvision-memory/src/store.rs`
- Create: `crates/xvision-memory/src/embedder.rs`
- Create: `crates/xvision-memory/src/types.rs`
- Create: `crates/xvision-memory/migrations/0001_init.sql`
- Create: `crates/xvision-memory/migrations/0001_init.down.sql`
- Create: `crates/xvision-memory/tests/store.rs`
- Modify: `Cargo.toml` (workspace `members` + `default-members`)

### Task 1.1: Register the crate in the workspace

- [ ] **Step 1.1.1: Add the crate to `Cargo.toml`**

Edit the workspace `Cargo.toml`. Add `"crates/xvision-memory"` to `members` AND to `default-members` (alphabetically between `xvision-intern` and `xvision-observability`):

```toml
[workspace]
members = [
    "crates/xvision-agent-client",
    "crates/xvision-core",
    "crates/xvision-data",
    "crates/xvision-intern",
    "crates/xvision-memory",
    "crates/xvision-trader",
    # … (existing entries continue)
]

default-members = [
    "crates/xvision-agent-client",
    "crates/xvision-core",
    "crates/xvision-data",
    "crates/xvision-intern",
    "crates/xvision-memory",
    "crates/xvision-trader",
    # … (existing entries continue)
]
```

- [ ] **Step 1.1.2: Create `crates/xvision-memory/Cargo.toml`**

```toml
[package]
name = "xvision-memory"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-rustls", "macros", "chrono"] }
thiserror = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
ulid = { version = "1", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["test-util"] }
```

- [ ] **Step 1.1.3: Verify the crate compiles empty**

Create `crates/xvision-memory/src/lib.rs` with a single line:

```rust
//! xvision-memory — embedded vector memory for agent slots (V2D).
```

Run: `cargo check -p xvision-memory`
Expected: PASS (no warnings).

- [ ] **Step 1.1.4: Commit**

```bash
git add Cargo.toml crates/xvision-memory/Cargo.toml crates/xvision-memory/src/lib.rs
git commit -m "v2d: scaffold xvision-memory crate"
```

### Task 1.2: Core types (`MemoryMode`, `MemoryItem`, `Namespace`)

- [ ] **Step 1.2.1: Write the failing test**

Create `crates/xvision-memory/tests/store.rs`:

```rust
use xvision_memory::types::{MemoryMode, Namespace};

#[test]
fn memory_mode_serde_round_trip() {
    for mode in [MemoryMode::Off, MemoryMode::Global, MemoryMode::AgentScoped] {
        let json = serde_json::to_string(&mode).unwrap();
        let back: MemoryMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }
    assert_eq!(serde_json::to_string(&MemoryMode::Off).unwrap(), "\"off\"");
    assert_eq!(serde_json::to_string(&MemoryMode::AgentScoped).unwrap(), "\"agent_scoped\"");
}

#[test]
fn namespace_for_mode_uses_agent_id() {
    assert_eq!(Namespace::for_mode(MemoryMode::Off, "01HZTEST").as_str(), None.unwrap_or_default());
    assert_eq!(Namespace::for_mode(MemoryMode::Global, "01HZTEST").as_str(), "global");
    assert_eq!(Namespace::for_mode(MemoryMode::AgentScoped, "01HZTEST").as_str(), "agent:01HZTEST");
}
```

- [ ] **Step 1.2.2: Run to verify it fails**

Run: `cargo test -p xvision-memory --test store -- memory_mode_serde_round_trip`
Expected: FAIL (`unresolved import xvision_memory::types`).

- [ ] **Step 1.2.3: Implement `types.rs`**

Create `crates/xvision-memory/src/types.rs`:

```rust
//! Public value types for xvision-memory.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    #[default]
    Off,
    Global,
    AgentScoped,
}

impl MemoryMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryMode::Off => "off",
            MemoryMode::Global => "global",
            MemoryMode::AgentScoped => "agent_scoped",
        }
    }

    pub fn parse_or_off(s: &str) -> Self {
        match s {
            "global" => MemoryMode::Global,
            "agent_scoped" => MemoryMode::AgentScoped,
            _ => MemoryMode::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace(String);

impl Namespace {
    pub fn for_mode(mode: MemoryMode, agent_id: &str) -> Self {
        match mode {
            MemoryMode::Off => Namespace(String::new()),
            MemoryMode::Global => Namespace("global".to_string()),
            MemoryMode::AgentScoped => Namespace(format!("agent:{agent_id}")),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_active(&self) -> bool {
        !self.0.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItem {
    pub id: String,
    pub namespace: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub source_run_id: Option<String>,
    pub source_cycle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryMatch {
    pub id: String,
    pub text: String,
    pub score: f32,
}
```

Update `crates/xvision-memory/src/lib.rs`:

```rust
//! xvision-memory — embedded vector memory for agent slots (V2D).

pub mod types;
```

- [ ] **Step 1.2.4: Run to verify it passes**

Run: `cargo test -p xvision-memory --test store`
Expected: PASS (both tests).

- [ ] **Step 1.2.5: Commit**

```bash
git add crates/xvision-memory/src/lib.rs crates/xvision-memory/src/types.rs crates/xvision-memory/tests/store.rs
git commit -m "v2d: add MemoryMode + Namespace + MemoryItem types"
```

### Task 1.3: SQLite schema + lazy migrator

- [ ] **Step 1.3.1: Write the migration files**

Create `crates/xvision-memory/migrations/0001_init.sql`:

```sql
CREATE TABLE memory_items (
    id            TEXT PRIMARY KEY,
    namespace     TEXT NOT NULL,
    text          TEXT NOT NULL,
    embedding     BLOB NOT NULL,
    embedding_dim INTEGER NOT NULL,
    embedder_id   TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    source_run_id TEXT,
    source_cycle_id TEXT
);

CREATE INDEX idx_memory_items_namespace ON memory_items(namespace);
CREATE INDEX idx_memory_items_created   ON memory_items(created_at);
```

Create `crates/xvision-memory/migrations/0001_init.down.sql`:

```sql
DROP INDEX IF EXISTS idx_memory_items_created;
DROP INDEX IF EXISTS idx_memory_items_namespace;
DROP TABLE IF EXISTS memory_items;
```

- [ ] **Step 1.3.2: Write the failing test**

Append to `crates/xvision-memory/tests/store.rs`:

```rust
use xvision_memory::store::MemoryStore;

#[tokio::test]
async fn open_lazy_creates_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.db");
    let store = MemoryStore::open(&path).await.unwrap();
    // Reopening must be idempotent.
    let _store2 = MemoryStore::open(&path).await.unwrap();
    drop(store);
}
```

- [ ] **Step 1.3.3: Run to verify it fails**

Run: `cargo test -p xvision-memory --test store -- open_lazy_creates_schema`
Expected: FAIL (`unresolved import xvision_memory::store`).

- [ ] **Step 1.3.4: Implement `store.rs` with `MemoryStore::open`**

Create `crates/xvision-memory/src/store.rs`:

```rust
//! SQLite-backed memory store (V2D).

use std::path::Path;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::types::{MemoryItem, MemoryMatch, Namespace};

pub struct MemoryStore {
    pool: SqlitePool,
}

impl MemoryStore {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("memory: create parent dir")?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .context("memory: open sqlite pool")?;
        sqlx::migrate!("./migrations").run(&pool).await.context("memory: migrate")?;
        Ok(Self { pool })
    }

    pub async fn open_in_memory() -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .in_memory(true)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool { &self.pool }
}
```

Update `crates/xvision-memory/src/lib.rs`:

```rust
//! xvision-memory — embedded vector memory for agent slots (V2D).

pub mod store;
pub mod types;
```

- [ ] **Step 1.3.5: Run to verify it passes**

Run: `cargo test -p xvision-memory --test store -- open_lazy_creates_schema`
Expected: PASS.

- [ ] **Step 1.3.6: Commit**

```bash
git add crates/xvision-memory/migrations crates/xvision-memory/src/store.rs crates/xvision-memory/src/lib.rs crates/xvision-memory/tests/store.rs
git commit -m "v2d: add MemoryStore with lazy SQLite schema migrate"
```

### Task 1.4: Upsert + namespace-isolated query

- [ ] **Step 1.4.1: Write the failing test**

Append to `crates/xvision-memory/tests/store.rs`:

```rust
use xvision_memory::types::{MemoryItem, MemoryMatch};

fn make_item(id: &str, ns: &str, text: &str, emb: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        text: text.into(),
        embedding: emb,
        created_at: chrono::Utc::now(),
        source_run_id: None,
        source_cycle_id: None,
    }
}

#[tokio::test]
async fn upsert_then_query_returns_top_k_by_cosine() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert(&make_item("a", "global", "alpha", vec![1.0, 0.0, 0.0]), "test-embedder").await.unwrap();
    store.upsert(&make_item("b", "global", "beta",  vec![0.0, 1.0, 0.0]), "test-embedder").await.unwrap();
    store.upsert(&make_item("c", "global", "gamma", vec![0.9, 0.1, 0.0]), "test-embedder").await.unwrap();
    let hits = store.query("global", &[1.0, 0.0, 0.0], 2).await.unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, "a");
    assert_eq!(hits[1].id, "c");
    assert!(hits[0].score > hits[1].score);
}

#[tokio::test]
async fn query_isolates_by_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert(&make_item("a", "agent:A", "alpha", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert(&make_item("b", "agent:B", "beta",  vec![1.0, 0.0]), "test").await.unwrap();
    let hits_a = store.query("agent:A", &[1.0, 0.0], 5).await.unwrap();
    let hits_b = store.query("agent:B", &[1.0, 0.0], 5).await.unwrap();
    let hits_g = store.query("global",  &[1.0, 0.0], 5).await.unwrap();
    assert_eq!(hits_a.len(), 1);
    assert_eq!(hits_a[0].id, "a");
    assert_eq!(hits_b.len(), 1);
    assert_eq!(hits_b[0].id, "b");
    assert_eq!(hits_g.len(), 0);
}
```

- [ ] **Step 1.4.2: Run to verify it fails**

Run: `cargo test -p xvision-memory --test store`
Expected: FAIL (`no method named upsert / query found for struct MemoryStore`).

- [ ] **Step 1.4.3: Implement `upsert` + `query`**

Append to `crates/xvision-memory/src/store.rs`:

```rust
fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v { out.extend_from_slice(&f.to_le_bytes()); }
    out
}

fn embedding_from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na  += x * x;
        nb  += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

impl MemoryStore {
    pub async fn upsert(&self, item: &MemoryItem, embedder_id: &str) -> anyhow::Result<()> {
        let blob = embedding_to_blob(&item.embedding);
        let dim = item.embedding.len() as i64;
        let ts = item.created_at.to_rfc3339();
        sqlx::query(
            "INSERT OR REPLACE INTO memory_items \
             (id, namespace, text, embedding, embedding_dim, embedder_id, created_at, source_run_id, source_cycle_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.id)
        .bind(&item.namespace)
        .bind(&item.text)
        .bind(blob)
        .bind(dim)
        .bind(embedder_id)
        .bind(ts)
        .bind(&item.source_run_id)
        .bind(&item.source_cycle_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn query(&self, namespace: &str, query_embedding: &[f32], k: usize) -> anyhow::Result<Vec<MemoryMatch>> {
        let rows: Vec<(String, String, Vec<u8>)> = sqlx::query_as(
            "SELECT id, text, embedding FROM memory_items WHERE namespace = ?",
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;
        let mut scored: Vec<MemoryMatch> = rows
            .into_iter()
            .map(|(id, text, blob)| {
                let emb = embedding_from_blob(&blob);
                let score = cosine(query_embedding, &emb);
                MemoryMatch { id, text, score }
            })
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }
}
```

- [ ] **Step 1.4.4: Run to verify it passes**

Run: `cargo test -p xvision-memory --test store`
Expected: PASS (all four tests).

- [ ] **Step 1.4.5: Commit**

```bash
git add crates/xvision-memory/src/store.rs crates/xvision-memory/tests/store.rs
git commit -m "v2d: implement MemoryStore::upsert + query"
```

### Task 1.5: Forget API

- [ ] **Step 1.5.1: Write the failing test**

Append to `crates/xvision-memory/tests/store.rs`:

```rust
#[tokio::test]
async fn forget_namespace_clears_only_that_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store.upsert(&make_item("a", "agent:A", "alpha", vec![1.0, 0.0]), "test").await.unwrap();
    store.upsert(&make_item("b", "global",  "beta",  vec![1.0, 0.0]), "test").await.unwrap();
    store.forget("agent:A").await.unwrap();
    let hits_a = store.query("agent:A", &[1.0, 0.0], 5).await.unwrap();
    let hits_g = store.query("global",  &[1.0, 0.0], 5).await.unwrap();
    assert!(hits_a.is_empty());
    assert_eq!(hits_g.len(), 1);
}
```

- [ ] **Step 1.5.2: Run to verify it fails**

Run: `cargo test -p xvision-memory --test store -- forget_namespace_clears_only_that_namespace`
Expected: FAIL (`no method named forget`).

- [ ] **Step 1.5.3: Implement `forget`**

Append to the `impl MemoryStore` block in `crates/xvision-memory/src/store.rs`:

```rust
    pub async fn forget(&self, namespace: &str) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM memory_items WHERE namespace = ?")
            .bind(namespace)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }
```

- [ ] **Step 1.5.4: Run to verify it passes**

Run: `cargo test -p xvision-memory --test store`
Expected: PASS (all five tests).

- [ ] **Step 1.5.5: Commit**

```bash
git add crates/xvision-memory/src/store.rs crates/xvision-memory/tests/store.rs
git commit -m "v2d: implement MemoryStore::forget"
```

### Task 1.6: Embedder trait + OpenAI adapter

- [ ] **Step 1.6.1: Write the failing test**

Append to `crates/xvision-memory/tests/store.rs`:

```rust
use xvision_memory::embedder::{Embedder, StaticEmbedder};

#[tokio::test]
async fn static_embedder_returns_configured_vector() {
    let embedder = StaticEmbedder::new("test-embedder", vec![0.5, 0.5, 0.0]);
    let v = embedder.embed("anything").await.unwrap();
    assert_eq!(v, vec![0.5, 0.5, 0.0]);
    assert_eq!(embedder.id(), "test-embedder");
}
```

- [ ] **Step 1.6.2: Run to verify it fails**

Run: `cargo test -p xvision-memory --test store -- static_embedder_returns_configured_vector`
Expected: FAIL (`unresolved import xvision_memory::embedder`).

- [ ] **Step 1.6.3: Implement `embedder.rs`**

Create `crates/xvision-memory/src/embedder.rs`:

```rust
//! Embedder trait + adapters.
//!
//! The OpenAI adapter is implemented in `xvision-engine` (where the
//! provider client already lives). This module only defines the
//! abstract trait + a `StaticEmbedder` used by tests and by the
//! disabled-by-default unit-test path.

use async_trait::async_trait;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    fn id(&self) -> &str;
    fn dim(&self) -> usize;
}

pub struct StaticEmbedder {
    id: String,
    vector: Vec<f32>,
}

impl StaticEmbedder {
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self { id: id.into(), vector }
    }
}

#[async_trait]
impl Embedder for StaticEmbedder {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.vector.clone())
    }
    fn id(&self) -> &str { &self.id }
    fn dim(&self) -> usize { self.vector.len() }
}
```

Update `crates/xvision-memory/src/lib.rs`:

```rust
//! xvision-memory — embedded vector memory for agent slots (V2D).

pub mod embedder;
pub mod store;
pub mod types;
```

- [ ] **Step 1.6.4: Run to verify it passes**

Run: `cargo test -p xvision-memory`
Expected: PASS (all six tests).

- [ ] **Step 1.6.5: Commit**

```bash
git add crates/xvision-memory/src/embedder.rs crates/xvision-memory/src/lib.rs crates/xvision-memory/tests/store.rs
git commit -m "v2d: add Embedder trait + StaticEmbedder"
```

End of Phase 1. The `xvision-memory` crate is complete. No engine touch. Ready for `v2d-agent-memory-mode` to depend on it.

---

## Phase 2: `v2d-agent-memory-mode` (engine schema + slot field)

Adds `agent_slots.memory_mode` column, the `AgentSlot.memory_mode` field, and the store roundtrip. Engine-only. Claims migration **026**.

**Files:**
- Create: `crates/xvision-engine/migrations/026_agent_slot_memory_mode.sql`
- Create: `crates/xvision-engine/migrations/026_agent_slot_memory_mode.down.sql`
- Modify: `crates/xvision-engine/src/agents/model.rs` (struct + default + tests)
- Modify: `crates/xvision-engine/src/agents/store.rs` (column read/write)
- Modify: `crates/xvision-engine/Cargo.toml` (add `xvision-memory` dep)
- Modify: `team/MANIFEST.md` (claim 026)

### Task 2.1: Reserve migration 026 in the manifest

- [ ] **Step 2.1.1: Edit `team/MANIFEST.md`**

Add a row to the migration table (after row 025), and bump the "next available number" paragraph from 026 to 027:

```markdown
| 026 | agent-slot-memory-mode (V2D)             | in flight     |
```

```markdown
The next available number is **027**. The conductor must approve and
reserve in this table before a track touches
`crates/xvision-engine/migrations/`.
```

- [ ] **Step 2.1.2: Verify board lint**

Run: `bash scripts/board-lint.sh`
Expected: PASS.

- [ ] **Step 2.1.3: Commit**

```bash
git add team/MANIFEST.md
git commit -m "v2d: reserve migration 026 for agent_slot_memory_mode"
```

### Task 2.2: Add `xvision-memory` as a dependency of `xvision-engine`

- [ ] **Step 2.2.1: Edit `crates/xvision-engine/Cargo.toml`**

In the `[dependencies]` section, add:

```toml
xvision-memory = { path = "../xvision-memory" }
```

- [ ] **Step 2.2.2: Verify it compiles**

Run: `cargo check -p xvision-engine`
Expected: PASS.

- [ ] **Step 2.2.3: Commit**

```bash
git add crates/xvision-engine/Cargo.toml Cargo.lock
git commit -m "v2d: engine depends on xvision-memory"
```

### Task 2.3: Migration 026

- [ ] **Step 2.3.1: Write the up migration**

Create `crates/xvision-engine/migrations/026_agent_slot_memory_mode.sql`:

```sql
-- 026_agent_slot_memory_mode.sql — V2D per-slot memory toggle.
--
-- Adds a `memory_mode` text column to `agent_slots`. Values:
--   * 'off'           — no recall, no record. Default for existing rows.
--   * 'global'        — recall/write into a single shared `global` bucket.
--   * 'agent_scoped'  — recall/write into `agent:<agent_id>` bucket.
--
-- Stored as TEXT (not INTEGER) so the column is self-documenting in
-- `sqlite3` shells and round-trips through `MemoryMode::as_str` /
-- `MemoryMode::parse_or_off` without an enum table. The DEFAULT 'off'
-- guarantees pre-026 rows read back as the safe value with no backfill
-- pass — matching the migration 020 (`inputs_policy`) and 025
-- (`bar_history_limit`) precedent.

ALTER TABLE agent_slots ADD COLUMN memory_mode TEXT NOT NULL DEFAULT 'off';
```

- [ ] **Step 2.3.2: Write the down migration**

Create `crates/xvision-engine/migrations/026_agent_slot_memory_mode.down.sql`:

```sql
-- SQLite cannot DROP a column via ALTER TABLE before 3.35. We rebuild
-- the table minus the column to keep the down path real, matching the
-- migration 025 down precedent (which CHECKs and rebuilds).

CREATE TABLE agent_slots_new AS
  SELECT
    agent_id, slot_name, provider, model, system_prompt, skill_ids,
    max_tokens, temperature, prompt_version, inputs_policy, bar_history_limit
  FROM agent_slots;

DROP TABLE agent_slots;
ALTER TABLE agent_slots_new RENAME TO agent_slots;
```

(If the engine `agent_slots` column set differs from the list above, the contract author updates this `SELECT` clause to match the table state immediately before 026 — read `crates/xvision-engine/migrations/025_agent_slot_cache_and_window.sql` and prior to confirm. The list above reflects 020 + 025 having landed.)

- [ ] **Step 2.3.3: Commit**

```bash
git add crates/xvision-engine/migrations/026_agent_slot_memory_mode.sql \
        crates/xvision-engine/migrations/026_agent_slot_memory_mode.down.sql
git commit -m "v2d: migration 026 — agent_slots.memory_mode column"
```

### Task 2.4: Add `memory_mode` to the `AgentSlot` struct

- [ ] **Step 2.4.1: Write the failing test**

Add a test to `crates/xvision-engine/src/agents/model.rs` inside the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn slot_default_memory_mode_is_off() {
        let a = Agent::single_slot_default(
            "01HZ000000000000000000000",
            "demo",
            "anthropic",
            "claude-sonnet-4-6",
        );
        assert_eq!(a.slots[0].memory_mode, xvision_memory::types::MemoryMode::Off);
    }

    #[test]
    fn slot_serde_default_memory_mode_is_off() {
        // A JSON payload omitting `memory_mode` (the pre-026 client shape)
        // must deserialize to MemoryMode::Off.
        let json = r#"{
            "name":"main","provider":"anthropic","model":"claude-sonnet-4-6",
            "system_prompt":"","skill_ids":[],"max_tokens":null
        }"#;
        let slot: AgentSlot = serde_json::from_str(json).unwrap();
        assert_eq!(slot.memory_mode, xvision_memory::types::MemoryMode::Off);
    }
```

- [ ] **Step 2.4.2: Run to verify they fail**

Run: `cargo test -p xvision-engine agents::model::tests::slot_default_memory_mode_is_off agents::model::tests::slot_serde_default_memory_mode_is_off`
Expected: FAIL (`no field memory_mode`).

- [ ] **Step 2.4.3: Add the field to `AgentSlot`**

In `crates/xvision-engine/src/agents/model.rs`, after the `bar_history_limit` field (line ~182), inside the `AgentSlot` struct:

```rust
    /// Per-slot memory toggle (V2D). `Off` is the migration default and
    /// the value pre-026 rows read back as. `Global` shares memory
    /// across every slot in the workspace that picks `Global`;
    /// `AgentScoped` isolates memory by `agent_id` (multiple slots in
    /// the same Agent share the bucket — slot name is a label, not an
    /// identity). See
    /// `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`
    /// and `team/intake/2026-05-21-v2d-agent-memory.md` Decision 4.
    ///
    /// Persisted as a TEXT column on `agent_slots.memory_mode`
    /// (migration 026). The store layer maps unknown values to `Off`
    /// so a corrupted row can never crash the read path.
    #[serde(default)]
    pub memory_mode: xvision_memory::types::MemoryMode,
```

Also update `Agent::single_slot_default` (around line 244) to include the field:

```rust
            slots: vec![AgentSlot {
                name: "main".to_string(),
                provider: provider.into(),
                model: model.into(),
                system_prompt: String::new(),
                skill_ids: Vec::new(),
                max_tokens: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::default(),
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
            }],
```

- [ ] **Step 2.4.4: Run the two new tests**

Run: `cargo test -p xvision-engine agents::model::tests::slot_default_memory_mode_is_off agents::model::tests::slot_serde_default_memory_mode_is_off`
Expected: PASS.

- [ ] **Step 2.4.5: Fix any other call site that constructs `AgentSlot` literal-ly**

Run: `cargo check -p xvision-engine --tests`
For each error of the form `missing field 'memory_mode' in initializer of 'AgentSlot'`, add `memory_mode: xvision_memory::types::MemoryMode::default()` to that initializer. Common sites: `crates/xvision-engine/src/agents/templates.rs`, integration tests under `crates/xvision-engine/tests/`.

Expected after fixes: `cargo check -p xvision-engine --tests` PASS.

- [ ] **Step 2.4.6: Commit**

```bash
git add crates/xvision-engine/src/agents/
git commit -m "v2d: add AgentSlot.memory_mode field (default Off)"
```

### Task 2.5: Store roundtrip in `agents/store.rs`

- [ ] **Step 2.5.1: Write the failing test**

Append to `crates/xvision-engine/src/agents/store.rs` inside `#[cfg(test)] mod tests` (or to the existing slot-roundtrip test module — match the file's existing test style):

```rust
    #[tokio::test]
    async fn memory_mode_roundtrips_through_store() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../xvision-engine/migrations").run(&pool).await.unwrap();
        // (use whatever migrator helper this file's other tests use; match style)
        let store = AgentStore::new(pool);
        let mut agent = Agent::single_slot_default("01HZTEST00000000000000000", "t", "anthropic", "claude-sonnet-4-6");
        agent.slots[0].memory_mode = xvision_memory::types::MemoryMode::AgentScoped;
        store.upsert(&agent).await.unwrap();
        let back = store.get("01HZTEST00000000000000000").await.unwrap().unwrap();
        assert_eq!(back.slots[0].memory_mode, xvision_memory::types::MemoryMode::AgentScoped);
    }
```

(Contract author: match the exact helper names this file already uses for migrator setup + `AgentStore` construction. The structure above is illustrative; mirror the precedent in the file's existing `#[tokio::test]` blocks.)

- [ ] **Step 2.5.2: Run to verify it fails**

Run: `cargo test -p xvision-engine agents::store::tests::memory_mode_roundtrips_through_store`
Expected: FAIL (column read returns empty or store insert omits the field).

- [ ] **Step 2.5.3: Wire the column in `store.rs`**

In `crates/xvision-engine/src/agents/store.rs`:

- Locate the `INSERT INTO agent_slots` SQL. Add `memory_mode` to the column list AND a `?` to the values list, binding `slot.memory_mode.as_str()`.
- Locate the `SELECT ... FROM agent_slots` SQL. Add `memory_mode` to the projection AND read the column into the row tuple; reconstruct via `xvision_memory::types::MemoryMode::parse_or_off(&row.memory_mode_str)`.
- If a `Row::from_row` impl exists, add the field there too.

- [ ] **Step 2.5.4: Run to verify it passes**

Run: `cargo test -p xvision-engine agents::store::tests::memory_mode_roundtrips_through_store`
Expected: PASS.

Also run the full store test module to confirm no regression:
Run: `cargo test -p xvision-engine agents::store`
Expected: PASS.

- [ ] **Step 2.5.5: Commit**

```bash
git add crates/xvision-engine/src/agents/store.rs
git commit -m "v2d: store roundtrip for AgentSlot.memory_mode"
```

End of Phase 2. The slot field is persisted; nothing in the dispatcher uses it yet. Ready for Phase 3.

---

## Phase 3: `v2d-dispatcher-wiring`

Adds the auto-recall + auto-write seam to `execute_slot`. Emits `memory_recall` / `memory_write` events. No UI changes.

**Files:**
- Modify: `crates/xvision-engine/src/agent/execute.rs`
- Create: `crates/xvision-engine/src/agent/memory_recorder.rs`
- Modify: `crates/xvision-engine/src/agent/mod.rs` (or wherever `execute` is re-exported — add `memory_recorder` module)
- Create: `crates/xvision-engine/tests/agent_memory_dispatch.rs`

### Task 3.1: Author the memory_recorder module

- [ ] **Step 3.1.1: Write the failing test**

Create `crates/xvision-engine/tests/agent_memory_dispatch.rs`:

```rust
//! V2D dispatcher wiring — integration tests for memory recall/write.

use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMode};
use xvision_engine::agent::memory_recorder::{MemoryRecorder, RecallResult};

#[tokio::test]
async fn recall_returns_empty_when_mode_is_off() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let recorder = MemoryRecorder::new(std::sync::Arc::new(store));
    let r = recorder
        .recall(MemoryMode::Off, "agent-1", "any query text", 5)
        .await
        .unwrap();
    assert!(matches!(r, RecallResult::Skipped));
}

#[tokio::test]
async fn recall_returns_top_k_for_agent_scoped() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    // Pre-seed two items in the agent-scoped namespace.
    for (id, text) in [("m1", "first note"), ("m2", "second note")] {
        store.upsert(&MemoryItem {
            id: id.into(),
            namespace: "agent:agent-1".into(),
            text: text.into(),
            embedding: vec![1.0, 0.0],
            created_at: chrono::Utc::now(),
            source_run_id: None, source_cycle_id: None,
        }, "test-embedder").await.unwrap();
    }
    let recorder = MemoryRecorder::with_static_embedder(
        std::sync::Arc::new(store),
        "test-embedder",
        vec![1.0, 0.0],
    );
    let r = recorder
        .recall(MemoryMode::AgentScoped, "agent-1", "query", 5)
        .await
        .unwrap();
    match r {
        RecallResult::Hits { matches, namespace } => {
            assert_eq!(namespace, "agent:agent-1");
            assert_eq!(matches.len(), 2);
        }
        other => panic!("expected Hits, got {other:?}"),
    }
}

#[tokio::test]
async fn record_writes_into_correct_namespace() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let store_arc = std::sync::Arc::new(store);
    let recorder = MemoryRecorder::with_static_embedder(
        std::sync::Arc::clone(&store_arc),
        "test-embedder",
        vec![0.0, 1.0],
    );
    recorder
        .record(MemoryMode::AgentScoped, "agent-1", "decision text", None, None)
        .await
        .unwrap();
    let hits = store_arc.query("agent:agent-1", &[0.0, 1.0], 5).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text, "decision text");
}
```

- [ ] **Step 3.1.2: Run to verify it fails**

Run: `cargo test -p xvision-engine --test agent_memory_dispatch`
Expected: FAIL (`unresolved import xvision_engine::agent::memory_recorder`).

- [ ] **Step 3.1.3: Implement `memory_recorder.rs`**

Create `crates/xvision-engine/src/agent/memory_recorder.rs`:

```rust
//! V2D auto-recall + auto-write recorder.
//!
//! Sits between `execute_slot` and `xvision_memory::MemoryStore`.
//! Resolves the slot's `MemoryMode` + `agent_id` to a namespace,
//! runs a top-k recall before dispatch, and writes the post-dispatch
//! decision into the same namespace. The provider client used for
//! embeddings is injected — see Decision 3 in the V2D intake.

use std::sync::Arc;

use xvision_memory::embedder::{Embedder, StaticEmbedder};
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, MemoryMatch, MemoryMode, Namespace};

#[derive(Debug)]
pub enum RecallResult {
    /// `memory_mode == Off`. No recall attempted.
    Skipped,
    /// Recall completed; zero-or-more hits.
    Hits { namespace: String, matches: Vec<MemoryMatch> },
    /// Mode was non-off but no embedder is available for the slot's
    /// provider. Dispatcher emits a `memory_disabled_no_embedder`
    /// event and proceeds without prepending the prior-observations
    /// block.
    NoEmbedder { namespace: String },
}

pub struct MemoryRecorder {
    store: Arc<MemoryStore>,
    embedder: Option<Arc<dyn Embedder>>,
}

impl MemoryRecorder {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store, embedder: None }
    }

    pub fn with_embedder(store: Arc<MemoryStore>, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder: Some(embedder) }
    }

    pub fn with_static_embedder(store: Arc<MemoryStore>, id: &str, vector: Vec<f32>) -> Self {
        Self {
            store,
            embedder: Some(Arc::new(StaticEmbedder::new(id, vector))),
        }
    }

    pub async fn recall(
        &self,
        mode: MemoryMode,
        agent_id: &str,
        query_text: &str,
        k: usize,
    ) -> anyhow::Result<RecallResult> {
        let ns = Namespace::for_mode(mode, agent_id);
        if !ns.is_active() {
            return Ok(RecallResult::Skipped);
        }
        let Some(embedder) = &self.embedder else {
            return Ok(RecallResult::NoEmbedder { namespace: ns.as_str().to_string() });
        };
        let q = embedder.embed(query_text).await?;
        let hits = self.store.query(ns.as_str(), &q, k).await?;
        Ok(RecallResult::Hits { namespace: ns.as_str().to_string(), matches: hits })
    }

    pub async fn record(
        &self,
        mode: MemoryMode,
        agent_id: &str,
        decision_text: &str,
        source_run_id: Option<String>,
        source_cycle_id: Option<String>,
    ) -> anyhow::Result<Option<String>> {
        let ns = Namespace::for_mode(mode, agent_id);
        if !ns.is_active() {
            return Ok(None);
        }
        let Some(embedder) = &self.embedder else {
            return Ok(None);
        };
        let emb = embedder.embed(decision_text).await?;
        let id = ulid::Ulid::new().to_string();
        let item = MemoryItem {
            id: id.clone(),
            namespace: ns.as_str().to_string(),
            text: decision_text.to_string(),
            embedding: emb,
            created_at: chrono::Utc::now(),
            source_run_id,
            source_cycle_id,
        };
        self.store.upsert(&item, embedder.id()).await?;
        Ok(Some(id))
    }
}
```

Update `crates/xvision-engine/src/agent/mod.rs` (or whichever file declares the `agent` module surface) to add:

```rust
pub mod memory_recorder;
```

- [ ] **Step 3.1.4: Run to verify it passes**

Run: `cargo test -p xvision-engine --test agent_memory_dispatch`
Expected: PASS (all three tests).

- [ ] **Step 3.1.5: Commit**

```bash
git add crates/xvision-engine/src/agent/memory_recorder.rs \
        crates/xvision-engine/src/agent/mod.rs \
        crates/xvision-engine/tests/agent_memory_dispatch.rs
git commit -m "v2d: add MemoryRecorder (recall + record)"
```

### Task 3.2: Wire recall + record into `execute_slot`

- [ ] **Step 3.2.1: Write the failing test**

Append to `crates/xvision-engine/tests/agent_memory_dispatch.rs`:

```rust
use xvision_engine::agent::execute::{execute_slot, SlotInput};
// … plus whichever mock dispatch utilities live in `crates/xvision-engine/src/agent/llm.rs`
// (the file has a `MockDispatch` impl at :507 — reuse it).

#[tokio::test]
async fn execute_slot_prepends_prior_observations_when_agent_scoped() {
    // Arrange: pre-seed two memories, build a MemoryRecorder, attach to SlotInput.
    // Use MockDispatch to capture the assembled system_prompt that hit the wire.
    // Assert: captured system_prompt contains "<prior_observations>" and the two
    // pre-seeded items appear (in score order).
    // Implementation: mirror the existing MockDispatch-based test pattern in
    // crates/xvision-engine/tests/agent_slot_token_forward.rs.
    todo!("contract author: write per the comment block above");
}
```

(The contract author replaces the `todo!()` with an assembled test using the same patterns as `agent_slot_token_forward.rs`. The reason this plan stops short of writing the full test verbatim is that `SlotInput`'s constructor signature is the right place for the `memory: Option<Arc<MemoryRecorder>>` argument — the exact shape depends on the file's existing builder ergonomics, which the contract author decides in the next step.)

- [ ] **Step 3.2.2: Extend `SlotInput` with an optional `MemoryRecorder`**

In `crates/xvision-engine/src/agent/execute.rs`, add a field to `SlotInput`:

```rust
    /// Optional V2D memory recorder. `Some` enables auto-recall before
    /// the first dispatch iteration and auto-write after the final
    /// `EndTurn`. `None` (or a recorder whose mode is Off) is a no-op.
    pub memory: Option<std::sync::Arc<crate::agent::memory_recorder::MemoryRecorder>>,
    /// The slot's resolved memory mode (snapshotted from
    /// `AgentSlot.memory_mode` at dispatch time) + the owning agent id
    /// the recorder uses to derive the namespace. Two scalars instead
    /// of one because `execute_slot` is one level below where the slot
    /// + agent are joined and we don't want to plumb the Agent through.
    pub memory_mode: xvision_memory::types::MemoryMode,
    pub agent_id: String,
```

Update every call site that constructs `SlotInput` (grep: `SlotInput {`) to pass `memory: None, memory_mode: MemoryMode::Off, agent_id: <existing-id-or-empty>`. The pipeline call site (`crates/xvision-engine/src/agent/pipeline.rs`) is the one that wires the real recorder; tests pass `None`.

- [ ] **Step 3.2.3: Implement the recall + record seam**

In `execute_slot`, **before** the tool-use loop, after `slot` is bound but before the first `dispatch.call(...)`:

```rust
    // V2D: recall before dispatch.
    let prior_block = if let Some(recorder) = &input.memory {
        let query_text = serde_json::to_string(&input.upstream_inputs).unwrap_or_default();
        match recorder.recall(input.memory_mode, &input.agent_id, &query_text, 5).await? {
            RecallResult::Skipped => None,
            RecallResult::NoEmbedder { namespace } => {
                // Emit observability event so eval review can explain the gap.
                emitter.emit("memory_disabled_no_embedder", serde_json::json!({
                    "namespace": namespace,
                }));
                None
            }
            RecallResult::Hits { namespace, matches } => {
                emitter.emit("memory_recall", serde_json::json!({
                    "namespace": namespace,
                    "k": matches.len(),
                    "items": matches.iter().map(|m| serde_json::json!({
                        "id": m.id, "score": m.score, "text_preview": preview(&m.text),
                    })).collect::<Vec<_>>(),
                }));
                Some(render_prior_observations(&matches))
            }
        }
    } else {
        None
    };

    // Prepend to system_prompt for the LlmRequest builder.
    let assembled_system_prompt = match prior_block {
        Some(block) => format!("{block}\n\n{}", slot.system_prompt),
        None => slot.system_prompt.clone(),
    };
```

Add the two small helpers at the bottom of the file:

```rust
fn preview(text: &str) -> String {
    let mut s: String = text.chars().take(160).collect();
    if text.chars().count() > 160 { s.push('…'); }
    s
}

fn render_prior_observations(matches: &[xvision_memory::types::MemoryMatch]) -> String {
    let mut out = String::from("<prior_observations>\n");
    for m in matches {
        out.push_str("- ");
        out.push_str(&preview(&m.text));
        out.push('\n');
    }
    out.push_str("</prior_observations>");
    out
}
```

Use `assembled_system_prompt` in the existing `LlmRequest` builder in place of `slot.system_prompt.clone()`.

**After** the loop, once the model emits `EndTurn`, write back:

```rust
    // V2D: record the final decision text into the slot's namespace.
    if let Some(recorder) = &input.memory {
        // `final_text` is the last assistant message's text content
        // (the existing loop already accumulates this for the return
        // value — reuse the same binding).
        if !final_text.is_empty() {
            if let Some(id) = recorder
                .record(input.memory_mode, &input.agent_id, &final_text, /* run_id */ None, /* cycle_id */ None)
                .await?
            {
                emitter.emit("memory_write", serde_json::json!({
                    "namespace": Namespace::for_mode(input.memory_mode, &input.agent_id).as_str(),
                    "id": id,
                    "text_preview": preview(&final_text),
                }));
            }
        }
    }
```

(Contract author: `run_id` / `cycle_id` are threaded into the recorder from the pipeline call site. If `execute_slot` doesn't already have those bindings in scope, add `run_id: Option<String>` and `cycle_id: Option<String>` to `SlotInput` alongside `memory_mode` + `agent_id`. The eval executor is the call site that supplies them.)

- [ ] **Step 3.2.4: Run the integration test**

Run: `cargo test -p xvision-engine --test agent_memory_dispatch`
Expected: PASS.

Also run the broader engine test suite to catch any `SlotInput {` constructor that wasn't updated:
Run: `cargo test -p xvision-engine`
Expected: PASS.

- [ ] **Step 3.2.5: Commit**

```bash
git add crates/xvision-engine/src/agent/execute.rs \
        crates/xvision-engine/src/agent/pipeline.rs \
        crates/xvision-engine/tests/agent_memory_dispatch.rs
git commit -m "v2d: wire MemoryRecorder into execute_slot (recall + write)"
```

### Task 3.3: Open the memory store + embedder at engine startup

- [ ] **Step 3.3.1: Locate the engine startup site**

Grep: `grep -rn "AgentStore::new\|SqlitePool::connect" crates/xvision-engine/src/ | head -10`

Identify the function that constructs the engine's shared services (likely `crates/xvision-engine/src/lib.rs` or a `services.rs`). That function gains:

```rust
let memory_db_path = std::env::var("XVN_MEMORY_DB")
    .map(std::path::PathBuf::from)
    .unwrap_or_else(|_| dirs::home_dir().expect("home dir").join(".xvn/memory.db"));
let memory_store = std::sync::Arc::new(
    xvision_memory::store::MemoryStore::open(&memory_db_path).await?
);
let embedder: Option<std::sync::Arc<dyn xvision_memory::embedder::Embedder>> =
    build_default_embedder().await?;
let memory_recorder = std::sync::Arc::new(
    match &embedder {
        Some(e) => crate::agent::memory_recorder::MemoryRecorder::with_embedder(
            std::sync::Arc::clone(&memory_store),
            std::sync::Arc::clone(e),
        ),
        None => crate::agent::memory_recorder::MemoryRecorder::new(
            std::sync::Arc::clone(&memory_store),
        ),
    }
);
```

Thread `memory_recorder` into whatever service struct the pipeline pulls from (mirror the pattern used by `AgentStore` / `ToolRegistry`).

- [ ] **Step 3.3.2: Write a thin OpenAI embedder adapter**

Create `crates/xvision-engine/src/agent/openai_embedder.rs`:

```rust
//! Thin OpenAI embeddings adapter for V2D memory.
//!
//! Calls `POST /v1/embeddings` against the operator's configured OpenAI
//! base URL with model `text-embedding-3-small` (1536-dim). Returns the
//! single embedding vector or an error. The `Embedder` trait impl is
//! intentionally narrow; the `xvision-intern` chat-completion client is
//! not reused because the request/response shape is different and the
//! coupling cost outweighs the share.

use async_trait::async_trait;
use serde::Deserialize;
use xvision_memory::embedder::Embedder;

pub struct OpenAiEmbedder {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiEmbedder {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: "text-embedding-3-small".to_string(),
        }
    }
}

#[derive(Deserialize)]
struct EmbeddingsResponse { data: Vec<EmbeddingEntry> }
#[derive(Deserialize)]
struct EmbeddingEntry { embedding: Vec<f32> }

#[async_trait]
impl Embedder for OpenAiEmbedder {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({ "model": &self.model, "input": text });
        let resp: EmbeddingsResponse = self.client.post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await?
            .error_for_status()?
            .json().await?;
        Ok(resp.data.into_iter().next().map(|e| e.embedding).unwrap_or_default())
    }
    fn id(&self) -> &str { "openai:text-embedding-3-small" }
    fn dim(&self) -> usize { 1536 }
}
```

Wire `build_default_embedder` to read the operator's OpenAI provider config (via the existing `providers` crate) and return `Some(Arc::new(OpenAiEmbedder::new(...)))` if configured, `None` otherwise.

- [ ] **Step 3.3.3: Run the engine test suite**

Run: `cargo test -p xvision-engine`
Expected: PASS.

- [ ] **Step 3.3.4: Commit**

```bash
git add crates/xvision-engine/src/agent/openai_embedder.rs \
        crates/xvision-engine/src/lib.rs   # or wherever startup wires the recorder
git commit -m "v2d: open memory store + default OpenAI embedder at startup"
```

End of Phase 3. The engine now recalls + records when a slot has non-off `memory_mode` and an embedder is configured. UI still doesn't expose the toggle; eval review still doesn't render the events. Ready for Phases 4 + 5.

---

## Phase 4: `v2d-memory-mode-ui`

Adds the Memory selector in the AgentForm. Frontend-only after Phase 3.

**Files:**
- Modify: `frontend/web/src/components/agent/AgentForm.tsx`
- Modify: `frontend/web/src/components/agent/agents.test.tsx`
- Regenerated: `frontend/web/src/api/types.gen/AgentSlot.ts` (via the engine ts-export feature)
- Possibly create: `frontend/web/src/api/types.gen/MemoryMode.ts` (ts-rs output)

### Task 4.1: Regenerate the ts-rs surface

- [ ] **Step 4.1.1: Add the `ts-export` derive to `MemoryMode`**

In `crates/xvision-memory/src/types.rs`, gate `MemoryMode` behind the ts-export feature alongside the engine's existing pattern. **First** add a `ts-export` feature to `xvision-memory`:

`crates/xvision-memory/Cargo.toml`:

```toml
[features]
ts-export = ["ts-rs"]

[dependencies]
ts-rs = { version = "9", optional = true }
```

`crates/xvision-memory/src/types.rs` — extend the `MemoryMode` derive:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode { /* … */ }
```

In `crates/xvision-engine/Cargo.toml`, ensure the engine's `ts-export` feature forwards to `xvision-memory`:

```toml
[features]
ts-export = ["xvision-memory/ts-export", "ts-rs"]
```

- [ ] **Step 4.1.2: Run the ts-rs export**

Run: `cargo test -p xvision-engine --features ts-export ts_export`
(The engine's existing ts-export test triggers the codegen. If the project uses a different invocation — e.g. `cargo run -p xvision-engine --features ts-export --bin export-types` — match the convention in `frontend/web/src/api/types.gen/` neighbours.)

Verify the file appears:
Run: `ls frontend/web/src/api/types.gen/MemoryMode.ts`
Expected: file exists, contains `export type MemoryMode = "off" | "global" | "agent_scoped";`.

The `AgentSlot.ts` file should regenerate to include `memory_mode: MemoryMode`.

- [ ] **Step 4.1.3: Commit**

```bash
git add crates/xvision-memory/Cargo.toml crates/xvision-memory/src/types.rs \
        crates/xvision-engine/Cargo.toml \
        frontend/web/src/api/types.gen/MemoryMode.ts \
        frontend/web/src/api/types.gen/AgentSlot.ts
git commit -m "v2d: ts-export MemoryMode + regen AgentSlot.ts"
```

### Task 4.2: Memory selector in `AgentForm.tsx`

- [ ] **Step 4.2.1: Locate the existing slot field block**

Grep: `grep -n "max_tokens\|temperature\|inputs_policy\|bar_history_limit" frontend/web/src/components/agent/AgentForm.tsx | head -20`

This finds where the existing per-slot scalars are rendered. The new Memory selector lives in the same group.

- [ ] **Step 4.2.2: Write the failing test**

Append to `frontend/web/src/components/agent/agents.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { AgentForm } from "./AgentForm";

describe("AgentForm — memory selector (V2D)", () => {
    it("renders three memory mode options matching MemoryMode", () => {
        render(<AgentForm /* whatever props the existing tests use */ />);
        const select = screen.getByLabelText(/memory/i);
        expect(select).toBeInTheDocument();
        expect(screen.getByRole("option", { name: /off/i })).toBeInTheDocument();
        expect(screen.getByRole("option", { name: /global/i })).toBeInTheDocument();
        expect(screen.getByRole("option", { name: /agent-scoped/i })).toBeInTheDocument();
    });

    it("updates the slot draft when a mode is selected", () => {
        const onChange = jest.fn();
        render(<AgentForm onChange={onChange} /* … */ />);
        fireEvent.change(screen.getByLabelText(/memory/i), { target: { value: "agent_scoped" } });
        expect(onChange).toHaveBeenCalledWith(
            expect.objectContaining({
                slots: expect.arrayContaining([
                    expect.objectContaining({ memory_mode: "agent_scoped" }),
                ]),
            })
        );
    });
});
```

(Match the test file's existing setup helpers — the snippets above use generic shapes; the contract author uses the same render helper / `AgentForm` props pattern the file's earlier tests use.)

- [ ] **Step 4.2.3: Run to verify it fails**

Run: `pnpm --dir frontend/web test --run agents.test.tsx`
Expected: FAIL (no `memory` label in the form).

- [ ] **Step 4.2.4: Add the selector**

Edit `frontend/web/src/components/agent/AgentForm.tsx`. In the slot-field block (after the `max_tokens` / `temperature` row, matching the existing layout), add:

```tsx
<div className="grid grid-cols-2 gap-2">
    <label htmlFor={`slot-${idx}-memory`} className="text-sm">Memory</label>
    <select
        id={`slot-${idx}-memory`}
        aria-label="Memory mode"
        value={slot.memory_mode ?? "off"}
        onChange={(e) => updateSlot(idx, { memory_mode: e.target.value as MemoryMode })}
        className="rounded border bg-background px-2 py-1 text-sm"
    >
        <option value="off">Off</option>
        <option value="global">Global (shared across all agents)</option>
        <option value="agent_scoped">Agent-scoped (this agent only)</option>
    </select>
</div>
```

Import `MemoryMode` at the top:

```tsx
import type { MemoryMode } from "@/api/types.gen/MemoryMode";
```

(Use whichever path alias the file already uses for `types.gen` imports — match the file's precedent.)

- [ ] **Step 4.2.5: Run the tests**

Run: `pnpm --dir frontend/web test --run agents.test.tsx`
Expected: PASS.

Also typecheck:
Run: `pnpm --dir frontend/web typecheck`
Expected: PASS.

- [ ] **Step 4.2.6: Commit**

```bash
git add frontend/web/src/components/agent/AgentForm.tsx \
        frontend/web/src/components/agent/agents.test.tsx
git commit -m "v2d: memory selector in AgentForm"
```

End of Phase 4. The UI exposes the toggle; saving an agent with `memory_mode = agent_scoped` roundtrips through the engine PUT and is honored by the dispatcher.

---

## Phase 5: `v2d-eval-review-memory-surface`

Renders a Memory panel in the eval-review run detail UI showing the two new event kinds emitted by Phase 3.

**Files:**
- Modify: `frontend/web/src/components/eval-review/` (whichever file renders per-cycle event panels; locate with the grep below)
- Modify: relevant Vitest file under `frontend/web/src/components/eval-review/`

### Task 5.1: Locate the eval-review cycle detail panel

- [ ] **Step 5.1.1: Find the panel**

Run: `grep -rn "events.jsonl\|CycleEvent\|memory_recall" frontend/web/src/components/eval-review/ frontend/web/src/components/cycles/ 2>/dev/null`

Identify the component that already iterates over a cycle's events (likely something named `CycleEvents`, `DecisionDetail`, `RunDetail`, or similar). The Memory panel is a sibling to whatever already lists tool calls / findings.

### Task 5.2: Memory panel component

- [ ] **Step 5.2.1: Write the failing test**

Create or extend the relevant test file with:

```tsx
import { render, screen } from "@testing-library/react";
import { MemoryPanel } from "./MemoryPanel";

describe("MemoryPanel (V2D)", () => {
    const recall = {
        kind: "memory_recall",
        payload: {
            namespace: "agent:01HZTEST",
            k: 2,
            items: [
                { id: "m1", score: 0.92, text_preview: "noted last RSI cross was a fade" },
                { id: "m2", score: 0.71, text_preview: "stop tightened pre-event" },
            ],
        },
    };
    const write = {
        kind: "memory_write",
        payload: {
            namespace: "agent:01HZTEST",
            id: "m3",
            text_preview: "decided to hold; volatility expanding",
        },
    };

    it("renders nothing when no memory events present", () => {
        const { container } = render(<MemoryPanel events={[]} />);
        expect(container).toBeEmptyDOMElement();
    });

    it("renders recall + write rows when present", () => {
        render(<MemoryPanel events={[recall, write]} />);
        expect(screen.getByText(/agent:01HZTEST/)).toBeInTheDocument();
        expect(screen.getByText(/noted last RSI cross/)).toBeInTheDocument();
        expect(screen.getByText(/decided to hold/)).toBeInTheDocument();
    });

    it("renders a disabled-no-embedder banner when present", () => {
        const evt = { kind: "memory_disabled_no_embedder", payload: { namespace: "agent:01HZTEST" } };
        render(<MemoryPanel events={[evt]} />);
        expect(screen.getByText(/no embedder configured/i)).toBeInTheDocument();
    });
});
```

- [ ] **Step 5.2.2: Run to verify it fails**

Run: `pnpm --dir frontend/web test --run MemoryPanel`
Expected: FAIL (no `MemoryPanel` import target).

- [ ] **Step 5.2.3: Implement `MemoryPanel.tsx`**

Create `frontend/web/src/components/eval-review/MemoryPanel.tsx`:

```tsx
import type { FC } from "react";

type RecallItem = { id: string; score: number; text_preview: string };
type RecallPayload = { namespace: string; k: number; items: RecallItem[] };
type WritePayload = { namespace: string; id: string; text_preview: string };
type DisabledPayload = { namespace: string };

type MemoryEvent =
    | { kind: "memory_recall"; payload: RecallPayload }
    | { kind: "memory_write"; payload: WritePayload }
    | { kind: "memory_disabled_no_embedder"; payload: DisabledPayload };

export const MemoryPanel: FC<{ events: Array<{ kind: string; payload: unknown }> }> = ({ events }) => {
    const memEvents = events.filter(
        (e): e is MemoryEvent =>
            e.kind === "memory_recall" ||
            e.kind === "memory_write" ||
            e.kind === "memory_disabled_no_embedder"
    );
    if (memEvents.length === 0) return null;

    return (
        <section className="rounded border border-border p-3">
            <h4 className="mb-2 text-sm font-medium">Memory</h4>
            <ul className="space-y-2">
                {memEvents.map((e, i) => {
                    if (e.kind === "memory_recall") {
                        return (
                            <li key={i} className="text-xs">
                                <div className="text-muted-foreground">recall · {e.payload.namespace} · k={e.payload.k}</div>
                                <ul className="mt-1 space-y-1">
                                    {e.payload.items.map((it) => (
                                        <li key={it.id} className="flex gap-2">
                                            <span className="tabular-nums text-muted-foreground">{it.score.toFixed(2)}</span>
                                            <span>{it.text_preview}</span>
                                        </li>
                                    ))}
                                </ul>
                            </li>
                        );
                    }
                    if (e.kind === "memory_write") {
                        return (
                            <li key={i} className="text-xs">
                                <div className="text-muted-foreground">write · {e.payload.namespace}</div>
                                <div>{e.payload.text_preview}</div>
                            </li>
                        );
                    }
                    return (
                        <li key={i} className="text-xs text-amber-700 dark:text-amber-400">
                            No embedder configured · {e.payload.namespace}
                        </li>
                    );
                })}
            </ul>
        </section>
    );
};
```

- [ ] **Step 5.2.4: Wire the panel into the cycle detail view**

In the file located by Step 5.1.1, import and render `<MemoryPanel events={cycle.events} />` alongside the existing event panels.

- [ ] **Step 5.2.5: Run the tests + typecheck**

Run: `pnpm --dir frontend/web test --run MemoryPanel`
Expected: PASS.

Run: `pnpm --dir frontend/web typecheck`
Expected: PASS.

- [ ] **Step 5.2.6: Commit**

```bash
git add frontend/web/src/components/eval-review/MemoryPanel.tsx \
        frontend/web/src/components/eval-review/*.test.tsx \
        # plus the file modified in Step 5.2.4
git commit -m "v2d: render MemoryPanel in eval-review cycle detail"
```

End of Phase 5. V2D is functionally complete: the operator can toggle memory per slot, the dispatcher recalls + records on non-off slots, the eval review surface shows what got recalled and written.

---

## Verification — full V2D smoke

After all five phases land:

- [ ] **Step V.1: Workspace build + test**

Run: `cargo test --workspace`
Expected: PASS. New tests covered: 6 in `crates/xvision-memory/tests/store.rs`, 3 in `crates/xvision-engine/tests/agent_memory_dispatch.rs`, 2 in `crates/xvision-engine/src/agents/model.rs::tests`, 1 in `crates/xvision-engine/src/agents/store.rs::tests`.

- [ ] **Step V.2: Frontend typecheck + tests**

Run: `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test --run`
Expected: PASS. Three new test blocks: AgentForm memory selector (2), MemoryPanel (3).

- [ ] **Step V.3: Manual smoke against a fresh slot**

```bash
xvn dashboard &                           # local dashboard
# create a new agent in /agents, set one slot's Memory selector to "Agent-scoped"
# run an eval with that agent as the trader
# open the eval-review page for the run
# confirm: cycle detail shows "Memory" panel; first cycle is empty (cold), later
# cycles show recall items pointing at earlier decisions
xvn memory forget --agent <agent_id>      # confirm clear works
# re-run the same eval; first cycle is again cold
```

- [ ] **Step V.4: Board lint + manifest**

Run: `bash scripts/board-lint.sh`
Expected: PASS. The migration registry shows `026 | agent-slot-memory-mode | merged` once the migration phase merges; intermediate phases keep the row at `in flight`.

---

## What this plan does NOT cover

Per the intake's "Out of this intake" section: the `cortex-http` sidecar (deferred to v2 after F28), cross-host memory sharing, tool-driven `memory_recall` / `memory_write` (v1.1), cross-namespace retrieval blending (v1.1), embedder-swap re-embedding (v1.1), memory-aware findings (post-V2D), TTL / time decay (operator-driven forget is enough until V3), and any mem0 / Honcho / mempalace third-party adapter (the store's narrow public API leaves room without changing the dispatcher seam). The implementation plan above stays inside the V2D scope.

## What lands in `team/MANIFEST.md`

| #   | Owner                                | Status        |
|-----|--------------------------------------|---------------|
| 026 | agent-slot-memory-mode (V2D)         | in flight → merged |

The conductor flips status to `merged` when Phase 2 (the migration phase) ships its PR.
