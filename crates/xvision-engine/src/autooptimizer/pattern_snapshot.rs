use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use ulid::Ulid;

/// Token and cost provenance for a single compile pass.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Provenance {
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost_micros_usd: u64,
}

impl Provenance {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn record_usage(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        self.prompt_tokens += prompt_tokens as u64;
        self.completion_tokens += completion_tokens as u64;
    }

    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// One observation that was compiled into a pattern, with an optional per-example
/// score from the GEPA scorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDemo {
    pub observation_id: String,
    pub text: String,
    pub score: Option<f64>,
}

/// Persistent record of a compiled pattern: captures what instruction was
/// produced, which observations fed in, who compiled it, and the lineage DAG
/// link back to the prior snapshot for this namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSnapshot {
    pub id: String,
    pub namespace: String,
    pub instruction: String,
    pub demos: Vec<SnapshotDemo>,
    pub signature_hash: String,
    pub metric_name: String,
    pub optimizer_name: String,
    pub optimizer_version: String,
    pub provenance: Provenance,
    pub rng_seed: u64,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PatternSnapshot {
    pub fn new(
        namespace: impl Into<String>,
        instruction: impl Into<String>,
        demos: Vec<SnapshotDemo>,
        metric_name: impl Into<String>,
        optimizer_name: impl Into<String>,
        provenance: Provenance,
        rng_seed: u64,
        parent_id: Option<String>,
    ) -> Self {
        let ns = namespace.into();
        let sig = signature_hash(&ns);
        Self {
            id: Ulid::new().to_string(),
            namespace: ns,
            instruction: instruction.into(),
            demos,
            signature_hash: sig,
            metric_name: metric_name.into(),
            optimizer_name: optimizer_name.into(),
            optimizer_version: env!("CARGO_PKG_VERSION").to_string(),
            provenance,
            rng_seed,
            parent_id,
            created_at: Utc::now(),
        }
    }
}

