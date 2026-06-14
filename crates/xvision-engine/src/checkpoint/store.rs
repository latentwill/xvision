//! `Checkpointer` — content-addressed snapshot + verbatim restore of a chat
//! session's mutable authoring artifacts.
//!
//! See the module docs for the design. This file owns the on-disk + DB layout:
//! blobs in a [`BlobStore`], a manifest row in the `chat_checkpoints` table
//! (migration 044 — named to avoid colliding with the agent-run `checkpoints`
//! table from migration 018), and verbatim restore back onto the strategy
//! filesystem, the `agent_slots` rows, and the session's tool-policy / focus
//! columns.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use thiserror::Error;
use ulid::Ulid;
use xvision_observability::{BlobRef, BlobStore, BlobStoreError};

use crate::agents::store::{AgentStore, UpdateAgent};
use crate::agents::AgentSlot;
use crate::chat_session::ChatSessionStore;
use crate::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use crate::strategies::Strategy;

/// Why a checkpoint was taken. Stored as the snake_case discriminant in the
/// `checkpoints.kind` column. Free text at the schema level — the rail renders
/// it; the engine does not branch on it. `Other` carries an arbitrary label so
/// callers are never forced to extend this enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    /// Snapshot taken automatically before a mutating tool runs (the rail hook).
    PreTool,
    /// Operator-requested manual checkpoint.
    Manual,
    /// Any other caller-defined kind.
    Other(String),
}

impl CheckpointKind {
    fn as_db_str(&self) -> String {
        match self {
            CheckpointKind::PreTool => "pre_tool".to_string(),
            CheckpointKind::Manual => "manual".to_string(),
            CheckpointKind::Other(s) => s.clone(),
        }
    }

    fn from_db_str(s: &str) -> Self {
        match s {
            "pre_tool" => CheckpointKind::PreTool,
            "manual" => CheckpointKind::Manual,
            other => CheckpointKind::Other(other.to_string()),
        }
    }
}

/// One artifact captured in a checkpoint. The blob hash points at the verbatim
/// payload bytes in the [`BlobStore`]; the inline metadata records where the
/// bytes are written back on restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapturedArtifact {
    /// A filesystem Strategy JSON. Restored verbatim to
    /// `$xvn_home/strategies/<id>.json`. `was_absent` records that the
    /// file did not exist at snapshot time (a CREATE-like pre-state):
    /// the blob is empty and restore deletes the file rather than
    /// writing it back.
    Strategy {
        id: String,
        blob_hash: String,
        #[serde(default)]
        was_absent: bool,
    },
    /// An agent's full `Vec<AgentSlot>`, captured as JSON. Restored via
    /// `AgentStore::update` (full slot replace, validated, non-destructive).
    AgentSlots { agent_id: String, blob_hash: String },
    /// The session's `tool_policy_json`. `blob_hash` is the verbatim policy
    /// JSON string (even when it was `NULL`, captured as the empty marker so
    /// restore can rewind back to `NULL`).
    ToolPolicy {
        /// `true` when the column was `NULL` at snapshot time; restore writes
        /// `NULL` back and ignores the blob.
        was_null: bool,
        blob_hash: String,
    },
    /// The session's focus file. `path` is the focus path relative to
    /// `$xvn_home`; the blob is the verbatim file contents. `was_set` is
    /// `false` when the session had no focus path (restore clears it).
    Focus {
        was_set: bool,
        path: String,
        blob_hash: String,
    },
}

impl CapturedArtifact {
    /// Short artifact-kind label surfaced in `CheckpointRestored.restored`.
    pub fn label(&self) -> &'static str {
        match self {
            CapturedArtifact::Strategy { .. } => "strategy",
            CapturedArtifact::AgentSlots { .. } => "agent_slots",
            CapturedArtifact::ToolPolicy { .. } => "tool_policy",
            CapturedArtifact::Focus { .. } => "focus",
        }
    }

    fn blob_hash(&self) -> &str {
        match self {
            CapturedArtifact::Strategy { blob_hash, .. }
            | CapturedArtifact::AgentSlots { blob_hash, .. }
            | CapturedArtifact::ToolPolicy { blob_hash, .. }
            | CapturedArtifact::Focus { blob_hash, .. } => blob_hash,
        }
    }
}

/// The persisted checkpoint manifest. Serializes to `checkpoints.captured_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedManifest {
    pub artifacts: Vec<CapturedArtifact>,
}

/// A loaded checkpoint row + its decoded manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub checkpoint_id: String,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub kind: CheckpointKind,
    pub content_hash: String,
    pub label: Option<String>,
    pub artifacts: Vec<CapturedArtifact>,
}

