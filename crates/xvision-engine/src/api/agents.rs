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
    ScopeFilter, ScopePatch, UpdateAgent, ValidationDiagnostic,
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
    /// Scope visibility filter. `None` (default) and `Some("")` map to
    /// `ScopeFilter::Workspace` — only workspace agents (rows where
    /// `scope_strategy_id IS NULL`). `Some("all")` opts out of the
    /// filter entirely (diagnostic). Any other value is interpreted as
    /// a strategy id and merges that strategy's scoped agents with the
    /// workspace set. Phase 3 of `agent-firing-filter`, migration 036.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Reject slots whose `system_prompt` is empty or whitespace-only.
///
/// An agent saved with an empty prompt is a silent eval landmine: shape
/// validation passes, but the launch gate later refuses with
/// `missing_prompt`. Catching it at the create/update boundary makes the
/// failure visible while the operator is still on the editor surface,
/// regardless of which caller (wizard, MCP, CLI, clone) routed the
/// request.
fn validate_slot_prompts(slots: &[AgentSlot]) -> ApiResult<()> {
    if let Some(slot) = slots.iter().find(|s| s.system_prompt.trim().is_empty()) {
        return Err(ApiError::Validation(format!(
            "slot '{}' needs a non-empty system_prompt",
            slot.name
        )));
    }
    Ok(())
}