/// SHA-256 of a domain-separated namespace string. Used to detect staleness when
/// the capability context (namespace) drifts between compile passes.
pub fn signature_hash(namespace: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"xvision-engine.pattern.v1:");
    hasher.update(namespace.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// SQLite-backed store for `PatternSnapshot` records, written to xvision.db
/// alongside the other autooptimizer tables.
pub struct PatternSnapshotStore {
    pool: SqlitePool,
}

impl PatternSnapshotStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, snapshot: &PatternSnapshot) -> anyhow::Result<()> {
        let demos_json = serde_json::to_string(&snapshot.demos)?;
        let provenance_json = serde_json::to_string(&snapshot.provenance)?;
        let created_at = snapshot.created_at.to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO autooptimizer_pattern_snapshots \
             (id, namespace, instruction, demos_json, signature_hash, metric_name, \
              optimizer_name, optimizer_version, provenance_json, rng_seed, parent_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&snapshot.id)
        .bind(&snapshot.namespace)
        .bind(&snapshot.instruction)
        .bind(&demos_json)
        .bind(&snapshot.signature_hash)
        .bind(&snapshot.metric_name)
        .bind(&snapshot.optimizer_name)
        .bind(&snapshot.optimizer_version)
        .bind(&provenance_json)
        .bind(snapshot.rng_seed as i64)
        .bind(&snapshot.parent_id)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn latest_for_namespace(&self, namespace: &str) -> anyhow::Result<Option<PatternSnapshot>> {
        let row = sqlx::query(
            "SELECT id, namespace, instruction, demos_json, signature_hash, metric_name, \
              optimizer_name, optimizer_version, provenance_json, rng_seed, parent_id, created_at \
             FROM autooptimizer_pattern_snapshots \
             WHERE namespace = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| Self::row_to_snapshot(r)).transpose()
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Option<PatternSnapshot>> {
        let row = sqlx::query(
            "SELECT id, namespace, instruction, demos_json, signature_hash, metric_name, \
              optimizer_name, optimizer_version, provenance_json, rng_seed, parent_id, created_at \
             FROM autooptimizer_pattern_snapshots \
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| Self::row_to_snapshot(r)).transpose()
    }

    fn row_to_snapshot(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<PatternSnapshot> {
        use sqlx::Row;
        let demos_json: String = row.try_get("demos_json")?;
        let provenance_json: String = row.try_get("provenance_json")?;
        let created_at_str: String = row.try_get("created_at")?;
        let rng_seed_i64: i64 = row.try_get("rng_seed")?;
        Ok(PatternSnapshot {
            id: row.try_get("id")?,
            namespace: row.try_get("namespace")?,
            instruction: row.try_get("instruction")?,
            demos: serde_json::from_str(&demos_json)?,
            signature_hash: row.try_get("signature_hash")?,
            metric_name: row.try_get("metric_name")?,
            optimizer_name: row.try_get("optimizer_name")?,
            optimizer_version: row.try_get("optimizer_version")?,
            provenance: serde_json::from_str(&provenance_json)?,
            rng_seed: rng_seed_i64 as u64,
            parent_id: row.try_get("parent_id")?,
            created_at: created_at_str.parse()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_hash_is_deterministic() {
        let h1 = signature_hash("autooptimizer:dspy");
        let h2 = signature_hash("autooptimizer:dspy");
        assert_eq!(h1, h2, "signature_hash must be deterministic");
    }

    #[test]
    fn signature_hash_varies_with_namespace() {
        let h1 = signature_hash("autooptimizer:dspy");
        let h2 = signature_hash("autooptimizer:other");
        assert_ne!(h1, h2, "different namespaces must produce different hashes");
    }

    #[test]
    fn provenance_record_usage_accumulates() {
        let mut p = Provenance::new("test", "model");
        p.record_usage(10, 20);
        p.record_usage(5, 7);
        assert_eq!(p.prompt_tokens, 15);
        assert_eq!(p.completion_tokens, 27);
        assert_eq!(p.total_tokens(), 42);
    }

    #[test]
    fn snapshot_demo_round_trips_json() {
        let demo = SnapshotDemo {
            observation_id: "01HWOBS1".to_string(),
            text: "raised threshold improved Sharpe".to_string(),
            score: Some(0.85),
        };
        let json = serde_json::to_string(&demo).unwrap();
        let round: SnapshotDemo = serde_json::from_str(&json).unwrap();
        assert_eq!(round.observation_id, demo.observation_id);
        assert_eq!(round.score, demo.score);
    }

    #[tokio::test]
    async fn store_insert_and_query() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE autooptimizer_pattern_snapshots (
                id TEXT PRIMARY KEY,
                namespace TEXT NOT NULL,
                instruction TEXT NOT NULL,
                demos_json TEXT NOT NULL,
                signature_hash TEXT NOT NULL,
                metric_name TEXT NOT NULL,
                optimizer_name TEXT NOT NULL,
                optimizer_version TEXT NOT NULL,
                provenance_json TEXT NOT NULL,
                rng_seed INTEGER NOT NULL DEFAULT 0,
                parent_id TEXT,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = PatternSnapshotStore::new(pool);
        let snap = PatternSnapshot::new(
            "autooptimizer:dspy",
            "prefer high-conviction setups",
            vec![SnapshotDemo {
                observation_id: "obs1".to_string(),
                text: "high conviction improved".to_string(),
                score: Some(0.9),
            }],
            "delta_sharpe",
            "gepa",
            Provenance::new("test", "model"),
            0,
            None,
        );
        let snap_id = snap.id.clone();
        store.insert(&snap).await.unwrap();

        let fetched = store.latest_for_namespace("autooptimizer:dspy").await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, snap_id);
        assert_eq!(fetched.instruction, "prefer high-conviction setups");
        assert_eq!(fetched.demos.len(), 1);
        assert_eq!(fetched.demos[0].score, Some(0.9));

        let by_id = store.get(&snap_id).await.unwrap();
        assert!(by_id.is_some());
        assert_eq!(by_id.unwrap().namespace, "autooptimizer:dspy");

        let missing = store.latest_for_namespace("nonexistent").await.unwrap();
        assert!(missing.is_none());
    }
}