/// Result of a successful restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreOutcome {
    pub checkpoint_id: String,
    pub session_id: String,
    /// Artifact-kind labels rewound, e.g. `["strategy", "agent_slots"]`.
    pub restored: Vec<String>,
}

/// Typed, never-silent checkpoint errors. Restore failures here are
/// non-destructive: the validation phase runs before any write, so a returned
/// error means nothing was rewound.
#[derive(Debug, Error)]
pub enum CheckpointError {
    /// No checkpoint row with this id (e.g. restoring an event that never had a
    /// checkpoint). Non-destructive.
    #[error("checkpoint not found: {0}")]
    NotFound(String),
    /// A referenced blob is missing from the store, so the snapshot cannot be
    /// faithfully rewound. Detected in the up-front validation phase, before any
    /// artifact is written — non-destructive.
    #[error("checkpoint {checkpoint_id} references missing blob {blob_hash} for {artifact}")]
    MissingBlob {
        checkpoint_id: String,
        artifact: &'static str,
        blob_hash: String,
    },
    /// An artifact failed to restore (e.g. agent slots rejected by the save-gate
    /// validator). Slot/policy writes validate before mutating, so this surfaces
    /// without a half-applied restore.
    #[error("restore of {artifact} for checkpoint {checkpoint_id} failed: {source}")]
    Restore {
        checkpoint_id: String,
        artifact: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("blob store: {0}")]
    Blob(#[from] BlobStoreError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CheckpointError {
    /// Stable machine code for the `CheckpointRestoreFailed` event.
    pub fn code(&self) -> &'static str {
        match self {
            CheckpointError::NotFound(_) => "checkpoint_not_found",
            CheckpointError::MissingBlob { .. } => "checkpoint_missing_blob",
            CheckpointError::Restore { .. } => "checkpoint_restore_failed",
            CheckpointError::Blob(_) => "checkpoint_blob_error",
            CheckpointError::Db(_) => "checkpoint_db_error",
            CheckpointError::Other(_) => "checkpoint_internal",
        }
    }
}

/// What to capture in a snapshot. Each `Some` field is captured; `None` is
/// skipped. The rail hook builds this from the tool about to run (e.g. a
/// `create_strategy`/`update_strategy` tool requests the Strategy + focus +
/// tool policy).
#[derive(Debug, Clone, Default)]
pub struct SnapshotRequest {
    /// Capture the Strategy JSON for this id (filesystem).
    pub strategy_id: Option<String>,
    /// Capture this agent's slot rows (DB).
    pub agent_id: Option<String>,
    /// Capture the session's tool policy.
    pub tool_policy: bool,
    /// Capture the session's focus file.
    pub focus: bool,
    /// Optional operator-facing label for the checkpoint.
    pub label: Option<String>,
}

/// Content-addressed snapshot + restore engine. Constructed per `xvn_home` so
/// the strategy filesystem, the blob store, and the DB pool all agree on roots.
pub struct Checkpointer {
    pool: SqlitePool,
    blobs: BlobStore,
    xvn_home: PathBuf,
}

impl Checkpointer {
    /// Construct a `Checkpointer` rooted at `xvn_home`. Checkpoint blobs live in
    /// their own content-addressed namespace at `$xvn_home/checkpoints/blobs/`,
    /// separate from agent-run payload blobs.
    pub fn new(pool: SqlitePool, xvn_home: impl Into<PathBuf>) -> Self {
        let xvn_home = xvn_home.into();
        let blobs = BlobStore::new(xvn_home.join("checkpoints").join("blobs"));
        Self {
            pool,
            blobs,
            xvn_home,
        }
    }

    fn strategies_dir(&self) -> PathBuf {
        strategy_store_dir(&self.xvn_home)
    }

