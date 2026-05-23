//! `/api/agents` — agent records CRUD. Wraps `engine::agents::store` with
//! audit-emitting handlers.
//!
//! v1 surface; see `docs/superpowers/plans/2026-05-11-agents-page-v1.md`.
//! Cross-references (`deployed_in`, `recent_runs`) are wired after the
//! strategies refactor: strategies now carry `Vec<AgentRef>` so we can
//! enumerate referencing strategies and their eval-run history.

use std::cmp::Reverse;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::agents::{
    builtin_templates, validate_agent, Agent, AgentSlot, AgentStore, AgentTemplate, ListFilter, NewAgent,
    UpdateAgent, ValidationDiagnostic,
};
use crate::api::audit::{self, Outcome};
use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::store::{ListFilter as RunListFilter, RunStore};
use crate::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListAgentsRequest {
    pub include_archived: bool,
    pub q: Option<String>,
    pub limit: Option<i64>,
    /// Optional row offset for paged listings. The dashboard's list
    /// route always sets `(limit, offset)`; CLI/MCP callers that want
    /// the full library leave both unset.
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Paged-list envelope used by the dashboard's `/api/agents` route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedAgents {
    pub items: Vec<Agent>,
    pub total: u64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub slots: Vec<AgentSlot>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub slots: Option<Vec<AgentSlot>>,
}

/// A strategy that references an agent. Empty in v1 — see plan §Downstream impact.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRef {
    pub strategy_id: String,
    pub name: String,
}

/// An eval-run that included a given agent. Empty in v1 — strategies don't
/// reference agents yet, so we can't attribute runs to agents.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRef {
    pub run_id: String,
    pub scenario_id: String,
    pub status: String,
}