fn resolve_scope(s: Option<&str>) -> ScopeFilter {
    match s {
        None | Some("") => ScopeFilter::Workspace,
        Some("all") => ScopeFilter::All,
        Some(id) => ScopeFilter::Strategy(id.to_string()),
    }
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
    /// Optional strategy id this agent is scoped to. Strategy editor's
    /// inline Filter composer sets this when the "Save as reusable
    /// agent" toggle is OFF — the resulting agent stays hidden from
    /// the workspace list. Migration 036.
    #[serde(default)]
    pub scope_strategy_id: Option<String>,
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
    /// Patch the agent's scope. `None` (default) leaves the column
    /// alone; `Some(ScopePatch::Clear)` promotes a scoped agent to
    /// the workspace; `Some(ScopePatch::Set(strategy_id))` scopes it.
    /// Migration 036.
    #[serde(default)]
    pub scope_strategy_id: Option<ScopePatch>,
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
    let scope = resolve_scope(req.scope.as_deref());
    store
        .list(ListFilter {
            include_archived: req.include_archived,
            name_contains: req.q,
            limit: req.limit,
            offset: req.offset,
            scope,
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
    let scope = resolve_scope(req.scope.as_deref());
    let filter = ListFilter {
        include_archived: req.include_archived,
        name_contains: req.q,
        limit: req.limit,
        offset: req.offset,
        scope,
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
    validate_slot_prompts(&req.slots)?;
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
            scope_strategy_id: req.scope_strategy_id,
        })
        .await
        .map_err(|e| {
            // `validate_agent_for_save` failures are content-quality rejections
            // (prompt too short / placeholder). Surface those as Validation so the
            // UI can display a clear message instead of a generic "internal error".
            let msg = e.to_string();
            if msg.contains("save validation failed:") {
                // Strip the "save validation failed: " prefix so the UI sees the
                // operator-actionable text directly.
                let detail = msg.strip_prefix("save validation failed: ").unwrap_or(&msg);
                ApiError::Validation(detail.to_string())
            } else {
                ApiError::Internal(msg)
            }
        })?;

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

    if let Some(ref slots) = req.slots {
        validate_slot_prompts(slots)?;
    }

    let updated = store
        .update(
            agent_id,
            UpdateAgent {
                name: req.name,
                description: req.description,
                tags: req.tags,
                slots: req.slots,
                scope_strategy_id: req.scope_strategy_id,
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

/// Hard-delete an agent by id. Returns 409 Conflict if any strategy currently
/// references this agent — delete the owning strategy first (or use
/// `xvn strategy rm`) so AgentRefs don't dangle.
pub async fn delete(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let result = agent_delete_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "delete",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn agent_delete_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let store = AgentStore::new(ctx.db.clone());
    if store
        .get(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::NotFound(format!("agent {agent_id}")));
    }
    let refs = referencing_strategy_ids(ctx, agent_id).await?;
    if !refs.is_empty() {
        return Err(ApiError::Conflict(format!(
            "agent is used by strategy {}; delete the strategy first or use `xvn strategy rm`",
            refs[0]
        )));
    }
    let deleted = store
        .delete_by_id(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !deleted {
        return Err(ApiError::NotFound(format!("agent {agent_id}")));
    }
    Ok(())
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
pub async fn referencing_strategy_ids(ctx: &ApiContext, agent_id: &str) -> ApiResult<Vec<String>> {
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

/// Returns the `limit` most-recent eval runs attributed to `agent_id`,
/// sorted by `started_at` descending.
///
/// Uses a DUAL PATH to cover both old and new runs:
///
/// (a) **Strategy-hop path** — finds strategies whose `agents` vec references
///     `agent_id` (post-2026-05-12 refactor), then queries runs by strategy id
///     (`eval_runs.agent_id`). This covers post-refactor runs.
///
/// (b) **Direct path** — queries `eval_runs.agents_agent_id = agent_id`
///     (migration 022 column). This covers new runs where the workspace agent
///     ULID was written directly into the run row.
///
/// Pre-2026-05-12 ("legacy") strategies have an empty `agents: Vec<AgentRef>`,
/// so path (a) misses them. Those legacy runs may only be reachable via path (b)
/// if they pre-date migration 022 as well — in that case `agents_agent_id` is
/// also NULL and neither path finds them, which is an accepted limitation
/// (legacy-on-legacy gap, no backfill).
///
/// Results from both paths are merged and deduplicated by run id before the
/// final `limit` is applied.
pub async fn recent_runs(ctx: &ApiContext, agent_id: &str, limit: u32) -> ApiResult<Vec<RunRef>> {
    let run_store = RunStore::new(ctx.db.clone());

    // (a) Strategy-hop path: find strategies that reference this agent, then
    //     fetch runs attributed to those strategies.
    let referencing = referencing_strategy_ids(ctx, agent_id).await?;
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

    // (b) Direct path: runs that carry the workspace agent ULID in
    //     `agents_agent_id` (migration 022). Covers runs where the strategy
    //     files have an empty `agents` vec (pre-refactor legacy strategies)
    //     or were created via a path that populates `agents_agent_id` directly.
    let direct_runs = run_store
        .list(RunListFilter {
            agents_agent_id: Some(agent_id.to_string()),
            ..Default::default()
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    all_runs.extend(direct_runs);

    // Dedup by run id (a run matched by both paths would appear twice).
    let mut seen = std::collections::HashSet::new();
    all_runs.retain(|r| seen.insert(r.id.clone()));

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
                timeframe_requirements: Default::default(),
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
            agents: vec![AgentRef {
                agent_id: agent_id.to_string(),
                role: "trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            }],
            pipeline: PipelineDef::default(),
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
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            live_config: None,
            paused: false,
            paused_at: None,
            flatten_requested: false,
            source: Default::default(),
            unrealized_pnl_usd: None,
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

    // ── W25 dual-path recent_runs tests ──────────────────────────────────────

    /// A run whose strategy file has an empty `agents` vec (legacy pre-refactor
    /// strategy) is invisible to the strategy-hop path. The direct
    /// `agents_agent_id` path must still surface it so the "RECENT RUNS" panel
    /// shows real activity instead of "No runs yet".
    #[tokio::test]
    async fn recent_runs_via_direct_agents_agent_id_path_finds_legacy_strategy_run() {
        let (ctx, _dir) = fresh_ctx().await;

        // Store a legacy strategy with an EMPTY `agents` vec — it won't
        // match the strategy-hop path because `strategy.agents.iter().any(...)` returns false.
        use crate::strategies::{manifest::PublicManifest, risk::RiskPreset, PipelineDef, Strategy};
        let legacy_strategy_id = "01HZSTRATEGY_LEGACY000000001";
        let agent_ulid = "01HZAGENT_LEGACY00000000001";
        let legacy = Strategy {
            manifest: PublicManifest {
                id: legacy_strategy_id.to_string(),
                display_name: "Legacy Strategy".into(),
                plain_summary: "test".into(),
                creator: "@test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec![],
                decision_cadence_minutes: 60,
                timeframe_requirements: Default::default(),
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
            agents: vec![], // intentionally EMPTY — simulates pre-refactor strategy
            pipeline: PipelineDef::default(),
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
        };
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        store.save(&legacy).await.unwrap();

        // Create a run attributed to the legacy strategy but with `agents_agent_id` set —
        // simulating a run created by the direct path (migration 022+ run on a legacy strategy).
        let mut run = queued_run(legacy_strategy_id, SEEDED_SCENARIO_ID);
        run.agents_agent_id = Some(agent_ulid.to_string());
        let run_id = run.id.clone();
        RunStore::new(ctx.db.clone()).create(&run).await.unwrap();

        // `referencing_strategy_ids` returns nothing for this agent because the strategy's
        // `agents` vec is empty — proving the strategy-hop path is blind to this run.
        let refs = referencing_strategy_ids(&ctx, agent_ulid).await.unwrap();
        assert!(
            refs.is_empty(),
            "strategy-hop path must not find the empty-agents strategy"
        );

        // But `recent_runs` must find the run via the direct path.
        let result = recent_runs(&ctx, agent_ulid, 5).await.unwrap();
        assert_eq!(result.len(), 1, "dual-path must surface the run; got {result:?}");
        assert_eq!(result[0].run_id, run_id);
    }

    /// A run reachable by BOTH paths (strategy-hop and direct) must appear
    /// exactly once in the result (deduplication check).
    #[tokio::test]
    async fn recent_runs_deduplicates_run_visible_on_both_paths() {
        let (ctx, _dir) = fresh_ctx().await;
        let agent_ulid = "01HZAGENT_DEDUP0000000001";
        let strategy_id = "01HZSTRATEGY_DEDUP0000001";

        // Save a strategy whose `agents` vec DOES reference the agent.
        let strategy = strategy_referencing(strategy_id, "Dedup Strategy", agent_ulid);
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        store.save(&strategy).await.unwrap();

        // Create a run attributed to the strategy AND carrying `agents_agent_id` —
        // it would be returned by both paths without dedup.
        let mut run = queued_run(strategy_id, SEEDED_SCENARIO_ID);
        run.agents_agent_id = Some(agent_ulid.to_string());
        let run_id = run.id.clone();
        RunStore::new(ctx.db.clone()).create(&run).await.unwrap();

        let result = recent_runs(&ctx, agent_ulid, 10).await.unwrap();
        assert_eq!(
            result.len(),
            1,
            "run visible on both paths must appear exactly once; got {result:?}"
        );
        assert_eq!(result[0].run_id, run_id);
    }

    // ── delete tests ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn delete_agent_returns_conflict_when_used_by_strategy() {
        use crate::agents::AgentSlot;
        use crate::api::strategy as api_strategy;

        let (ctx, _dir) = fresh_ctx().await;
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));

        let agent = create(
            &ctx,
            CreateAgentRequest {
                name: "exclusive-agent".into(),
                description: "".into(),
                tags: vec![],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openai".into(),
                    model: "gpt-4o".into(),
                    system_prompt: "Test prompt.".into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: crate::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap();

        let strategy = strategy_referencing("01HZSTRATEGY_DELETE_GUARD", "Guard Strategy", &agent.agent_id);
        store.save(&strategy).await.unwrap();

        let r = delete(&ctx, &agent.agent_id).await;
        assert!(
            matches!(r, Err(ApiError::Conflict(_))),
            "expected Conflict when agent is used by a strategy, got {r:?}"
        );
    }

    // ── dgh4 regression: create with short/placeholder prompt must return Validation ──

    #[tokio::test]
    async fn create_with_empty_system_prompt_returns_validation_not_internal() {
        // BLANK_SLOT default: system_prompt="" → validate_slot_prompts fires first.
        let (ctx, _dir) = fresh_ctx().await;
        let err = create(
            &ctx,
            CreateAgentRequest {
                name: "short-prompt-agent".into(),
                description: "".into(),
                tags: vec![],
                slots: vec![crate::agents::AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-6".into(),
                    system_prompt: "".into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: crate::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ApiError::Validation(_)),
            "empty system_prompt must return Validation, got: {err:?}"
        );
    }

    #[test]
    fn save_validation_error_maps_to_api_validation_not_internal() {
        // Unit regression for dgh4: the create_inner error mapper must convert
        // "save validation failed: <detail>" into ApiError::Validation(<detail>),
        // not ApiError::Internal. This is the path `validate_agent_for_save`
        // takes when the gate is active (production) and the prompt is too short.
        //
        // We test the mapping logic directly rather than through the full
        // create stack, because other tests in this module disable the save
        // gate via XVISION_DISABLE_AGENT_SAVE_GATE and manipulating that env
        // var in a concurrent test suite is inherently racy.
        let raw_err = anyhow::anyhow!(
            "save validation failed: slot 'main': system_prompt is the default placeholder \
             or fewer than 200 characters; replace with a real trading prompt before saving"
        );
        let msg = raw_err.to_string();
        let api_err = if msg.contains("save validation failed:") {
            let detail = msg.strip_prefix("save validation failed: ").unwrap_or(&msg);
            ApiError::Validation(detail.to_string())
        } else {
            ApiError::Internal(msg)
        };
        match api_err {
            ApiError::Validation(detail) => {
                assert!(
                    detail.contains("system_prompt")
                        || detail.contains("characters")
                        || detail.contains("placeholder"),
                    "detail must be operator-actionable, got: {detail}"
                );
            }
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_with_valid_agent_succeeds() {
        // Regression: a well-formed create (>200-char prompt, non-empty provider/model)
        // must succeed and return the created agent with the correct name.
        let (ctx, _dir) = fresh_ctx().await;
        let agent = create(
            &ctx,
            CreateAgentRequest {
                name: "valid-agent".into(),
                description: "A properly formed agent.".into(),
                tags: vec![],
                slots: vec![crate::agents::AgentSlot {
                    name: "main".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-6".into(),
                    system_prompt: "You are a quantitative trading assistant. Analyse the OHLCV data \
                             provided and respond with a JSON object containing: action \
                             (buy/sell/hold), size_pct (0–100), and reason (string). \
                             Apply disciplined risk management: never risk more than 1% of \
                             notional equity per trade, and always respect the configured \
                             stop-loss and take-profit levels. Avoid over-trading on low-volume bars."
                        .into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: crate::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .expect("valid agent create must succeed");
        assert_eq!(agent.name, "valid-agent");
        assert!(!agent.agent_id.is_empty(), "agent_id must be populated");
    }

    #[tokio::test]
    async fn delete_agent_succeeds_when_not_referenced_by_any_strategy() {
        use crate::agents::AgentSlot;

        let (ctx, _dir) = fresh_ctx().await;

        let agent = create(
            &ctx,
            CreateAgentRequest {
                name: "standalone-agent".into(),
                description: "".into(),
                tags: vec![],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openai".into(),
                    model: "gpt-4o".into(),
                    system_prompt: "Test prompt.".into(),
                    skill_ids: vec![],
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: crate::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap();

        delete(&ctx, &agent.agent_id).await.unwrap();
        let r = get(&ctx, &agent.agent_id).await;
        assert!(
            matches!(r, Err(ApiError::NotFound(_))),
            "agent should be gone after delete"
        );
    }
}
