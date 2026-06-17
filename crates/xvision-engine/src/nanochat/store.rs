// crates/xvision-engine/src/nanochat/store.rs

use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;

// ── Row types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TrainedModel {
    pub model_id: String,
    pub display_name: String,
    pub source_strategy_id: Option<String>,
    pub source_strategy_name: Option<String>,
    pub run_tag: String,
    pub checkpoint_path: String,
    pub weights_format: String,
    pub weights_sha256: String,
    pub input_spec: String,       // JSON string
    pub base_model: String,
    pub label_strategy: String,
    pub label_config: String,     // JSON string
    pub best_acc: Option<f64>,
    pub best_loss: Option<f64>,
    pub holdout_samples: Option<i64>,
    pub promoted: bool,
    pub live_approved: bool,
    pub created_at: String,
    pub autoresearch_run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTrainedModel {
    pub display_name: String,
    pub source_strategy_id: Option<String>,
    pub source_strategy_name: Option<String>,
    pub run_tag: String,
    pub checkpoint_path: String,
    pub weights_sha256: String,
    pub input_spec: String,
    pub label_strategy: String,
    pub label_config: String,
    pub best_acc: Option<f64>,
    pub best_loss: Option<f64>,
    pub holdout_samples: Option<i64>,
    pub autoresearch_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoresearchRun {
    pub run_id: String,
    pub run_tag: String,
    pub source_strategy_id: Option<String>,
    pub label_strategy: String,
    pub label_config: String,
    pub git_branch: String,
    pub worktree_path: String,
    pub status: String,
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub experiments: i64,
    pub best_acc: Option<f64>,
    pub best_model_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewAutoresearchRun {
    pub run_tag: String,
    pub source_strategy_id: Option<String>,
    pub label_strategy: String,
    pub label_config: String,
    pub git_branch: String,
    pub worktree_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoresearchExperiment {
    pub experiment_id: String,
    pub run_id: String,
    pub git_commit: String,
    pub val_acc: Option<f64>,
    pub val_loss: Option<f64>,
    pub peak_vram_mb: Option<f64>,
    pub training_seconds: Option<f64>,
    pub status: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewExperiment {
    pub run_id: String,
    pub git_commit: String,
    pub val_acc: Option<f64>,
    pub val_loss: Option<f64>,
    pub peak_vram_mb: Option<f64>,
    pub training_seconds: Option<f64>,
    pub status: String,
    pub description: String,
}

// ── Private row-mapping helpers ───────────────────────────────────────────────

fn row_to_trained_model(row: sqlx::sqlite::SqliteRow) -> TrainedModel {
    use sqlx::Row;
    TrainedModel {
        model_id: row.get("model_id"),
        display_name: row.get("display_name"),
        source_strategy_id: row.get("source_strategy_id"),
        source_strategy_name: row.get("source_strategy_name"),
        run_tag: row.get("run_tag"),
        checkpoint_path: row.get("checkpoint_path"),
        weights_format: row.get("weights_format"),
        weights_sha256: row.get("weights_sha256"),
        input_spec: row.get("input_spec"),
        base_model: row.get("base_model"),
        label_strategy: row.get("label_strategy"),
        label_config: row.get("label_config"),
        best_acc: row.get("best_acc"),
        best_loss: row.get("best_loss"),
        holdout_samples: row.get("holdout_samples"),
        promoted: row.get::<i64, _>("promoted") != 0,
        live_approved: row.get::<i64, _>("live_approved") != 0,
        created_at: row.get("created_at"),
        autoresearch_run_id: row.get("autoresearch_run_id"),
    }
}

fn row_to_run(row: sqlx::sqlite::SqliteRow) -> AutoresearchRun {
    use sqlx::Row;
    AutoresearchRun {
        run_id: row.get("run_id"),
        run_tag: row.get("run_tag"),
        source_strategy_id: row.get("source_strategy_id"),
        label_strategy: row.get("label_strategy"),
        label_config: row.get("label_config"),
        git_branch: row.get("git_branch"),
        worktree_path: row.get("worktree_path"),
        status: row.get("status"),
        started_at: row.get("started_at"),
        stopped_at: row.get("stopped_at"),
        experiments: row.get("experiments"),
        best_acc: row.get("best_acc"),
        best_model_id: row.get("best_model_id"),
    }
}

fn row_to_experiment(row: sqlx::sqlite::SqliteRow) -> AutoresearchExperiment {
    use sqlx::Row;
    AutoresearchExperiment {
        experiment_id: row.get("experiment_id"),
        run_id: row.get("run_id"),
        git_commit: row.get("git_commit"),
        val_acc: row.get("val_acc"),
        val_loss: row.get("val_loss"),
        peak_vram_mb: row.get("peak_vram_mb"),
        training_seconds: row.get("training_seconds"),
        status: row.get("status"),
        description: row.get("description"),
        created_at: row.get("created_at"),
    }
}

// ── Store ────────────────────────────────────────────────────────────────────

pub struct NanochatStore {
    pool: SqlitePool,
}

impl NanochatStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // trained_models ──────────────────────────────────────────────────────────

    pub async fn insert_model(&self, m: NewTrainedModel) -> anyhow::Result<String> {
        let model_id = Ulid::new().to_string();
        let created_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO trained_models \
             (model_id, display_name, source_strategy_id, source_strategy_name, run_tag, \
              checkpoint_path, weights_format, weights_sha256, input_spec, base_model, \
              label_strategy, label_config, best_acc, best_loss, holdout_samples, \
              promoted, live_approved, created_at, autoresearch_run_id) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,0,?,?)",
        )
        .bind(&model_id)
        .bind(&m.display_name)
        .bind(&m.source_strategy_id)
        .bind(&m.source_strategy_name)
        .bind(&m.run_tag)
        .bind(&m.checkpoint_path)
        .bind("safetensors")
        .bind(&m.weights_sha256)
        .bind(&m.input_spec)
        .bind("gpt2-nanochat")
        .bind(&m.label_strategy)
        .bind(&m.label_config)
        .bind(m.best_acc)
        .bind(m.best_loss)
        .bind(m.holdout_samples)
        .bind(&created_at)
        .bind(&m.autoresearch_run_id)
        .execute(&self.pool)
        .await?;
        Ok(model_id)
    }

    pub async fn get_model(&self, model_id: &str) -> anyhow::Result<Option<TrainedModel>> {
        let row = sqlx::query(
            "SELECT model_id, display_name, source_strategy_id, source_strategy_name, run_tag, \
             checkpoint_path, weights_format, weights_sha256, input_spec, base_model, \
             label_strategy, label_config, best_acc, best_loss, holdout_samples, \
             promoted, live_approved, created_at, autoresearch_run_id \
             FROM trained_models WHERE model_id = ?",
        )
        .bind(model_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_trained_model))
    }

    pub async fn list_promoted_models(&self) -> anyhow::Result<Vec<TrainedModel>> {
        let rows = sqlx::query(
            "SELECT model_id, display_name, source_strategy_id, source_strategy_name, run_tag, \
             checkpoint_path, weights_format, weights_sha256, input_spec, base_model, \
             label_strategy, label_config, best_acc, best_loss, holdout_samples, \
             promoted, live_approved, created_at, autoresearch_run_id \
             FROM trained_models WHERE promoted = 1 ORDER BY best_acc DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_trained_model).collect())
    }

    /// Sets `promoted = 1` for the given model. Idempotent.
    pub async fn update_promotion(&self, model_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE trained_models SET promoted = 1 WHERE model_id = ?")
            .bind(model_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Sets `live_approved = 1` for the given model. Idempotent.
    pub async fn set_live_approved(&self, model_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE trained_models SET live_approved = 1 WHERE model_id = ?")
            .bind(model_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // autoresearch_runs ───────────────────────────────────────────────────────

    pub async fn insert_run(&self, r: NewAutoresearchRun) -> anyhow::Result<String> {
        let run_id = Ulid::new().to_string();
        let started_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO autoresearch_runs \
             (run_id, run_tag, source_strategy_id, label_strategy, label_config, \
              git_branch, worktree_path, status, started_at, experiments) \
             VALUES (?,?,?,?,?,?,?,'running',?,0)",
        )
        .bind(&run_id)
        .bind(&r.run_tag)
        .bind(&r.source_strategy_id)
        .bind(&r.label_strategy)
        .bind(&r.label_config)
        .bind(&r.git_branch)
        .bind(&r.worktree_path)
        .bind(&started_at)
        .execute(&self.pool)
        .await?;
        Ok(run_id)
    }

    pub async fn get_run(&self, run_id: &str) -> anyhow::Result<Option<AutoresearchRun>> {
        let row = sqlx::query(
            "SELECT run_id, run_tag, source_strategy_id, label_strategy, label_config, \
             git_branch, worktree_path, status, started_at, stopped_at, \
             experiments, best_acc, best_model_id \
             FROM autoresearch_runs WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_run))
    }

    pub async fn list_runs(&self) -> anyhow::Result<Vec<AutoresearchRun>> {
        let rows = sqlx::query(
            "SELECT run_id, run_tag, source_strategy_id, label_strategy, label_config, \
             git_branch, worktree_path, status, started_at, stopped_at, \
             experiments, best_acc, best_model_id \
             FROM autoresearch_runs ORDER BY started_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_run).collect())
    }

    pub async fn update_run_status(&self, run_id: &str, status: &str, stopped_at: Option<&str>) -> anyhow::Result<()> {
        sqlx::query("UPDATE autoresearch_runs SET status = ?, stopped_at = ? WHERE run_id = ?")
            .bind(status)
            .bind(stopped_at)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Increments `experiments` counter and updates `best_acc` + `best_model_id`
    /// if the new acc exceeds the current best (NULL treated as -infinity).
    pub async fn bump_experiment_count(
        &self,
        run_id: &str,
        new_acc: Option<f64>,
        new_model_id: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE autoresearch_runs SET \
             experiments = experiments + 1, \
             best_acc = CASE \
                 WHEN ? IS NOT NULL AND (best_acc IS NULL OR ? > best_acc) THEN ? \
                 ELSE best_acc \
             END, \
             best_model_id = CASE \
                 WHEN ? IS NOT NULL AND (best_acc IS NULL OR ? > best_acc) THEN ? \
                 ELSE best_model_id \
             END \
             WHERE run_id = ?",
        )
        .bind(new_acc)
        .bind(new_acc)
        .bind(new_acc)
        .bind(new_acc)
        .bind(new_acc)
        .bind(new_model_id)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // autoresearch_experiments ────────────────────────────────────────────────

    pub async fn insert_experiment(&self, e: NewExperiment) -> anyhow::Result<String> {
        let experiment_id = Ulid::new().to_string();
        let created_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO autoresearch_experiments \
             (experiment_id, run_id, git_commit, val_acc, val_loss, peak_vram_mb, \
              training_seconds, status, description, created_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&experiment_id)
        .bind(&e.run_id)
        .bind(&e.git_commit)
        .bind(e.val_acc)
        .bind(e.val_loss)
        .bind(e.peak_vram_mb)
        .bind(e.training_seconds)
        .bind(&e.status)
        .bind(&e.description)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;
        Ok(experiment_id)
    }

    pub async fn list_experiments(&self, run_id: &str) -> anyhow::Result<Vec<AutoresearchExperiment>> {
        let rows = sqlx::query(
            "SELECT experiment_id, run_id, git_commit, val_acc, val_loss, peak_vram_mb, \
             training_seconds, status, description, created_at \
             FROM autoresearch_experiments WHERE run_id = ? ORDER BY created_at ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_experiment).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn open_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::query(include_str!(
            "../../migrations/069_nanochat_models.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn sample_model() -> NewTrainedModel {
        NewTrainedModel {
            display_name: "test-strategy — jun12a — acc 0.55".into(),
            source_strategy_id: Some("strat-01".into()),
            source_strategy_name: Some("Trend Alpha".into()),
            run_tag: "jun12a".into(),
            checkpoint_path: "/models/nanochat/jun12a".into(),
            weights_sha256: "deadbeef".into(),
            input_spec: r#"{"window_bars":64,"indicators":[],"normalization":"zscore"}"#.into(),
            label_strategy: "price_forward".into(),
            label_config: r#"{"pnl":{"$gt":0}}"#.into(),
            best_acc: Some(0.55),
            best_loss: Some(0.9),
            holdout_samples: Some(250),
            autoresearch_run_id: None,
        }
    }

    #[tokio::test]
    async fn insert_and_get_model_roundtrips() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let id = store.insert_model(sample_model()).await.unwrap();
        let got = store.get_model(&id).await.unwrap().expect("model must exist");
        assert_eq!(got.model_id, id);
        assert_eq!(got.run_tag, "jun12a");
        assert!(!got.promoted);
        assert!(!got.live_approved);
    }

    #[tokio::test]
    async fn update_promotion_sets_promoted_flag() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let id = store.insert_model(sample_model()).await.unwrap();
        store.update_promotion(&id).await.unwrap();
        let got = store.get_model(&id).await.unwrap().unwrap();
        assert!(got.promoted);
        assert!(!got.live_approved);
    }

    #[tokio::test]
    async fn set_live_approved_sets_flag() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let id = store.insert_model(sample_model()).await.unwrap();
        store.update_promotion(&id).await.unwrap();
        store.set_live_approved(&id).await.unwrap();
        let got = store.get_model(&id).await.unwrap().unwrap();
        assert!(got.promoted);
        assert!(got.live_approved);
    }

    #[tokio::test]
    async fn list_promoted_models_returns_only_promoted() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let id_a = store.insert_model(sample_model()).await.unwrap();
        let id_b = store.insert_model(sample_model()).await.unwrap();
        store.update_promotion(&id_a).await.unwrap();
        let promoted = store.list_promoted_models().await.unwrap();
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].model_id, id_a);
        assert!(!promoted.iter().any(|m| m.model_id == id_b));
    }

    #[tokio::test]
    async fn insert_and_get_run_roundtrips() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let run_id = store.insert_run(NewAutoresearchRun {
            run_tag: "jun12a".into(),
            source_strategy_id: Some("strat-01".into()),
            label_strategy: "price_forward".into(),
            label_config: "{}".into(),
            git_branch: "autoresearch/jun12a".into(),
            worktree_path: ".worktrees/autoresearch-jun12a".into(),
        }).await.unwrap();
        let got = store.get_run(&run_id).await.unwrap().expect("run must exist");
        assert_eq!(got.run_id, run_id);
        assert_eq!(got.status, "running");
        assert_eq!(got.experiments, 0);
    }

    #[tokio::test]
    async fn update_run_status_to_stopped() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let run_id = store.insert_run(NewAutoresearchRun {
            run_tag: "jun12a".into(),
            source_strategy_id: None,
            label_strategy: "price_forward".into(),
            label_config: "{}".into(),
            git_branch: "autoresearch/jun12a".into(),
            worktree_path: ".worktrees/autoresearch-jun12a".into(),
        }).await.unwrap();
        let ts = chrono::Utc::now().to_rfc3339();
        store.update_run_status(&run_id, "stopped", Some(&ts)).await.unwrap();
        let got = store.get_run(&run_id).await.unwrap().unwrap();
        assert_eq!(got.status, "stopped");
        assert!(got.stopped_at.is_some());
    }

    #[tokio::test]
    async fn bump_experiment_count_increments_and_updates_best() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let run_id = store.insert_run(NewAutoresearchRun {
            run_tag: "jun12a".into(),
            source_strategy_id: None,
            label_strategy: "price_forward".into(),
            label_config: "{}".into(),
            git_branch: "autoresearch/jun12a".into(),
            worktree_path: ".worktrees/autoresearch-jun12a".into(),
        }).await.unwrap();
        let model_id = store.insert_model(sample_model()).await.unwrap();
        store.bump_experiment_count(&run_id, Some(0.55), Some(&model_id)).await.unwrap();
        let got = store.get_run(&run_id).await.unwrap().unwrap();
        assert_eq!(got.experiments, 1);
        assert_eq!(got.best_acc, Some(0.55));
        assert_eq!(got.best_model_id.as_deref(), Some(model_id.as_str()));
    }

    #[tokio::test]
    async fn insert_and_list_experiments_ordered_by_created_at() {
        let pool = open_pool().await;
        let store = NanochatStore::new(pool);
        let run_id = store.insert_run(NewAutoresearchRun {
            run_tag: "jun12a".into(),
            source_strategy_id: None,
            label_strategy: "price_forward".into(),
            label_config: "{}".into(),
            git_branch: "autoresearch/jun12a".into(),
            worktree_path: ".worktrees/autoresearch-jun12a".into(),
        }).await.unwrap();
        store.insert_experiment(NewExperiment {
            run_id: run_id.clone(),
            git_commit: "abc1234".into(),
            val_acc: Some(0.51),
            val_loss: Some(1.1),
            peak_vram_mb: None,
            training_seconds: Some(120.0),
            status: "keep".into(),
            description: "baseline OHLCV only".into(),
        }).await.unwrap();
        store.insert_experiment(NewExperiment {
            run_id: run_id.clone(),
            git_commit: "def5678".into(),
            val_acc: None,
            val_loss: None,
            peak_vram_mb: None,
            training_seconds: None,
            status: "crash".into(),
            description: "added rsi_14 — OOM".into(),
        }).await.unwrap();
        let exps = store.list_experiments(&run_id).await.unwrap();
        assert_eq!(exps.len(), 2);
        assert_eq!(exps[0].git_commit, "abc1234");
        assert_eq!(exps[1].status, "crash");
        assert_eq!(exps[1].val_acc, None);
    }
}