    /// Take a content-addressed snapshot of the requested artifacts and persist
    /// a checkpoint row. Returns the loaded [`Checkpoint`] (id + content hash +
    /// manifest). Capturing the same artifacts at the same content yields the
    /// same per-artifact blob hashes, so blobs dedupe; a fresh checkpoint row
    /// (unique id) is always written.
    pub async fn snapshot(
        &self,
        session_id: &str,
        kind: CheckpointKind,
        request: SnapshotRequest,
    ) -> Result<Checkpoint, CheckpointError> {
        let mut artifacts = Vec::new();

        if let Some(strategy_id) = &request.strategy_id {
            let store = FilesystemStore::new(self.strategies_dir());
            // Resolve the path up front so an invalid id (path traversal, NUL,
            // etc.) surfaces as the existing `checkpoint_restore_failed`
            // signature — preserving the prior behaviour where bad ids
            // failed inside `store.load`.
            let path = store
                .path_for(strategy_id)
                .map_err(|e| CheckpointError::Restore {
                    checkpoint_id: String::new(),
                    artifact: "strategy",
                    source: anyhow::anyhow!(e),
                })?;
            let exists = tokio::fs::try_exists(&path)
                .await
                .map_err(|e| CheckpointError::Restore {
                    checkpoint_id: String::new(),
                    artifact: "strategy",
                    source: anyhow::Error::from(e),
                })?;
            if !exists {
                // CREATE-like pre-state: the strategy file does not exist yet.
                // Capture an empty blob with `was_absent: true` so restore can
                // rewind back to the absent state by deleting the file.
                let blob = self.blobs.write(&[])?;
                artifacts.push(CapturedArtifact::Strategy {
                    id: strategy_id.clone(),
                    blob_hash: blob.as_str().to_string(),
                    was_absent: true,
                });
            } else {
                let strategy = store
                    .load(strategy_id)
                    .await
                    .map_err(|e| CheckpointError::Restore {
                        checkpoint_id: String::new(),
                        artifact: "strategy",
                        source: e,
                    })?;
                // Capture the EXACT bytes the store persists (to_vec_pretty), so a
                // byte-compare of the restored file against the original is identical.
                let bytes =
                    serde_json::to_vec_pretty(&strategy).map_err(|e| CheckpointError::Other(e.into()))?;
                let blob = self.blobs.write(&bytes)?;
                artifacts.push(CapturedArtifact::Strategy {
                    id: strategy_id.clone(),
                    blob_hash: blob.as_str().to_string(),
                    was_absent: false,
                });
            }
        }

        if let Some(agent_id) = &request.agent_id {
            let agent_store = AgentStore::new(self.pool.clone());
            let agent = agent_store
                .get(agent_id)
                .await
                .map_err(|e| CheckpointError::Other(e))?
                .ok_or_else(|| CheckpointError::Other(anyhow::anyhow!("agent not found: {agent_id}")))?;
            let bytes = serde_json::to_vec(&agent.slots).map_err(|e| CheckpointError::Other(e.into()))?;
            let blob = self.blobs.write(&bytes)?;
            artifacts.push(CapturedArtifact::AgentSlots {
                agent_id: agent_id.clone(),
                blob_hash: blob.as_str().to_string(),
            });
        }

        if request.tool_policy {
            let rail = ChatSessionStore::load_rail_state(&self.pool, session_id)
                .await
                .map_err(CheckpointError::Other)?;
            let (was_null, payload) = match rail.tool_policy_json {
                Some(json) => (false, json.into_bytes()),
                None => (true, Vec::new()),
            };
            let blob = self.blobs.write(&payload)?;
            artifacts.push(CapturedArtifact::ToolPolicy {
                was_null,
                blob_hash: blob.as_str().to_string(),
            });
        }

        if request.focus {
            let rail = ChatSessionStore::load_rail_state(&self.pool, session_id)
                .await
                .map_err(CheckpointError::Other)?;
            match rail.focus_path {
                Some(path) => {
                    let abs = self.xvn_home.join(&path);
                    let bytes = tokio::fs::read(&abs).await.unwrap_or_default();
                    let blob = self.blobs.write(&bytes)?;
                    artifacts.push(CapturedArtifact::Focus {
                        was_set: true,
                        path,
                        blob_hash: blob.as_str().to_string(),
                    });
                }
                None => {
                    // No focus set — capture the empty marker so restore clears
                    // any focus set after the snapshot.
                    let blob = self.blobs.write(&[])?;
                    artifacts.push(CapturedArtifact::Focus {
                        was_set: false,
                        path: String::new(),
                        blob_hash: blob.as_str().to_string(),
                    });
                }
            }
        }

        let manifest = CapturedManifest { artifacts };
        // Canonical manifest JSON → content hash. Same artifacts at the same
        // hashes produce the same content_hash, so a no-op checkpoint is
        // recognizable.
        let captured_json = serde_json::to_string(&manifest).map_err(|e| CheckpointError::Other(e.into()))?;
        let content_hash = hex::encode(Sha256::digest(captured_json.as_bytes()));

        let checkpoint_id = Ulid::new().to_string();
        let created_at = Utc::now();
        let kind_db = kind.as_db_str();

        sqlx::query(
            "INSERT INTO chat_checkpoints \
             (checkpoint_id, session_id, created_at, kind, content_hash, captured_json, label) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&checkpoint_id)
        .bind(session_id)
        .bind(created_at.to_rfc3339())
        .bind(&kind_db)
        .bind(&content_hash)
        .bind(&captured_json)
        .bind(request.label.as_deref())
        .execute(&self.pool)
        .await?;

        Ok(Checkpoint {
            checkpoint_id,
            session_id: session_id.to_string(),
            created_at,
            kind,
            content_hash,
            label: request.label,
            artifacts: manifest.artifacts,
        })
    }

    /// List checkpoints for a session, newest first.
    pub async fn list(&self, session_id: &str) -> Result<Vec<Checkpoint>, CheckpointError> {
        let rows = sqlx::query(
            "SELECT checkpoint_id, session_id, created_at, kind, content_hash, captured_json, label \
             FROM chat_checkpoints WHERE session_id = ?1 ORDER BY created_at DESC, checkpoint_id DESC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_checkpoint).collect()
    }

    /// Load a single checkpoint by id.
    pub async fn get(&self, checkpoint_id: &str) -> Result<Checkpoint, CheckpointError> {
        let row = sqlx::query(
            "SELECT checkpoint_id, session_id, created_at, kind, content_hash, captured_json, label \
             FROM chat_checkpoints WHERE checkpoint_id = ?1",
        )
        .bind(checkpoint_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| CheckpointError::NotFound(checkpoint_id.to_string()))?;

        row_to_checkpoint(row)
    }

    /// Restore every artifact captured by `checkpoint_id` to its snapshot value,
    /// verbatim. Returns the artifact-kind labels rewound.
    ///
    /// Restore is non-destructive on failure: it first VALIDATES that every
    /// referenced blob exists (returning [`CheckpointError::MissingBlob`]
    /// without touching anything), and per-artifact writes that can reject
    /// (agent slots) validate before mutating. Restoring an unknown checkpoint
    /// id returns [`CheckpointError::NotFound`].
    pub async fn restore(&self, checkpoint_id: &str) -> Result<RestoreOutcome, CheckpointError> {
        let checkpoint = self.get(checkpoint_id).await?;

        // ── Validation phase: every referenced blob must exist BEFORE we write
        // anything. A missing blob aborts with a typed error and zero mutation.
        for artifact in &checkpoint.artifacts {
            let blob = BlobRef(artifact.blob_hash().to_string());
            if !self.blobs.exists(&blob) {
                return Err(CheckpointError::MissingBlob {
                    checkpoint_id: checkpoint_id.to_string(),
                    artifact: artifact.label(),
                    blob_hash: artifact.blob_hash().to_string(),
                });
            }
        }

        // ── Write phase: rewind each artifact verbatim.
        let mut restored = Vec::new();
        for artifact in &checkpoint.artifacts {
            match artifact {
                CapturedArtifact::Strategy {
                    id,
                    blob_hash,
                    was_absent,
                } => {
                    if *was_absent {
                        // Pre-state was "no file". Rewind by deleting the
                        // file the CREATE op wrote. Idempotent: a NotFound
                        // here means we're already in the target state.
                        let store = FilesystemStore::new(self.strategies_dir());
                        let path = store.path_for(id).map_err(|e| CheckpointError::Restore {
                            checkpoint_id: checkpoint_id.to_string(),
                            artifact: "strategy",
                            source: anyhow::anyhow!(e),
                        })?;
                        match tokio::fs::remove_file(&path).await {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => {
                                return Err(CheckpointError::Restore {
                                    checkpoint_id: checkpoint_id.to_string(),
                                    artifact: "strategy",
                                    source: anyhow::Error::from(e),
                                });
                            }
                        }
                    } else {
                        let bytes = self.blobs.read(&BlobRef(blob_hash.clone()))?;
                        let strategy: Strategy =
                            serde_json::from_slice(&bytes).map_err(|e| CheckpointError::Restore {
                                checkpoint_id: checkpoint_id.to_string(),
                                artifact: "strategy",
                                source: e.into(),
                            })?;
                        let store = FilesystemStore::new(self.strategies_dir());
                        store
                            .save(&strategy)
                            .await
                            .map_err(|e| CheckpointError::Restore {
                                checkpoint_id: checkpoint_id.to_string(),
                                artifact: "strategy",
                                source: e,
                            })?;
                        let _ = id;
                    }
                    restored.push("strategy".to_string());
                }
                CapturedArtifact::AgentSlots { agent_id, blob_hash } => {
                    let bytes = self.blobs.read(&BlobRef(blob_hash.clone()))?;
                    let slots: Vec<AgentSlot> =
                        serde_json::from_slice(&bytes).map_err(|e| CheckpointError::Restore {
                            checkpoint_id: checkpoint_id.to_string(),
                            artifact: "agent_slots",
                            source: e.into(),
                        })?;
                    let agent_store = AgentStore::new(self.pool.clone());
                    // `update` validates slots before deleting old rows — a
                    // rejected restore leaves the live agent intact.
                    agent_store
                        .update(
                            agent_id,
                            UpdateAgent {
                                slots: Some(slots),
                                ..Default::default()
                            },
                        )
                        .await
                        .map_err(|e| CheckpointError::Restore {
                            checkpoint_id: checkpoint_id.to_string(),
                            artifact: "agent_slots",
                            source: e,
                        })?;
                    restored.push("agent_slots".to_string());
                }
                CapturedArtifact::ToolPolicy { was_null, blob_hash } => {
                    let json = if *was_null {
                        None
                    } else {
                        let bytes = self.blobs.read(&BlobRef(blob_hash.clone()))?;
                        Some(String::from_utf8(bytes).map_err(|e| CheckpointError::Restore {
                            checkpoint_id: checkpoint_id.to_string(),
                            artifact: "tool_policy",
                            source: e.into(),
                        })?)
                    };
                    ChatSessionStore::set_tool_policy(&self.pool, &checkpoint.session_id, json.as_deref())
                        .await
                        .map_err(|e| CheckpointError::Restore {
                            checkpoint_id: checkpoint_id.to_string(),
                            artifact: "tool_policy",
                            source: e,
                        })?;
                    restored.push("tool_policy".to_string());
                }
                CapturedArtifact::Focus {
                    was_set,
                    path,
                    blob_hash,
                } => {
                    if *was_set {
                        let bytes = self.blobs.read(&BlobRef(blob_hash.clone()))?;
                        let abs = self.xvn_home.join(path);
                        if let Some(parent) = abs.parent() {
                            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                                CheckpointError::Restore {
                                    checkpoint_id: checkpoint_id.to_string(),
                                    artifact: "focus",
                                    source: e.into(),
                                }
                            })?;
                        }
                        tokio::fs::write(&abs, &bytes)
                            .await
                            .map_err(|e| CheckpointError::Restore {
                                checkpoint_id: checkpoint_id.to_string(),
                                artifact: "focus",
                                source: e.into(),
                            })?;
                        ChatSessionStore::set_focus_path(&self.pool, &checkpoint.session_id, Some(path))
                            .await
                            .map_err(|e| CheckpointError::Restore {
                                checkpoint_id: checkpoint_id.to_string(),
                                artifact: "focus",
                                source: e,
                            })?;
                    } else {
                        // Snapshot had no focus — clear whatever was set later.
                        ChatSessionStore::set_focus_path(&self.pool, &checkpoint.session_id, None)
                            .await
                            .map_err(|e| CheckpointError::Restore {
                                checkpoint_id: checkpoint_id.to_string(),
                                artifact: "focus",
                                source: e,
                            })?;
                    }
                    restored.push("focus".to_string());
                }
            }
        }

        Ok(RestoreOutcome {
            checkpoint_id: checkpoint_id.to_string(),
            session_id: checkpoint.session_id,
            restored,
        })
    }