pub async fn list(ctx: &ApiContext, req: ListAgentsRequest) -> ApiResult<Vec<Agent>> {
    let started = Instant::now();
    let result = list_inner(ctx, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext, req: ListAgentsRequest) -> ApiResult<Vec<Agent>> {
    let store = AgentStore::new(ctx.db.clone());
    store
        .list(ListFilter {
            include_archived: req.include_archived,
            name_contains: req.q,
            limit: req.limit,
            offset: req.offset,
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

/// Paged variant of `list` — returns one page of `Agent` rows plus the
/// total count, sharing the audit + outcome wrapper with `list`.
pub async fn list_paged(ctx: &ApiContext, req: ListAgentsRequest) -> ApiResult<PagedAgents> {
    let started = Instant::now();
    let result = list_paged_inner(ctx, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "list_paged",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_paged_inner(ctx: &ApiContext, req: ListAgentsRequest) -> ApiResult<PagedAgents> {
    let store = AgentStore::new(ctx.db.clone());
    let filter = ListFilter {
        include_archived: req.include_archived,
        name_contains: req.q,
        limit: req.limit,
        offset: req.offset,
    };
    let total = store
        .count(&filter)
        .await
        .map_err(|e| ApiError::Internal(format!("count agents: {e}")))?;
    let items = store
        .list(filter)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(PagedAgents { items, total })
}

pub async fn create(ctx: &ApiContext, req: CreateAgentRequest) -> ApiResult<Agent> {
    let started = Instant::now();
    let target_name = req.name.clone();
    let result = create_inner(ctx, req).await;
    let target = result.as_ref().ok().map(|a| a.agent_id.clone());
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "create",
        target.as_deref(),
        Some(&target_name),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn create_inner(ctx: &ApiContext, req: CreateAgentRequest) -> ApiResult<Agent> {
    let store = AgentStore::new(ctx.db.clone());

    if req.name.trim().is_empty() {
        return Err(ApiError::Validation("name is required".into()));
    }
    if req.slots.is_empty() {
        return Err(ApiError::Validation("agent needs at least one slot".into()));
    }
    if store
        .name_exists(&req.name, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::Conflict(format!(
            "an agent named '{}' already exists",
            req.name
        )));
    }

    let id = store
        .create(NewAgent {
            name: req.name,
            description: req.description,
            tags: req.tags,
            slots: req.slots,
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    store
        .get(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("agent vanished after create".into()))
}

pub async fn get(ctx: &ApiContext, agent_id: &str) -> ApiResult<Agent> {
    let started = Instant::now();
    let result = get_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "get",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn get_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<Agent> {
    let store = AgentStore::new(ctx.db.clone());
    store
        .get(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_id)))
}

pub async fn update(ctx: &ApiContext, agent_id: &str, req: UpdateAgentRequest) -> ApiResult<Agent> {
    let started = Instant::now();
    let result = update_inner(ctx, agent_id, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "update",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn update_inner(ctx: &ApiContext, agent_id: &str, req: UpdateAgentRequest) -> ApiResult<Agent> {
    let store = AgentStore::new(ctx.db.clone());

    if let Some(ref name) = req.name {
        if name.trim().is_empty() {
            return Err(ApiError::Validation("name must be non-empty".into()));
        }
        if store
            .name_exists(name, Some(agent_id))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            return Err(ApiError::Conflict(format!(
                "an agent named '{}' already exists",
                name
            )));
        }
    }

    let updated = store
        .update(
            agent_id,
            UpdateAgent {
                name: req.name,
                description: req.description,
                tags: req.tags,
                slots: req.slots,
            },
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    updated.ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_id)))
}

pub async fn archive(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let result = archive_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "archive",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn archive_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let store = AgentStore::new(ctx.db.clone());
    let archived = store
        .archive(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !archived {
        return Err(ApiError::NotFound(format!(
            "agent {} (or already archived)",
            agent_id
        )));
    }
    Ok(())
}

pub async fn validate(ctx: &ApiContext, agent_id: &str) -> ApiResult<Vec<ValidationDiagnostic>> {
    let started = Instant::now();
    let result = validate_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "validate",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn validate_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<Vec<ValidationDiagnostic>> {
    let store = AgentStore::new(ctx.db.clone());
    let agent = store
        .get(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_id)))?;
    Ok(validate_agent(&agent))
}

/// Returns the strategy ids for every on-disk strategy that contains
/// `agent_id` in its `agents` vec. Best-effort: strategies that fail to
/// load are skipped so a single corrupted file does not break the panel.
async fn referencing_strategy_ids(ctx: &ApiContext, agent_id: &str) -> ApiResult<Vec<String>> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let strategy_ids = store
        .list()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut out = Vec::new();
    for sid in strategy_ids {
        let strategy = match store.load(&sid).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        if strategy.agents.iter().any(|aref| aref.agent_id == agent_id) {
            out.push(sid);
        }
    }
    Ok(out)
}

/// Returns every on-disk strategy that references `agent_id`.
pub async fn deployed_in(ctx: &ApiContext, agent_id: &str) -> ApiResult<Vec<StrategyRef>> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let strategy_ids = store
        .list()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut out = Vec::new();
    for sid in strategy_ids {
        let strategy = match store.load(&sid).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        if strategy.agents.iter().any(|aref| aref.agent_id == agent_id) {
            out.push(StrategyRef {
                strategy_id: strategy.manifest.id.clone(),
                name: strategy.manifest.display_name.clone(),
            });
        }
    }
    Ok(out)
}

/// Starter templates for the `/agents/new` picker. Hardcoded for v1;
/// user-authored templates land later when the strategies refactor
/// makes promote-from-strategy a real flow.
pub async fn templates(_ctx: &ApiContext) -> ApiResult<Vec<AgentTemplate>> {
    Ok(builtin_templates())
}

/// Returns the `limit` most-recent eval runs attributed to any strategy
/// that references `agent_id`, sorted by `started_at` descending.
pub async fn recent_runs(ctx: &ApiContext, agent_id: &str, limit: u32) -> ApiResult<Vec<RunRef>> {
    let referencing = referencing_strategy_ids(ctx, agent_id).await?;
    if referencing.is_empty() {
        return Ok(Vec::new());
    }

    let run_store = RunStore::new(ctx.db.clone());
    let mut all_runs = Vec::new();
    for strategy_id in referencing {
        let runs = run_store
            .list(RunListFilter {
                agent_id: Some(strategy_id),
                scenario_id: None,
                status: None,
                ..Default::default()
            })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        all_runs.extend(runs);
    }

    // Sort newest-first and take the requested limit.
    all_runs.sort_by_key(|r| Reverse(r.started_at));
    all_runs.truncate(limit as usize);

    let refs = all_runs
        .into_iter()
        .map(|run| RunRef {
            run_id: run.id,
            scenario_id: run.scenario_id,
            status: run.status.as_str().to_string(),
        })
        .collect();
    Ok(refs)
}

fn outcome_of<T>(result: &ApiResult<T>) -> Outcome {
    match result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use crate::eval::run::{Run, RunMode, RunStatus};
    use crate::eval::store::RunStore;
    use crate::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use crate::strategies::{manifest::PublicManifest, risk::RiskPreset, AgentRef, PipelineDef, Strategy};
    use chrono::Utc;
    use ulid::Ulid;

    #[test]
    fn create_agent_request_rejects_unknown_fields() {
        let err = serde_json::from_str::<CreateAgentRequest>(
            r#"{"name":"agent","description":"","tags":[],"slots":[],"extra":true}"#,
        )
        .expect_err("unknown create-agent fields must be rejected");

        assert!(err.to_string().contains("unknown field"));
    }

    // ── test helpers ──────────────────────────────────────────────────────────

    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        (ctx, dir)
    }

    fn strategy_referencing(id: &str, display_name: &str, agent_id: &str) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: id.to_string(),
                display_name: display_name.to_string(),
                plain_summary: "test".into(),
                creator: "@test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec![],
                decision_cadence_minutes: 60,
                attested_with: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            hypothesis: None,
            agents: vec![AgentRef {
                agent_id: agent_id.to_string(),
                role: "trader".into(),
                activates: None,
            }],
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
        acknowledge_no_filter: false,
        }
    }

    /// A canonical scenario id seeded by `ApiContext::open` on every fresh
    /// `xvn_home`. Tests that need to insert eval runs must use a known
    /// FK-valid scenario id — using this avoids inserting a separate row.
    const SEEDED_SCENARIO_ID: &str = "crypto-bull-q1-2025";

    fn queued_run(strategy_id: &str, scenario_id: &str) -> Run {
        Run {
            id: Ulid::new().to_string(),
            agent_id: strategy_id.to_string(),
            agents_agent_id: None,
            scenario_id: scenario_id.to_string(),
            params_override: None,
            mode: RunMode::Backtest,
            status: RunStatus::Completed,
            started_at: Utc::now(),
            completed_at: None,
            metrics: None,
            error: None,
            estimated_total_tokens: None,
            actual_input_tokens: None,
            actual_output_tokens: None,
            bars_content_hash: None,
            manifest_canonical: None,
            bars_manifest: None,
        }
    }

    // ── deployed_in tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn deployed_in_returns_empty_when_no_strategy_references_agent() {
        let (ctx, _dir) = fresh_ctx().await;
        let result = deployed_in(&ctx, "01HZAGENT_UNREFERENCED").await.unwrap();
        assert!(result.is_empty(), "expected empty, got {result:?}");
    }

    #[tokio::test]
    async fn deployed_in_returns_both_refs_when_two_strategies_reference_agent() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));

        let s1 = strategy_referencing("01HZSTRATEGY_A00000000001", "Alpha Strategy", "01HZAGENT_SHARED");
        let s2 = strategy_referencing("01HZSTRATEGY_B00000000002", "Beta Strategy", "01HZAGENT_SHARED");
        store.save(&s1).await.unwrap();
        store.save(&s2).await.unwrap();

        let mut result = deployed_in(&ctx, "01HZAGENT_SHARED").await.unwrap();
        // Sort for deterministic comparison (filesystem list order varies).
        result.sort_by_key(|r| r.strategy_id.clone());

        assert_eq!(result.len(), 2, "expected 2 refs, got {result:?}");
        assert_eq!(result[0].strategy_id, "01HZSTRATEGY_A00000000001");
        assert_eq!(result[0].name, "Alpha Strategy");
        assert_eq!(result[1].strategy_id, "01HZSTRATEGY_B00000000002");
        assert_eq!(result[1].name, "Beta Strategy");
    }

    // ── recent_runs tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn recent_runs_returns_empty_when_no_strategy_references_agent() {
        let (ctx, _dir) = fresh_ctx().await;
        let result = recent_runs(&ctx, "01HZAGENT_NOREFERENCE", 5).await.unwrap();
        assert!(result.is_empty(), "expected empty, got {result:?}");
    }

    #[tokio::test]
    async fn recent_runs_returns_one_entry_when_strategy_and_run_exist() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));

        let strategy =
            strategy_referencing("01HZSTRATEGY_RUN0000000001", "Run Strategy", "01HZAGENT_WITHRUN");
        store.save(&strategy).await.unwrap();

        let run = queued_run("01HZSTRATEGY_RUN0000000001", SEEDED_SCENARIO_ID);
        let run_id = run.id.clone();
        RunStore::new(ctx.db.clone()).create(&run).await.unwrap();

        let result = recent_runs(&ctx, "01HZAGENT_WITHRUN", 5).await.unwrap();
        assert_eq!(result.len(), 1, "expected 1 run, got {result:?}");
        assert_eq!(result[0].run_id, run_id);
        assert_eq!(result[0].scenario_id, SEEDED_SCENARIO_ID);
        assert_eq!(result[0].status, "completed");
    }

    #[tokio::test]
    async fn recent_runs_respects_limit_and_orders_by_started_at_desc() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));

        let strategy =
            strategy_referencing("01HZSTRATEGY_LIMIT000000001", "Limit Strategy", "01HZAGENT_LIMIT");
        store.save(&strategy).await.unwrap();

        let run_store = RunStore::new(ctx.db.clone());
        // Insert 3 runs with distinct started_at timestamps.
        let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut run_ids = Vec::new();
        for i in 0u32..3 {
            let mut run = queued_run("01HZSTRATEGY_LIMIT000000001", SEEDED_SCENARIO_ID);
            run.started_at = base + chrono::Duration::hours(i as i64);
            run_ids.push(run.id.clone());
            run_store.create(&run).await.unwrap();
        }

        // limit=2 should return the 2 newest, newest first.
        let result = recent_runs(&ctx, "01HZAGENT_LIMIT", 2).await.unwrap();
        assert_eq!(result.len(), 2, "expected 2 runs, got {result:?}");
        // Newest first: run_ids[2] then run_ids[1].
        assert_eq!(result[0].run_id, run_ids[2]);
        assert_eq!(result[1].run_id, run_ids[1]);
    }
}
