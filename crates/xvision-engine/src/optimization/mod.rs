//! Optimization store (Phase 3.5).
//!
//! Durable, reproducible persistence for offline prompt/demonstration
//! optimization runs produced by the `xvision-dspy` optimizer.
//!
//! ## HARD INVARIANT — no `xvision-dspy` dependency
//!
//! This module lives in `xvision-engine` (the deploy-critical runtime crate),
//! so it MUST NOT import `xvision-dspy` or `dspy-rs`. The optimizer produces the
//! snapshot/demos/candidate strings on the CLI side; this store treats them as
//! opaque JSON blobs (`snapshot_json`, `payload_json`) plus scalar provenance
//! columns. Nothing here reaches into the optimizer's type system. That keeps
//! the heavy dspy-rs transitive tree out of the runtime binaries and the slim
//! deploy image, and keeps `cargo tree -p xvision-engine` free of dspy-rs.
//!
//! ## Reproducibility contract
//!
//! A run is reconstructable from its persisted inputs alone:
//!
//! * `corpus_query` — the trainset source.
//! * `rng_seed` — demo sampling / search order.
//! * `model_provider` / `model_name` — model identity.
//! * `optimizer` / `optimizer_version` — the search algorithm + its version.
//! * `signature_hash` — the bound signature shape.
//! * the accepted snapshot's `demos` (content-addressed in `optimization_demos`).
//! * `metric` — the objective that was maximized.
//!
//! [`OptimizationStore::reproduction_recipe`] returns exactly that tuple, and
//! the round-trip tests prove a run rebuilt from the recipe + its persisted
//! snapshot equals the original.
//!
//! ## Content addressing
//!
//! Demo sets are stored once in `optimization_demos`, keyed by the sha256 (hex)
//! of their canonical JSON. Candidates and snapshots reference a demo set by
//! that hash, so warm-started lineage reusing a demo set doesn't duplicate the
//! blob. [`demo_set_hash`] computes the content address.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use ulid::Ulid;

use crate::api::{ApiError, ApiResult};

/// A persisted optimization run header — the reproduction recipe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizationRun {
    /// ULID primary key.
    pub id: String,
    /// Agent template being optimized (pre-mint local id).
    pub agent_id: String,
    /// Free-text slot/role label within the agent.
    pub slot_name: String,
    /// Capability key: `trader`, `filter`, `decision_grader`, `chat_authoring`.
    pub capability: String,
    /// Optimizer name: `mipro`, `gepa`, `copro`.
    pub optimizer: String,
    /// Metric name maximized.
    pub metric: String,
    /// Saved-query id or serialized filter (opaque).
    pub corpus_query: String,
    /// RNG seed for demo sampling / search order.
    pub rng_seed: i64,
    /// Model provider key (e.g. `dummy`, `openai`).
    pub model_provider: Option<String>,
    /// Provider's model id.
    pub model_name: Option<String>,
    /// sha256 of the bound signature shape.
    pub signature_hash: Option<String>,
    /// Optimizer version / internal tag.
    pub optimizer_version: Option<String>,
    /// `pending` | `running` | `completed` | `failed`.
    pub status: String,
    /// RFC3339 UTC timestamp.
    pub created_at: String,
}

/// Request to create an optimization run row.
#[derive(Clone, Debug)]
pub struct NewOptimizationRun {
    pub agent_id: String,
    pub slot_name: String,
    pub capability: String,
    pub optimizer: String,
    pub metric: String,
    pub corpus_query: String,
    pub rng_seed: i64,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub signature_hash: Option<String>,
    pub optimizer_version: Option<String>,
}

/// A per-candidate search result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizationCandidate {
    pub id: String,
    pub run_id: String,
    pub candidate_index: i64,
    pub instruction: String,
    pub metric_value: Option<f64>,
    pub split: String,
    pub demo_set: Option<String>,
    pub selected: bool,
}