    /// The blob store this checkpointer writes to (exposed for tests + the
    /// janitor).
    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    /// Resolve a focus file's absolute path under `xvn_home` (helper for callers
    /// preparing a [`SnapshotRequest`]).
    pub fn focus_abs_path(&self, rel: &Path) -> PathBuf {
        self.xvn_home.join(rel)
    }
}

fn row_to_checkpoint(row: sqlx::sqlite::SqliteRow) -> Result<Checkpoint, CheckpointError> {
    let checkpoint_id: String = row.try_get("checkpoint_id")?;
    let session_id: String = row.try_get("session_id")?;
    let created_at_s: String = row.try_get("created_at")?;
    let kind_s: String = row.try_get("kind")?;
    let content_hash: String = row.try_get("content_hash")?;
    let captured_json: String = row.try_get("captured_json")?;
    let label: Option<String> = row.try_get("label")?;

    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map_err(|e| CheckpointError::Other(anyhow::anyhow!("bad created_at: {e}")))?
        .with_timezone(&Utc);
    let manifest: CapturedManifest = serde_json::from_str(&captured_json)
        .map_err(|e| CheckpointError::Other(anyhow::anyhow!("bad captured_json: {e}")))?;

    Ok(Checkpoint {
        checkpoint_id,
        session_id,
        created_at,
        kind: CheckpointKind::from_db_str(&kind_s),
        content_hash,
        label,
        artifacts: manifest.artifacts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::model::InputsPolicy;
    use crate::agents::store::NewAgent;
    use crate::agents::AgentSlot;
    use crate::api::{Actor as ApiActor, ApiContext};
    use crate::chat_session::{ChatSessionStore, ContextScope};
    use crate::strategies::manifest::PublicManifest;
    use crate::strategies::risk::RiskPreset;
    use tempfile::TempDir;

    // ── Test scaffolding ────────────────────────────────────────────────────

    /// Open a real `ApiContext` rooted at `xvn_home` and hand back its pool.
    /// Using the genuine `ApiContext::open` runs the full hand-maintained
    /// migration registry — including the `migrate_checkpoints` wiring this
    /// module depends on — so the test exercises the production migration path,
    /// not a hand-rolled subset. The same `xvn_home` is reused for the
    /// `Checkpointer` so the strategy filesystem + blob store + DB agree.
    async fn open_ctx(xvn_home: &std::path::Path) -> SqlitePool {
        let ctx = ApiContext::open(xvn_home, ApiActor::Cli { user: "test".into() })
            .await
            .expect("open ApiContext");
        ctx.db.clone()
    }

    fn sample_strategy(id: &str) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: id.to_string(),
                display_name: "Test Strategy".into(),
                plain_summary: "t".into(),
                creator: "@tester".into(),
                template: "trend_follower".into(),
                regime_fit: vec![],
                asset_universe: vec![],
                decision_cadence_minutes: 60,
                attested_with: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
            },
            hypothesis: None,
            agents: vec![],
            pipeline: Default::default(),
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        }
    }

    /// Build a slot with a long, non-placeholder prompt so the AgentStore
    /// save-gate (which requires a substantive prompt) accepts it. `delta_briefing`
    /// is `None` to match the DB read path (the column is not persisted) so a
    /// post-restore `get` compares equal to the captured slots.
    fn slot(name: &str, prompt_lead: &str) -> AgentSlot {
        let system_prompt = format!(
            "{prompt_lead} Analyse the OHLCV data provided and respond with a JSON object \
             containing: action (buy/sell/hold), size_pct (0-100), and reason (string). Apply \
             disciplined risk management: never risk more than 1% of notional equity per trade, \
             and always respect the configured stop-loss and take-profit levels. Avoid \
             over-trading on low-volume bars."
        );
        AgentSlot {
            name: name.to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            system_prompt,
            skill_ids: vec![],
            max_tokens: Some(4096),
            max_wall_ms: None,
            temperature: None,
            prompt_version: String::new(),
            inputs_policy: InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::default(),
            noop_skip: None,
            allowed_tools: Vec::new(),
            delta_briefing: None,
        }
    }

    async fn make_session(pool: &SqlitePool) -> String {
        ChatSessionStore::create_session(pool, &ContextScope::Workspace)
            .await
            .unwrap()
    }

    // ── Tests ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn snapshot_then_restore_strategy_is_byte_identical() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        // Persist the original strategy via the SAME filesystem store the
        // checkpointer uses, so the on-disk bytes are the canonical form.
        let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
        let strategy_id = "01HZSTRATEGY00000000000000";
        let original = sample_strategy(strategy_id);
        store.save(&original).await.unwrap();
        let strategy_path = store.path_for(strategy_id).unwrap();
        let original_bytes = tokio::fs::read(&strategy_path).await.unwrap();

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::PreTool,
                SnapshotRequest {
                    strategy_id: Some(strategy_id.to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].label(), "strategy");

        // MUTATE the strategy on disk.
        let mut mutated = original.clone();
        mutated.manifest.display_name = "MUTATED".to_string();
        store.save(&mutated).await.unwrap();
        let mutated_bytes = tokio::fs::read(&strategy_path).await.unwrap();
        assert_ne!(mutated_bytes, original_bytes, "mutation must change the file");

        // RESTORE.
        let outcome = ckpt.restore(&snapshot.checkpoint_id).await.unwrap();
        assert_eq!(outcome.restored, vec!["strategy".to_string()]);

        // BYTE-COMPARE the restored file against the original.
        let restored_bytes = tokio::fs::read(&strategy_path).await.unwrap();
        assert_eq!(
            restored_bytes, original_bytes,
            "restored strategy bytes must be identical to the original"
        );
    }

    #[tokio::test]
    async fn restore_of_unknown_checkpoint_is_typed_and_non_destructive() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;

        // Write a strategy and DO NOT checkpoint it.
        let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
        let strategy_id = "01HZSTRATEGY00000000000001";
        store.save(&sample_strategy(strategy_id)).await.unwrap();
        let path = store.path_for(strategy_id).unwrap();
        let before = tokio::fs::read(&path).await.unwrap();

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let err = ckpt.restore("ckpt_does_not_exist").await.unwrap_err();
        match &err {
            CheckpointError::NotFound(id) => assert_eq!(id, "ckpt_does_not_exist"),
            other => panic!("expected NotFound, got {other:?}"),
        }
        assert_eq!(err.code(), "checkpoint_not_found");

        // Non-destructive: the on-disk strategy is untouched.
        let after = tokio::fs::read(&path).await.unwrap();
        assert_eq!(before, after, "failed restore must not mutate anything");
    }

    #[tokio::test]
    async fn restore_missing_blob_is_typed_and_non_destructive() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
        let strategy_id = "01HZSTRATEGY00000000000002";
        let original = sample_strategy(strategy_id);
        store.save(&original).await.unwrap();
        let path = store.path_for(strategy_id).unwrap();
        let original_bytes = tokio::fs::read(&path).await.unwrap();

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::Manual,
                SnapshotRequest {
                    strategy_id: Some(strategy_id.to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        // Delete the captured blob out from under the checkpoint.
        let blob_hash = snapshot.artifacts[0].blob_hash().to_string();
        ckpt.blobs().delete(&BlobRef(blob_hash.clone())).unwrap();

        // Mutate the strategy so we can prove restore didn't run.
        let mut mutated = original.clone();
        mutated.manifest.display_name = "MUTATED".to_string();
        store.save(&mutated).await.unwrap();
        let mutated_bytes = tokio::fs::read(&path).await.unwrap();

        let err = ckpt.restore(&snapshot.checkpoint_id).await.unwrap_err();
        match &err {
            CheckpointError::MissingBlob {
                artifact,
                blob_hash: bh,
                ..
            } => {
                assert_eq!(*artifact, "strategy");
                assert_eq!(bh, &blob_hash);
            }
            other => panic!("expected MissingBlob, got {other:?}"),
        }
        assert_eq!(err.code(), "checkpoint_missing_blob");

        // Non-destructive: the (mutated) file is still the mutated bytes, NOT
        // the original — the validation phase aborted before any write.
        let after = tokio::fs::read(&path).await.unwrap();
        assert_eq!(after, mutated_bytes);
        assert_ne!(after, original_bytes);
    }

    #[tokio::test]
    async fn snapshot_then_restore_agent_slots_roundtrips() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        let agent_store = AgentStore::new(pool.clone());
        let agent_id = agent_store
            .create(NewAgent {
                name: "Default Agent".to_string(),
                description: "for checkpoint test".to_string(),
                tags: vec![],
                slots: vec![slot("trader", "You are a careful trader. Decide buy or sell.")],
                scope_strategy_id: None,
            })
            .await
            .unwrap();

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::PreTool,
                SnapshotRequest {
                    agent_id: Some(agent_id.clone()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let original = agent_store.get(&agent_id).await.unwrap().unwrap();

        // MUTATE the slots.
        agent_store
            .update(
                &agent_id,
                UpdateAgent {
                    slots: Some(vec![
                        slot("trader", "MUTATED PROMPT for the trader slot decision."),
                        slot("reviewer", "MUTATED second slot to review the trade."),
                    ]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let mutated = agent_store.get(&agent_id).await.unwrap().unwrap();
        assert_ne!(mutated.slots, original.slots);

        // RESTORE.
        let outcome = ckpt.restore(&snapshot.checkpoint_id).await.unwrap();
        assert_eq!(outcome.restored, vec!["agent_slots".to_string()]);

        let restored = agent_store.get(&agent_id).await.unwrap().unwrap();
        assert_eq!(
            restored.slots, original.slots,
            "restored agent slots must match the snapshot"
        );
    }

    #[tokio::test]
    async fn snapshot_then_restore_tool_policy_and_list() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        // Original policy.
        ChatSessionStore::set_tool_policy(
            &pool,
            &session_id,
            Some(r#"{"create_strategy":"needs_approval"}"#),
        )
        .await
        .unwrap();

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::Manual,
                SnapshotRequest {
                    tool_policy: true,
                    label: Some("before edit".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        // list() surfaces the new checkpoint.
        let listed = ckpt.list(&session_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].checkpoint_id, snapshot.checkpoint_id);
        assert_eq!(listed[0].label.as_deref(), Some("before edit"));

        // MUTATE the policy.
        ChatSessionStore::set_tool_policy(&pool, &session_id, Some(r#"{"create_strategy":"denied"}"#))
            .await
            .unwrap();

        // RESTORE.
        let outcome = ckpt.restore(&snapshot.checkpoint_id).await.unwrap();
        assert_eq!(outcome.restored, vec!["tool_policy".to_string()]);

        let rail = ChatSessionStore::load_rail_state(&pool, &session_id)
            .await
            .unwrap();
        assert_eq!(
            rail.tool_policy_json.as_deref(),
            Some(r#"{"create_strategy":"needs_approval"}"#)
        );
    }

    #[tokio::test]
    async fn snapshot_of_absent_strategy_records_was_absent() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        // No strategy file on disk — this is the CREATE pre-state.
        let strategy_id = "01HZSTRATEGYABSENT00000001";
        let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
        assert!(!tokio::fs::try_exists(&store.path_for(strategy_id).unwrap())
            .await
            .unwrap());

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::PreTool,
                SnapshotRequest {
                    strategy_id: Some(strategy_id.to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("snapshotting an absent strategy must succeed (empty pre-state)");
        assert_eq!(snapshot.artifacts.len(), 1);
        match &snapshot.artifacts[0] {
            CapturedArtifact::Strategy { id, was_absent, .. } => {
                assert_eq!(id, strategy_id);
                assert!(*was_absent, "absent file must capture was_absent: true");
            }
            other => panic!("expected Strategy artifact, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn restore_of_create_pre_state_deletes_the_created_file() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        // Pre-state: file does not exist.
        let strategy_id = "01HZSTRATEGYCREATE00000002";
        let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
        let path = store.path_for(strategy_id).unwrap();
        assert!(!tokio::fs::try_exists(&path).await.unwrap());

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::PreTool,
                SnapshotRequest {
                    strategy_id: Some(strategy_id.to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        // Simulate the CREATE: write the strategy after the snapshot.
        store.save(&sample_strategy(strategy_id)).await.unwrap();
        assert!(tokio::fs::try_exists(&path).await.unwrap());

        // RESTORE rewinds to the absent state by deleting the file.
        let outcome = ckpt.restore(&snapshot.checkpoint_id).await.unwrap();
        assert_eq!(outcome.restored, vec!["strategy".to_string()]);
        assert!(
            !tokio::fs::try_exists(&path).await.unwrap(),
            "restore of a was_absent snapshot must delete the created file"
        );
    }

    #[tokio::test]
    async fn restore_of_create_pre_state_is_idempotent_when_file_already_gone() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        let strategy_id = "01HZSTRATEGYCREATE00000003";
        let store = FilesystemStore::new(strategy_store_dir(tmp.path()));
        let path = store.path_for(strategy_id).unwrap();

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        let snapshot = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::PreTool,
                SnapshotRequest {
                    strategy_id: Some(strategy_id.to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        // CREATE op never produced a file (e.g. it failed). Restore must still
        // succeed and leave us in the absent state.
        assert!(!tokio::fs::try_exists(&path).await.unwrap());
        let outcome = ckpt.restore(&snapshot.checkpoint_id).await.unwrap();
        assert_eq!(outcome.restored, vec!["strategy".to_string()]);
        assert!(!tokio::fs::try_exists(&path).await.unwrap());
    }

    /// Backward-compat: rows written before this fix have no `was_absent`
    /// field in their captured_json. `#[serde(default)]` on the variant must
    /// keep them deserializing as `was_absent: false` so legacy snapshots
    /// continue to round-trip through the write-back path, never the
    /// delete-on-restore path.
    #[test]
    fn legacy_manifest_without_was_absent_field_deserializes_as_false() {
        let legacy_json = r#"{
            "artifacts": [
                {
                    "kind": "strategy",
                    "id": "01HZLEGACY0000000000000000",
                    "blob_hash": "deadbeefcafebabe"
                }
            ]
        }"#;
        let manifest: CapturedManifest =
            serde_json::from_str(legacy_json).expect("legacy manifest must deserialize");
        assert_eq!(manifest.artifacts.len(), 1);
        match &manifest.artifacts[0] {
            CapturedArtifact::Strategy {
                id,
                blob_hash,
                was_absent,
            } => {
                assert_eq!(id, "01HZLEGACY0000000000000000");
                assert_eq!(blob_hash, "deadbeefcafebabe");
                assert!(
                    !*was_absent,
                    "legacy snapshot (no field) must default to was_absent: false \
                     so restore still writes the blob back instead of deleting"
                );
            }
            other => panic!("expected Strategy variant, got {other:?}"),
        }
    }

    /// The fix moved strategy-id validation from `store.load` (in the
    /// file-exists branch) to an unconditional `store.path_for` call up
    /// front, so the absent branch can use it too. An invalid id must still
    /// surface as `CheckpointError::Restore` (code `checkpoint_restore_failed`)
    /// — same machine code the rail already handles, not a silent success or
    /// a new error class.
    #[tokio::test]
    async fn snapshot_with_invalid_strategy_id_surfaces_checkpoint_restore_failed() {
        let tmp = TempDir::new().unwrap();
        let pool = open_ctx(tmp.path()).await;
        let session_id = make_session(&pool).await;

        let ckpt = Checkpointer::new(pool.clone(), tmp.path());
        // Path traversal — rejected by validate_strategy_id_for_path.
        let err = ckpt
            .snapshot(
                &session_id,
                CheckpointKind::PreTool,
                SnapshotRequest {
                    strategy_id: Some("../etc/passwd".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        match &err {
            CheckpointError::Restore { artifact, .. } => assert_eq!(*artifact, "strategy"),
            other => panic!("expected Restore, got {other:?}"),
        }
        assert_eq!(err.code(), "checkpoint_restore_failed");
    }
}