/// Request to create a candidate row.
#[derive(Clone, Debug)]
pub struct NewCandidate {
    pub candidate_index: i64,
    pub instruction: String,
    pub metric_value: Option<f64>,
    pub split: String,
    pub demo_set: Option<String>,
    pub selected: bool,
}

/// A persisted snapshot row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizationSnapshotRow {
    pub id: String,
    pub run_id: String,
    pub snapshot_json: String,
    pub signature_hash: String,
    pub demo_set: Option<String>,
    pub accepted: bool,
    pub created_at: String,
}

/// Request to create a snapshot row.
#[derive(Clone, Debug)]
pub struct NewSnapshot {
    /// The snapshot's own id (== the OptimizationSnapshot lineage id). The
    /// caller supplies it because the snapshot JSON already carries it.
    pub id: String,
    pub snapshot_json: String,
    pub signature_hash: String,
    pub demo_set: Option<String>,
}

/// A lineage edge: a child agent minted from an accepted optimization run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineageEdge {
    pub child_agent_id: String,
    pub parent_agent_id: String,
    pub optimization_run_id: String,
    pub created_at: String,
}

/// The minimal recipe required to reproduce a run, drawn from persisted inputs.
/// Equal recipes + equal accepted snapshot ⇒ the run is reconstructable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReproductionRecipe {
    pub corpus_query: String,
    pub rng_seed: i64,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub optimizer: String,
    pub optimizer_version: Option<String>,
    pub signature_hash: Option<String>,
    pub metric: String,
}

/// Content address (sha256 hex) of a canonical demos JSON string.
///
/// The caller passes the already-canonicalized demos JSON (the optimizer's
/// snapshot serializes deterministically via sorted `BTreeMap`s), so equal demo
/// sets hash identically regardless of who produced them.
pub fn demo_set_hash(payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"xvision.optimization.demo_set.v1\n");
    hasher.update(payload_json.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &b in digest.iter() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// CRUD surface over the optimization-store tables. Thin wrapper around a
/// `SqlitePool`; holds no state beyond the pool handle.
#[derive(Clone)]
pub struct OptimizationStore {
    pool: SqlitePool,
}

impl OptimizationStore {
    /// Build a store over an existing pool. The pool must already have had
    /// migration 045 applied (via `ApiContext::open`).
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert a new run with a freshly-minted ULID id and `pending` status.
    /// Returns the full persisted row.
    pub async fn create_run(&self, req: NewOptimizationRun) -> ApiResult<OptimizationRun> {
        let id = Ulid::new().to_string();
        let created_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO optimization_runs \
             (id, agent_id, slot_name, capability, optimizer, metric, corpus_query, \
              rng_seed, model_provider, model_name, signature_hash, optimizer_version, \
              status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
        )
        .bind(&id)
        .bind(&req.agent_id)
        .bind(&req.slot_name)
        .bind(&req.capability)
        .bind(&req.optimizer)
        .bind(&req.metric)
        .bind(&req.corpus_query)
        .bind(req.rng_seed)
        .bind(&req.model_provider)
        .bind(&req.model_name)
        .bind(&req.signature_hash)
        .bind(&req.optimizer_version)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;

        Ok(OptimizationRun {
            id,
            agent_id: req.agent_id,
            slot_name: req.slot_name,
            capability: req.capability,
            optimizer: req.optimizer,
            metric: req.metric,
            corpus_query: req.corpus_query,
            rng_seed: req.rng_seed,
            model_provider: req.model_provider,
            model_name: req.model_name,
            signature_hash: req.signature_hash,
            optimizer_version: req.optimizer_version,
            status: "pending".to_string(),
            created_at,
        })
    }

    /// Fetch a run by id. `NotFound` if absent.
    pub async fn get_run(&self, id: &str) -> ApiResult<OptimizationRun> {
        let row: Option<OptimizationRun> = sqlx::query_as::<_, OptRunRow>(
            "SELECT id, agent_id, slot_name, capability, optimizer, metric, corpus_query, \
                    rng_seed, model_provider, model_name, signature_hash, optimizer_version, \
                    status, created_at \
             FROM optimization_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(Into::into);
        row.ok_or_else(|| ApiError::NotFound(format!("optimization_run {id}")))
    }

    /// List runs for an agent (optionally filtered by slot), newest first.
    pub async fn list_runs_for_agent(
        &self,
        agent_id: &str,
        slot_name: Option<&str>,
    ) -> ApiResult<Vec<OptimizationRun>> {
        let rows: Vec<OptRunRow> = match slot_name {
            Some(slot) => {
                sqlx::query_as(
                    "SELECT id, agent_id, slot_name, capability, optimizer, metric, corpus_query, \
                            rng_seed, model_provider, model_name, signature_hash, optimizer_version, \
                            status, created_at \
                     FROM optimization_runs WHERE agent_id = ? AND slot_name = ? \
                     ORDER BY created_at DESC, id DESC",
                )
                .bind(agent_id)
                .bind(slot)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, agent_id, slot_name, capability, optimizer, metric, corpus_query, \
                            rng_seed, model_provider, model_name, signature_hash, optimizer_version, \
                            status, created_at \
                     FROM optimization_runs WHERE agent_id = ? \
                     ORDER BY created_at DESC, id DESC",
                )
                .bind(agent_id)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Update a run's status (`pending`/`running`/`completed`/`failed`).
    pub async fn set_run_status(&self, id: &str, status: &str) -> ApiResult<()> {
        let res = sqlx::query("UPDATE optimization_runs SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("optimization_run {id}")));
        }
        Ok(())
    }

    /// The reproduction recipe for a run.
    pub async fn reproduction_recipe(&self, id: &str) -> ApiResult<ReproductionRecipe> {
        let run = self.get_run(id).await?;
        Ok(ReproductionRecipe {
            corpus_query: run.corpus_query,
            rng_seed: run.rng_seed,
            model_provider: run.model_provider,
            model_name: run.model_name,
            optimizer: run.optimizer,
            optimizer_version: run.optimizer_version,
            signature_hash: run.signature_hash,
            metric: run.metric,
        })
    }

    /// Insert a candidate row under a run.
    pub async fn add_candidate(&self, run_id: &str, req: NewCandidate) -> ApiResult<OptimizationCandidate> {
        let id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO optimization_candidates \
             (id, run_id, candidate_index, instruction, metric_value, split, demo_set, selected) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(run_id)
        .bind(req.candidate_index)
        .bind(&req.instruction)
        .bind(req.metric_value)
        .bind(&req.split)
        .bind(&req.demo_set)
        .bind(req.selected as i64)
        .execute(&self.pool)
        .await?;
        Ok(OptimizationCandidate {
            id,
            run_id: run_id.to_string(),
            candidate_index: req.candidate_index,
            instruction: req.instruction,
            metric_value: req.metric_value,
            split: req.split,
            demo_set: req.demo_set,
            selected: req.selected,
        })
    }

    /// Mark exactly one candidate (by index) as the run's selected winner,
    /// clearing the flag on all others under the same run.
    pub async fn mark_candidate_selected(&self, run_id: &str, candidate_index: i64) -> ApiResult<()> {
        sqlx::query("UPDATE optimization_candidates SET selected = 0 WHERE run_id = ?")
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        let res = sqlx::query(
            "UPDATE optimization_candidates SET selected = 1 \
             WHERE run_id = ? AND candidate_index = ?",
        )
        .bind(run_id)
        .bind(candidate_index)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!(
                "optimization_candidate run={run_id} index={candidate_index}"
            )));
        }
        Ok(())
    }

    /// List a run's candidates by ascending candidate_index.
    pub async fn list_candidates(&self, run_id: &str) -> ApiResult<Vec<OptimizationCandidate>> {
        let rows: Vec<OptCandidateRow> = sqlx::query_as(
            "SELECT id, run_id, candidate_index, instruction, metric_value, split, demo_set, selected \
             FROM optimization_candidates WHERE run_id = ? ORDER BY candidate_index ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Store a demo set content-addressed. Returns the content hash. Idempotent:
    /// re-storing identical canonical JSON is a no-op and returns the same hash.
    pub async fn put_demo_set(&self, payload_json: &str) -> ApiResult<String> {
        let hash = demo_set_hash(payload_json);
        let created_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO optimization_demos (demo_set, payload_json, created_at) \
             VALUES (?, ?, ?)",
        )
        .bind(&hash)
        .bind(payload_json)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;
        Ok(hash)
    }

    /// Fetch a demo set's canonical JSON by content hash.
    pub async fn get_demo_set(&self, demo_set: &str) -> ApiResult<String> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT payload_json FROM optimization_demos WHERE demo_set = ?")
                .bind(demo_set)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(p,)| p)
            .ok_or_else(|| ApiError::NotFound(format!("optimization_demos {demo_set}")))
    }

    /// Insert a snapshot row under a run.
    pub async fn add_snapshot(&self, run_id: &str, req: NewSnapshot) -> ApiResult<OptimizationSnapshotRow> {
        let created_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO optimization_snapshots \
             (id, run_id, snapshot_json, signature_hash, demo_set, accepted, created_at) \
             VALUES (?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(&req.id)
        .bind(run_id)
        .bind(&req.snapshot_json)
        .bind(&req.signature_hash)
        .bind(&req.demo_set)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;
        Ok(OptimizationSnapshotRow {
            id: req.id,
            run_id: run_id.to_string(),
            snapshot_json: req.snapshot_json,
            signature_hash: req.signature_hash,
            demo_set: req.demo_set,
            accepted: false,
            created_at,
        })
    }

    /// Fetch a snapshot by id.
    pub async fn get_snapshot(&self, id: &str) -> ApiResult<OptimizationSnapshotRow> {
        let row: Option<OptSnapshotRow> = sqlx::query_as(
            "SELECT id, run_id, snapshot_json, signature_hash, demo_set, accepted, created_at \
             FROM optimization_snapshots WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Into::into)
            .ok_or_else(|| ApiError::NotFound(format!("optimization_snapshot {id}")))
    }

    /// List a run's snapshots, newest first.
    pub async fn list_snapshots(&self, run_id: &str) -> ApiResult<Vec<OptimizationSnapshotRow>> {
        let rows: Vec<OptSnapshotRow> = sqlx::query_as(
            "SELECT id, run_id, snapshot_json, signature_hash, demo_set, accepted, created_at \
             FROM optimization_snapshots WHERE run_id = ? ORDER BY created_at DESC, id DESC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Set a snapshot's `accepted` flag.
    pub async fn set_snapshot_accepted(&self, id: &str, accepted: bool) -> ApiResult<()> {
        let res = sqlx::query("UPDATE optimization_snapshots SET accepted = ? WHERE id = ?")
            .bind(accepted as i64)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("optimization_snapshot {id}")));
        }
        Ok(())
    }

    /// Record a lineage edge (child agent minted from an accepted run).
    /// `Conflict` if the child agent already has a lineage row.
    pub async fn add_lineage(
        &self,
        child_agent_id: &str,
        parent_agent_id: &str,
        optimization_run_id: &str,
    ) -> ApiResult<LineageEdge> {
        let created_at = Utc::now().to_rfc3339();
        let res = sqlx::query(
            "INSERT INTO agent_lineage \
             (child_agent_id, parent_agent_id, optimization_run_id, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(child_agent_id)
        .bind(parent_agent_id)
        .bind(optimization_run_id)
        .bind(&created_at)
        .execute(&self.pool)
        .await;
        match res {
            Ok(_) => Ok(LineageEdge {
                child_agent_id: child_agent_id.to_string(),
                parent_agent_id: parent_agent_id.to_string(),
                optimization_run_id: optimization_run_id.to_string(),
                created_at,
            }),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => Err(ApiError::Conflict(format!(
                "agent_lineage child_agent_id {child_agent_id} already recorded"
            ))),
            Err(e) => Err(e.into()),
        }
    }

    /// Fetch the lineage edge for a child agent, if any.
    pub async fn get_lineage_for_child(&self, child_agent_id: &str) -> ApiResult<Option<LineageEdge>> {
        let row: Option<LineageRow> = sqlx::query_as(
            "SELECT child_agent_id, parent_agent_id, optimization_run_id, created_at \
             FROM agent_lineage WHERE child_agent_id = ?",
        )
        .bind(child_agent_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// List children of a parent agent.
    pub async fn list_children(&self, parent_agent_id: &str) -> ApiResult<Vec<LineageEdge>> {
        let rows: Vec<LineageRow> = sqlx::query_as(
            "SELECT child_agent_id, parent_agent_id, optimization_run_id, created_at \
             FROM agent_lineage WHERE parent_agent_id = ? ORDER BY created_at ASC",
        )
        .bind(parent_agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Delete a lineage edge for a child agent (used by `revert accepted`).
    /// `NotFound` if absent.
    pub async fn delete_lineage_for_child(&self, child_agent_id: &str) -> ApiResult<()> {
        let res = sqlx::query("DELETE FROM agent_lineage WHERE child_agent_id = ?")
            .bind(child_agent_id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!(
                "agent_lineage child_agent_id {child_agent_id}"
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// sqlx row shims. SQLite stores bools as INTEGER; we map through dedicated row
// structs so the public types can carry `bool` / clean `Option` fields.
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct OptRunRow {
    id: String,
    agent_id: String,
    slot_name: String,
    capability: String,
    optimizer: String,
    metric: String,
    corpus_query: String,
    rng_seed: i64,
    model_provider: Option<String>,
    model_name: Option<String>,
    signature_hash: Option<String>,
    optimizer_version: Option<String>,
    status: String,
    created_at: String,
}

impl From<OptRunRow> for OptimizationRun {
    fn from(r: OptRunRow) -> Self {
        OptimizationRun {
            id: r.id,
            agent_id: r.agent_id,
            slot_name: r.slot_name,
            capability: r.capability,
            optimizer: r.optimizer,
            metric: r.metric,
            corpus_query: r.corpus_query,
            rng_seed: r.rng_seed,
            model_provider: r.model_provider,
            model_name: r.model_name,
            signature_hash: r.signature_hash,
            optimizer_version: r.optimizer_version,
            status: r.status,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct OptCandidateRow {
    id: String,
    run_id: String,
    candidate_index: i64,
    instruction: String,
    metric_value: Option<f64>,
    split: String,
    demo_set: Option<String>,
    selected: i64,
}

impl From<OptCandidateRow> for OptimizationCandidate {
    fn from(r: OptCandidateRow) -> Self {
        OptimizationCandidate {
            id: r.id,
            run_id: r.run_id,
            candidate_index: r.candidate_index,
            instruction: r.instruction,
            metric_value: r.metric_value,
            split: r.split,
            demo_set: r.demo_set,
            selected: r.selected != 0,
        }
    }
}

#[derive(sqlx::FromRow)]
struct OptSnapshotRow {
    id: String,
    run_id: String,
    snapshot_json: String,
    signature_hash: String,
    demo_set: Option<String>,
    accepted: i64,
    created_at: String,
}

impl From<OptSnapshotRow> for OptimizationSnapshotRow {
    fn from(r: OptSnapshotRow) -> Self {
        OptimizationSnapshotRow {
            id: r.id,
            run_id: r.run_id,
            snapshot_json: r.snapshot_json,
            signature_hash: r.signature_hash,
            demo_set: r.demo_set,
            accepted: r.accepted != 0,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LineageRow {
    child_agent_id: String,
    parent_agent_id: String,
    optimization_run_id: String,
    created_at: String,
}

impl From<LineageRow> for LineageEdge {
    fn from(r: LineageRow) -> Self {
        LineageEdge {
            child_agent_id: r.child_agent_id,
            parent_agent_id: r.parent_agent_id,
            optimization_run_id: r.optimization_run_id,
            created_at: r.created_at,
        }
    }
}

#[cfg(test)]
mod tests;
