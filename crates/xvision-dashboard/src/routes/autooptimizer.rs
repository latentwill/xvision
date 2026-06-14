//! `/api/autooptimizer/*` — read-only REST endpoints for the autooptimizer
//! substrate (AR-3 backend).
//!
//! These handlers expose the lineage graph, mutator ladder, diversity samples,
//! and judge findings via direct `sqlx` queries against the shared
//! `AppState::pool`. All routes are GET-only and registered in
//! `server::readonly_router`.
//!
//! ## Endpoint inventory
//!
//! - `GET /api/autooptimizer/status`
//! - `GET /api/autooptimizer/sessions[?limit=&offset=]`
//! - `GET /api/autooptimizer/sessions/:id`
//! - `GET /api/autooptimizer/lineage[?status=active|rejected|quarantined&cycle_id=&limit=&offset=]`
//! - `GET /api/autooptimizer/lineage/:hash`
//! - `GET /api/autooptimizer/ladder[?since=<rfc3339>]`
//! - `GET /api/autooptimizer/diversity[?cycle_id=&limit=]`
//! - `GET /api/autooptimizer/findings/:bundle_hash`
//! - `GET /api/autooptimizer/blob/:hash`
//! - `GET /api/autooptimizer/flywheel`
//!
//! ## Notes on `findings`
//!
//! Judge `Finding`s are produced at LLM-evaluation time inside an optimizer
//! cycle run and are surfaced via SSE progress events (`CycleProgressEvent::
//! JudgeFinding`). They are **not** persisted to the DB or blob store — the
//! `findings` endpoint therefore always returns an empty array. It exists
//! as a stable API surface for a future AR-N task that adds persistence.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use xvision_engine::autooptimizer::{
    cycle_runs::{
        get_cycle_cost, get_cycle_run, list_cycle_runs, list_cycle_runs_filtered, CycleCost, CycleRunDetail,
        CycleRunSummary,
    },
    evidence::{load_findings, load_gate_record, FindingRow, GateRecordRow},
    lineage::{LineageNode, LineageStatus, LineageStore},
    mutator_ladder::{compute_ladder, MutatorScore},
    regime_results::load_regime_results,
    session::{create_session, get_active_session, OptimizerSession},
};

use crate::error::DashboardError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Session routes: GET /status, GET /sessions, GET /sessions/:id,
//                 POST /sessions
// ---------------------------------------------------------------------------

/// Summary of an active session for the /status response.
#[derive(Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub strategy_id: String,
    pub state: String,
    pub mode: String,
    pub cycles_completed: i64,
    pub kept_count: i64,
    pub suspect_count: i64,
    pub dropped_count: i64,
    pub errored_count: i64,
    pub cost_usd: Option<f64>,
}

impl From<OptimizerSession> for SessionSummary {
    fn from(s: OptimizerSession) -> Self {
        SessionSummary {
            session_id: s.session_id,
            strategy_id: s.strategy_id,
            state: s.state,
            mode: s.mode,
            cycles_completed: s.cycles_completed,
            kept_count: s.kept_count,
            suspect_count: s.suspect_count,
            dropped_count: s.dropped_count,
            errored_count: s.errored_count,
            cost_usd: None,
        }
    }
}

/// Response body for `GET /api/autooptimizer/status`.
#[derive(Serialize)]
pub struct StatusResponse {
    pub active_session: Option<SessionSummary>,
    pub last_event_seq: i64,
    /// Control Tower S0 (O3): the in-flight cycle id for the active session,
    /// derived from the newest `autooptimizer_events` row that carries a
    /// `cycle_id`. Pause/resume controls target this cycle via
    /// `POST /api/autooptimizer/cycles/:cycle_id/{pause,resume}` (the only
    /// mounted pause/resume surface — there is no session-level pause route).
    /// `None` when no session is active or no cycle has emitted events yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_cycle_id: Option<String>,
}

/// GET /api/autooptimizer/status
///
/// Returns the active session (if any) and the highest persisted SSE event seq.
pub async fn get_status(State(state): State<AppState>) -> Result<Json<StatusResponse>, DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_session_state").await? {
        return Ok(Json(StatusResponse {
            active_session: None,
            last_event_seq: 0,
            active_cycle_id: None,
        }));
    }

    let active = get_active_session(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(e))?
        .map(SessionSummary::from);

    let has_events = table_exists(&state.pool, "autooptimizer_events").await?;

    let last_event_seq: i64 = if has_events {
        sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM autooptimizer_events")
            .fetch_one(&state.pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        0
    };

    // The pause/resume registry is keyed by cycle_id; surface the active
    // session's newest in-flight cycle so the Active-tasks strip can drive it.
    let active_cycle_id: Option<String> = match (&active, has_events) {
        (Some(s), true) => active_session_cycle_id(&state.pool, &s.session_id).await?,
        _ => None,
    };

    Ok(Json(StatusResponse {
        active_session: active,
        last_event_seq,
        active_cycle_id,
    }))
}

/// Request body for `POST /api/autooptimizer/sessions`.
#[derive(Deserialize)]
pub struct StartSessionBody {
    pub strategy_id: String,
    pub mode: String,
    pub cycles_planned: Option<i64>,
    pub budget_usd: Option<f64>,
    pub config_json: Option<String>,
}

/// Response body for `POST /api/autooptimizer/sessions`.
#[derive(Serialize)]
pub struct StartSessionResponse {
    pub session_id: String,
}

/// POST /api/autooptimizer/sessions
///
/// Creates a new optimizer session. Returns 409 if a session is already active.
pub async fn start_session(
    State(state): State<AppState>,
    Json(body): Json<StartSessionBody>,
) -> Result<(StatusCode, Json<StartSessionResponse>), DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_session_state").await? {
        // Table not yet created — first call will create it via migration.
        // Run the migration inline (idempotent).
        run_session_migration(&state.pool).await?;
    }

    // Check for active session.
    let active = get_active_session(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    if active.is_some() {
        return Err(DashboardError::Conflict("session already active".to_string()));
    }

    let config_json = body.config_json.unwrap_or_else(|| "{}".to_string());
    let session_id = create_session(
        &state.pool,
        &body.strategy_id,
        &config_json,
        &body.mode,
        body.cycles_planned,
    )
    .await
    .map_err(|e| DashboardError::Internal(e))?;

    Ok((StatusCode::ACCEPTED, Json(StartSessionResponse { session_id })))
}

/// Query parameters for `GET /api/autooptimizer/sessions`.
#[derive(Deserialize, Default)]
pub struct SessionListQuery {
    #[serde(default = "default_session_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_session_limit() -> i64 {
    20
}

/// One session row enriched with the two cross-table digest signals the
/// Control Tower home strip needs (S0 / O1b+O1c). The session table itself
/// carries no realized cost or honesty column, so both are aggregated from
/// the per-cycle tables via the `autooptimizer_events(session_id, cycle_id)`
/// link and serialized alongside the flattened session fields.
#[derive(Serialize)]
pub struct SessionListRow {
    #[serde(flatten)]
    pub session: OptimizerSession,
    /// Sum of `cycle_cost.cost_usd` across every cycle this session ran.
    /// `None` (renders as "$?") when no cost rows exist yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// The most-recent cycle's honesty-check outcome for this session
    /// (`true` = passed, `false` = failed). `None` (renders as "—") when no
    /// cycle in the session has an honesty-check row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub honesty_passed: Option<bool>,
    /// The most-recent cycle's accepted-lineage edge over the random baseline
    /// (`parent_score - random_baseline_score`) for this session. A live health
    /// glance: > 0 = the lineage still beats a no-intelligence random agent;
    /// trending toward 0 = decaying toward noise. `None` (renders "—") when no
    /// cycle has an edge record yet (pre-061 or no baseline run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_parent_edge: Option<f64>,
}

/// GET /api/autooptimizer/sessions
///
/// Returns sessions newest-first, each enriched with realized cost and the
/// latest honesty-check outcome (S0 / O1b+O1c).
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<SessionListQuery>,
) -> Result<Json<Vec<SessionListRow>>, DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_session_state").await? {
        return Ok(Json(Vec::new()));
    }

    let sessions: Vec<OptimizerSession> = sqlx::query_as(
        "SELECT * FROM autooptimizer_session_state \
         ORDER BY created_at DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(q.limit)
    .bind(q.offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;

    // Aggregation tables are created lazily by the optimizer; treat absence as
    // "no data" rather than an error so the strip still renders the counts.
    let has_events = table_exists(&state.pool, "autooptimizer_events").await?;
    let has_cost = has_events && table_exists(&state.pool, "cycle_cost").await?;
    let has_honesty = has_events && table_exists(&state.pool, "cycle_honesty_checks").await?;
    let has_edge = has_events
        && table_exists(&state.pool, "autooptimizer_gate_records").await?
        && table_exists(&state.pool, "lineage_nodes").await?;

    let mut rows = Vec::with_capacity(sessions.len());
    for session in sessions {
        let cost_usd = if has_cost {
            session_cost_usd(&state.pool, &session.session_id).await?
        } else {
            None
        };
        let honesty_passed = if has_honesty {
            session_latest_honesty(&state.pool, &session.session_id).await?
        } else {
            None
        };
        let latest_parent_edge = if has_edge {
            session_latest_parent_edge(&state.pool, &session.session_id).await?
        } else {
            None
        };
        rows.push(SessionListRow {
            session,
            cost_usd,
            honesty_passed,
            latest_parent_edge,
        });
    }

    Ok(Json(rows))
}

/// Realized cost for a session = Σ `cycle_cost.cost_usd` over the distinct
/// cycles it ran. Returns `None` when the session has no priced cycle yet
/// (SUM over zero rows is SQL NULL), so the UI shows "$?" rather than "$0.00".
async fn session_cost_usd(pool: &sqlx::SqlitePool, session_id: &str) -> Result<Option<f64>, DashboardError> {
    sqlx::query_scalar::<_, Option<f64>>(
        "SELECT SUM(cc.cost_usd) FROM cycle_cost cc \
         WHERE cc.cycle_id IN ( \
            SELECT DISTINCT cycle_id FROM autooptimizer_events \
            WHERE session_id = ? AND cycle_id IS NOT NULL )",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))
}

/// The newest in-flight cycle id for a session (by event `seq`). Drives the
/// Active-tasks pause/resume controls (S0 / O3). `None` when the session has
/// emitted no cycle-bearing events.
async fn active_session_cycle_id(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> Result<Option<String>, DashboardError> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT cycle_id FROM autooptimizer_events \
         WHERE session_id = ? AND cycle_id IS NOT NULL \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))
    .map(Option::flatten)
}

/// The newest honesty-check outcome among the session's cycles (by
/// `cycle_honesty_checks.created_at`). `None` when none of its cycles ran one.
async fn session_latest_honesty(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> Result<Option<bool>, DashboardError> {
    let passed: Option<i64> = sqlx::query_scalar(
        "SELECT h.passed FROM cycle_honesty_checks h \
         WHERE h.cycle_id IN ( \
            SELECT DISTINCT cycle_id FROM autooptimizer_events \
            WHERE session_id = ? AND cycle_id IS NOT NULL ) \
         ORDER BY h.created_at DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(passed.map(|p| p != 0))
}

/// The most-recent cycle's accepted-lineage edge over the random baseline for
/// this session: `autooptimizer_gate_records.parent_edge` for the newest
/// lineage node belonging to one of the session's cycles. `None` when no such
/// record carries an edge value (pre-061 or no baseline run). Gate records key
/// on `bundle_hash`; `lineage_nodes` maps that to a `cycle_id`.
async fn session_latest_parent_edge(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> Result<Option<f64>, DashboardError> {
    let edge: Option<f64> = sqlx::query_scalar(
        "SELECT agr.parent_edge FROM autooptimizer_gate_records agr \
         JOIN lineage_nodes ln ON ln.bundle_hash = agr.bundle_hash \
         WHERE agr.parent_edge IS NOT NULL \
           AND ln.cycle_id IN ( \
              SELECT DISTINCT cycle_id FROM autooptimizer_events \
              WHERE session_id = ? AND cycle_id IS NOT NULL ) \
         ORDER BY ln.created_at DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(edge)
}

/// GET /api/autooptimizer/sessions/:id
///
/// Returns a single session or 404 if not found.
pub async fn get_session(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<OptimizerSession>, DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_session_state").await? {
        return Err(DashboardError::NotFound(format!(
            "session '{session_id}' not found"
        )));
    }

    let row: Option<OptimizerSession> =
        sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
            .bind(&session_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?;

    match row {
        Some(s) => Ok(Json(s)),
        None => Err(DashboardError::NotFound(format!(
            "session '{session_id}' not found"
        ))),
    }
}

/// Idempotently run migration 057 (session + events tables).
async fn run_session_migration(pool: &sqlx::SqlitePool) -> Result<(), DashboardError> {
    let sql = include_str!("../../../xvision-engine/migrations/057_autooptimizer_sessions.sql");
    for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Query parameter structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct LineageListQuery {
    /// Filter by lineage status: "active", "rejected", or "quarantined" (suspect).
    pub status: Option<String>,
    pub cycle_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Deserialize, Default)]
pub struct LadderQuery {
    /// ISO-8601 lower bound for `proposed_at`. Defaults to 30 days ago.
    pub since: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct DiversityQuery {
    pub cycle_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Deserialize, Default)]
pub struct CycleRunListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    /// Filter to cycles belonging to a specific optimizer session.
    /// Resolved via the `autooptimizer_events(session_id, cycle_id)` bridge —
    /// the same join the stats and session-list handlers use. `None` returns
    /// all cycles (existing behaviour).
    pub session_id: Option<String>,
}

fn default_limit() -> i64 {
    50
}

// ---------------------------------------------------------------------------
// Response helper: diversity row
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DiversityRow {
    pub bundle_hash: String,
    pub diversity_score: f64,
    pub cycle_id: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/lineage
// ---------------------------------------------------------------------------

pub async fn list_lineage(
    State(state): State<AppState>,
    Query(q): Query<LineageListQuery>,
) -> Result<Json<Vec<LineageNode>>, DashboardError> {
    let pool = &state.pool;
    if !table_exists(pool, "lineage_nodes").await? {
        return Ok(Json(Vec::new()));
    }

    let rows = if let (Some(status_str), Some(cycle_id)) = (&q.status, &q.cycle_id) {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, \
             created_at, diversity_score \
             FROM lineage_nodes WHERE status = ? AND cycle_id = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(status_str)
        .bind(cycle_id)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else if let Some(status_str) = &q.status {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, \
             created_at, diversity_score \
             FROM lineage_nodes WHERE status = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(status_str)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else if let Some(cycle_id) = &q.cycle_id {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, \
             created_at, diversity_score \
             FROM lineage_nodes WHERE cycle_id = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(cycle_id)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, \
             created_at, diversity_score \
             FROM lineage_nodes \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    };

    let nodes: Vec<LineageNode> = rows
        .into_iter()
        .map(row_to_lineage_node)
        .collect::<Result<_, _>>()?;
    Ok(Json(nodes))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/lineage/:hash
// ---------------------------------------------------------------------------

pub async fn get_lineage_node(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<LineageNode>, DashboardError> {
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Err(DashboardError::NotFound(format!(
            "lineage node '{hash}' not found"
        )));
    }
    let content_hash = xvision_engine::autooptimizer::ContentHash::from_hex(&hash).map_err(|e| {
        DashboardError::Validation {
            field: "hash".into(),
            msg: format!("invalid content hash: {e}"),
        }
    })?;
    let store = LineageStore::new(state.pool.clone());
    let node = store
        .get(&content_hash)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    match node {
        Some(n) => Ok(Json(n)),
        None => Err(DashboardError::NotFound(format!(
            "lineage node '{hash}' not found"
        ))),
    }
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/cycles
//
// F13/F19 (QA 2026-06-04): a first-class "historic run" list derived from the
// lineage nodes a completed `run-cycle` produced (grouped by cycle_id). The
// pre-existing `GET /api/autooptimizer` list serves the memory-distillation
// ledger, which mutation cycles deliberately don't write to; this surfaces the
// cycles an operator actually ran.
// ---------------------------------------------------------------------------

/// One historic cycle, the engine's [`CycleRunSummary`] enriched with the
/// strategy it optimized. `CycleRunSummary` itself carries no strategy column
/// (it is grouped purely from `lineage_nodes` by `cycle_id`); the strategy is
/// resolved here through the `autooptimizer_events(session_id, cycle_id)` bridge
/// to `autooptimizer_session_state.strategy_id` — the same join the stats and
/// session-list handlers already use. `None` for CLI cycles that ran before the
/// events bridge existed (or never wrote a session row).
#[derive(Serialize)]
pub struct CycleRunRow {
    #[serde(flatten)]
    pub summary: CycleRunSummary,
    /// The strategy (agent_id) this cycle optimized. `None` when the cycle has
    /// no session bridge row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_id: Option<String>,
}

pub async fn list_cycles(
    State(state): State<AppState>,
    Query(q): Query<CycleRunListQuery>,
) -> Result<Json<Vec<CycleRunRow>>, DashboardError> {
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Ok(Json(Vec::new()));
    }

    // When a session_id filter is requested, resolve the matching cycle_ids
    // via the autooptimizer_events bridge (same join the stats handler uses).
    let allowed_cycle_ids: Option<Vec<String>> = if let Some(ref sid) = q.session_id {
        if !table_exists(&state.pool, "autooptimizer_events").await?
            || !table_exists(&state.pool, "autooptimizer_session_state").await?
        {
            return Ok(Json(Vec::new()));
        }
        // Reuse the same bridge query shape as load_filtered_cycle_ids_stats.
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT DISTINCT cycle_id FROM autooptimizer_events \
             WHERE session_id = ? AND cycle_id IS NOT NULL",
        )
        .bind(sid)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?;
        let ids: Vec<String> = rows
            .into_iter()
            .map(|r| {
                r.try_get::<String, _>("cycle_id")
                    .map_err(|e| DashboardError::Internal(e.into()))
            })
            .collect::<Result<_, _>>()?;
        if ids.is_empty() {
            return Ok(Json(Vec::new()));
        }
        Some(ids)
    } else {
        None
    };

    let runs = if let Some(ref ids) = allowed_cycle_ids {
        list_cycle_runs_filtered(&state.pool, ids, q.limit, q.offset)
            .await
            .map_err(DashboardError::Internal)?
    } else {
        list_cycle_runs(&state.pool, q.limit, q.offset)
            .await
            .map_err(DashboardError::Internal)?
    };

    // Resolve each cycle's strategy via the events → session bridge. Skipped
    // entirely when the bridge tables are absent (fresh install), so the list
    // still renders with `strategy_id` omitted.
    let has_bridge = table_exists(&state.pool, "autooptimizer_events").await?
        && table_exists(&state.pool, "autooptimizer_session_state").await?;

    let mut rows = Vec::with_capacity(runs.len());
    for summary in runs {
        let strategy_id = if has_bridge {
            cycle_strategy_id(&state.pool, &summary.cycle_id).await?
        } else {
            None
        };
        rows.push(CycleRunRow { summary, strategy_id });
    }
    Ok(Json(rows))
}

/// The strategy a cycle optimized, resolved through the
/// `autooptimizer_events(session_id, cycle_id)` bridge to
/// `autooptimizer_session_state.strategy_id`. `None` when no session row links
/// to this cycle (e.g. a pre-bridge CLI cycle).
async fn cycle_strategy_id(
    pool: &sqlx::SqlitePool,
    cycle_id: &str,
) -> Result<Option<String>, DashboardError> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT ss.strategy_id FROM autooptimizer_session_state ss \
         WHERE ss.session_id = ( \
            SELECT session_id FROM autooptimizer_events \
            WHERE cycle_id = ? ORDER BY seq DESC LIMIT 1 ) \
         LIMIT 1",
    )
    .bind(cycle_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))
    .map(Option::flatten)
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/cycles/:cycle_id
// ---------------------------------------------------------------------------

pub async fn get_cycle(
    Path(cycle_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<CycleRunDetail>, DashboardError> {
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Err(DashboardError::NotFound(format!(
            "optimizer cycle '{cycle_id}' not found"
        )));
    }
    match get_cycle_run(&state.pool, &cycle_id)
        .await
        .map_err(DashboardError::Internal)?
    {
        Some(detail) => Ok(Json(detail)),
        None => Err(DashboardError::NotFound(format!(
            "optimizer cycle '{cycle_id}' not found"
        ))),
    }
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/cycles/:cycle_id/cost
//
// F35.3: live per-cycle cost/tokens for the Live tab. Reads `cycle_cost`
// directly (the background ticker persists it every ~10s), so it returns
// climbing spend *during* a run — and, crucially, before the first lineage
// node commits (the runaway-token case the operator hit). Unlike
// `GET /cycles/:id` this never 404s on a known-but-node-less cycle; an unknown
// id simply returns `recorded: false` with null totals.
// ---------------------------------------------------------------------------

pub async fn get_cycle_cost_handler(
    Path(cycle_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<CycleCost>, DashboardError> {
    // `cycle_cost` is created lazily by the lineage store; absent table → no spend
    // recorded yet, which is a valid "not recorded" answer, not an error.
    if !table_exists(&state.pool, "cycle_cost").await? {
        return Ok(Json(CycleCost {
            cycle_id,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            unpriced_calls: None,
            recorded: false,
        }));
    }
    Ok(Json(get_cycle_cost(&state.pool, &cycle_id).await))
}

// ---------------------------------------------------------------------------
// POST /api/autooptimizer/lineage/:hash/retire
//
// F29: retire a cycle-produced candidate by moving its lineage node to
// `Rejected` (the operator-surface "Rejected" status). Brings the CLI
// `xvn optimizer retire` affordance to the dashboard genealogy. Idempotent:
// retiring an already-rejected node succeeds. 404 when no node has that hash.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RetireResponse {
    pub bundle_hash: String,
    pub status: String,
    pub message: String,
}

pub async fn retire_lineage_node(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<RetireResponse>, DashboardError> {
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Err(DashboardError::NotFound(format!(
            "lineage node '{hash}' not found"
        )));
    }
    let content_hash = xvision_engine::autooptimizer::ContentHash::from_hex(&hash).map_err(|e| {
        DashboardError::Validation {
            field: "hash".into(),
            msg: format!("invalid content hash: {e}"),
        }
    })?;
    let store = LineageStore::new(state.pool.clone());
    let updated = store
        .set_status(&content_hash, LineageStatus::Rejected)
        .await
        .map_err(DashboardError::Internal)?;
    if !updated {
        return Err(DashboardError::NotFound(format!(
            "lineage node '{hash}' not found"
        )));
    }
    Ok(Json(RetireResponse {
        bundle_hash: hash,
        status: "rejected".into(),
        message: "Experiment retired (moved to Rejected).".into(),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/ladder
// ---------------------------------------------------------------------------

pub async fn get_ladder(
    State(state): State<AppState>,
    Query(q): Query<LadderQuery>,
) -> Result<Json<Vec<MutatorScore>>, DashboardError> {
    if !table_exists(&state.pool, "mutator_attribution").await?
        || !table_exists(&state.pool, "lineage_nodes").await?
    {
        return Ok(Json(Vec::new()));
    }
    let since: DateTime<Utc> = match &q.since {
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map_err(|e| DashboardError::Validation {
                field: "since".into(),
                msg: format!("invalid RFC-3339 timestamp: {e}"),
            })?
            .with_timezone(&Utc),
        None => Utc::now() - chrono::Duration::days(30),
    };

    let scores = compute_ladder(&state.pool, since)
        .await
        .map_err(|e| DashboardError::Internal(e))?;
    Ok(Json(scores))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/diversity
// ---------------------------------------------------------------------------

pub async fn list_diversity(
    State(state): State<AppState>,
    Query(q): Query<DiversityQuery>,
) -> Result<Json<Vec<DiversityRow>>, DashboardError> {
    let pool = &state.pool;
    if !table_exists(pool, "lineage_nodes").await? {
        return Ok(Json(Vec::new()));
    }

    let rows = if let Some(cycle_id) = &q.cycle_id {
        sqlx::query(
            "SELECT bundle_hash, diversity_score, cycle_id, created_at \
             FROM lineage_nodes \
             WHERE diversity_score IS NOT NULL AND cycle_id = ? \
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(cycle_id)
        .bind(q.limit)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        sqlx::query(
            "SELECT bundle_hash, diversity_score, cycle_id, created_at \
             FROM lineage_nodes \
             WHERE diversity_score IS NOT NULL \
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(q.limit)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    };

    let out: Vec<DiversityRow> = rows
        .into_iter()
        .map(|row| -> Result<DiversityRow, DashboardError> {
            use sqlx::Row;
            let bundle_hash: String = row
                .try_get("bundle_hash")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            let diversity_score: f64 = row
                .try_get("diversity_score")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            let cycle_id: Option<String> = row
                .try_get("cycle_id")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            let created_at: String = row
                .try_get("created_at")
                .map_err(|e| DashboardError::Internal(e.into()))?;
            Ok(DiversityRow {
                bundle_hash,
                diversity_score,
                cycle_id,
                created_at,
            })
        })
        .collect::<Result<_, _>>()?;

    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/findings/:bundle_hash
//
// P2-W2: Judge Findings are now persisted to `autooptimizer_findings` at
// emit time (cycle.rs). This endpoint queries that table and returns all
// findings for the given bundle_hash ordered by created_at ascending.
// Returns an empty array when no findings have been written for this hash.
// ---------------------------------------------------------------------------

pub async fn get_findings(
    Path(bundle_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<FindingRow>>, DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_findings").await? {
        return Ok(Json(Vec::new()));
    }
    let rows = load_findings(&state.pool, &bundle_hash)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/experiments/:hash/detail
//
// P2-W2: 5-field detail envelope for a single experiment (lineage node):
//   { lineage_node, rationale, gate_record, findings, regime_results }
//
// - lineage_node: from `lineage_nodes` (404 if not found)
// - rationale: from gate_record.rationale (null if no gate record)
// - gate_record: from `autooptimizer_gate_records` (null if not yet written)
// - findings: from `autooptimizer_findings` (may be empty)
// - regime_results: from `autooptimizer_regime_results` (may be empty)
// ---------------------------------------------------------------------------

/// Response body for `GET /api/autooptimizer/experiments/:hash/detail`.
#[derive(serde::Serialize)]
pub struct ExperimentDetail {
    pub lineage_node: LineageNode,
    pub rationale: Option<String>,
    pub gate_record: Option<GateRecordRow>,
    pub findings: Vec<FindingRow>,
    pub regime_results: Vec<crate::routes::autooptimizer::RegimeResultOut>,
}

/// Serialisable regime result (mirrors `CycleNodeDetail.regime_results`).
#[derive(serde::Serialize)]
pub struct RegimeResultOut {
    pub regime_label: String,
    pub side: xvision_engine::autooptimizer::config::RegimeSide,
    pub delta_sharpe: f64,
    pub verdict: String,
    pub metrics_day: xvision_engine::eval::MetricsSummary,
    pub metrics_untouched: xvision_engine::eval::MetricsSummary,
}

pub async fn get_experiment_detail(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ExperimentDetail>, DashboardError> {
    use xvision_engine::autooptimizer::ContentHash;

    // 404 when the lineage table doesn't exist yet.
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Err(DashboardError::NotFound(format!("experiment '{hash}' not found")));
    }

    let content_hash = ContentHash::from_hex(&hash).map_err(|e| DashboardError::Validation {
        field: "hash".into(),
        msg: format!("invalid content hash: {e}"),
    })?;

    let store = LineageStore::new(state.pool.clone());
    let node = store.get(&content_hash).await.map_err(DashboardError::Internal)?;
    let lineage_node = match node {
        Some(n) => n,
        None => return Err(DashboardError::NotFound(format!("experiment '{hash}' not found"))),
    };

    // Gate record (null if table absent or no row yet).
    let gate_record = if table_exists(&state.pool, "autooptimizer_gate_records").await? {
        load_gate_record(&state.pool, &hash)
            .await
            .map_err(DashboardError::Internal)?
    } else {
        None
    };

    // Rationale lives in the gate record.
    let rationale = gate_record.as_ref().and_then(|g| g.rationale.clone());

    // Findings (empty if table absent).
    let findings = if table_exists(&state.pool, "autooptimizer_findings").await? {
        load_findings(&state.pool, &hash)
            .await
            .map_err(DashboardError::Internal)?
    } else {
        Vec::new()
    };

    // Regime results (empty if table absent or no rows).
    let regime_results = if table_exists(&state.pool, "autooptimizer_regime_results").await? {
        load_regime_results(&state.pool, &hash)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| RegimeResultOut {
                regime_label: r.regime_label,
                side: r.side,
                delta_sharpe: r.delta_sharpe,
                verdict: r.verdict,
                metrics_day: r.metrics_day,
                metrics_untouched: r.metrics_untouched,
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(Json(ExperimentDetail {
        lineage_node,
        rationale,
        gate_record,
        findings,
        regime_results,
    }))
}

// ---------------------------------------------------------------------------
// Private row-mapping helper
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/blob/:hash
// ---------------------------------------------------------------------------

pub async fn get_blob(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    use xvision_engine::autooptimizer::{blob_store::BlobStore, ContentHash};

    let content_hash = ContentHash::from_hex(&hash).map_err(|e| DashboardError::Validation {
        field: "hash".into(),
        msg: format!("invalid content hash: {e}"),
    })?;

    let blob_root = state.xvn_home.join("lineage").join("blobs");
    let store = BlobStore::new(blob_root);

    if !store.exists(&content_hash) {
        return Err(DashboardError::NotFound(format!("blob '{hash}' not found")));
    }

    let value = store
        .get_json(&content_hash)
        .await
        .map_err(DashboardError::Internal)?;
    Ok(Json(value))
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/flywheel  (P3-W2)
//
// Returns a summary of the DSPy flywheel state: whether dspy is enabled,
// the current cohort count (live observations in the memory store), the
// compiled pattern count and latest compile record from agent_slot_optimizations,
// and the most recent optimizer session id.
//
// When dspy_enabled=false returns { "enabled": false } immediately.
// ---------------------------------------------------------------------------

/// The `last_prompt_compile` sub-object returned by the flywheel endpoint.
#[derive(Serialize)]
pub struct LastPromptCompile {
    pub dev_metric: Option<String>,
    pub parent_dev_score: Option<f64>,
    pub child_dev_score: Option<f64>,
    pub delta_dev: Option<f64>,
    pub parent_holdout_score: Option<f64>,
    pub child_holdout_score: Option<f64>,
    pub delta_holdout: Option<f64>,
    pub gate_verdict: String,
    pub gated_at: String,
}

/// Response body for `GET /api/autooptimizer/flywheel`.
/// Uses an untagged enum so we can return either `{ enabled: false }` or
/// the full record without a discriminant key.
#[derive(Serialize)]
#[serde(untagged)]
pub enum FlywheelResponse {
    Disabled {
        enabled: bool,
    },
    Enabled {
        enabled: bool,
        cohort_count: i64,
        threshold: usize,
        compiled_pattern_count: i64,
        latest_optimization_run_id: Option<String>,
        last_prompt_compile: Option<LastPromptCompile>,
    },
}

/// GET /api/autooptimizer/flywheel
///
/// Returns the DSPy flywheel state. When `dspy_enabled=false` in the
/// autooptimizer config, returns `{ "enabled": false }`.
pub async fn get_flywheel(State(state): State<AppState>) -> Result<Json<FlywheelResponse>, DashboardError> {
    // Load config from the standard path (or default when file is absent).
    let cfg = load_autooptimizer_config_for_flywheel()?;

    if !cfg.dspy_enabled {
        return Ok(Json(FlywheelResponse::Disabled { enabled: false }));
    }

    let pool = &state.pool;

    // cohort_count: live observations in the memory store (memory_items table).
    // The memory store uses the same SQLite pool as the engine DB when the
    // dashboard runs in-process; we query the table directly via the main pool.
    let cohort_count: i64 = if table_exists(pool, "memory_items").await? {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_items \
             WHERE tier = 'observation' AND forgotten_at IS NULL",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        0
    };

    // compiled_pattern_count: rows in agent_slot_optimizations with a
    // non-null gate_verdict (i.e. rows that completed the gate step).
    let compiled_pattern_count: i64 = if table_exists(pool, "agent_slot_optimizations").await? {
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_slot_optimizations WHERE gate_verdict IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?
    } else {
        0
    };

    // latest_optimization_run_id: most recently created optimizer session.
    let latest_optimization_run_id: Option<String> =
        if table_exists(pool, "autooptimizer_session_state").await? {
            sqlx::query_scalar(
                "SELECT session_id FROM autooptimizer_session_state \
                 ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?
        } else {
            None
        };

    // last_prompt_compile: most recent gate-completed row from
    // agent_slot_optimizations, ordered by gated_at DESC.
    let last_prompt_compile: Option<LastPromptCompile> =
        if table_exists(pool, "agent_slot_optimizations").await? {
            use sqlx::Row;
            let row = sqlx::query(
                "SELECT dev_metric, parent_dev_score, child_dev_score, delta_dev, \
                 parent_holdout_score, child_holdout_score, delta_holdout, \
                 gate_verdict, gated_at \
                 FROM agent_slot_optimizations \
                 WHERE gate_verdict IS NOT NULL \
                 ORDER BY gated_at DESC \
                 LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?;

            match row {
                None => None,
                Some(r) => {
                    let dev_metric: Option<String> = r
                        .try_get("dev_metric")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let parent_dev_score: Option<f64> = r
                        .try_get("parent_dev_score")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let child_dev_score: Option<f64> = r
                        .try_get("child_dev_score")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let delta_dev: Option<f64> = r
                        .try_get("delta_dev")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let parent_holdout_score: Option<f64> = r
                        .try_get("parent_holdout_score")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let child_holdout_score: Option<f64> = r
                        .try_get("child_holdout_score")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let delta_holdout: Option<f64> = r
                        .try_get("delta_holdout")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let gate_verdict: String = r
                        .try_get("gate_verdict")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    let gated_at: String = r
                        .try_get("gated_at")
                        .map_err(|e| DashboardError::Internal(e.into()))?;
                    Some(LastPromptCompile {
                        dev_metric,
                        parent_dev_score,
                        child_dev_score,
                        delta_dev,
                        parent_holdout_score,
                        child_holdout_score,
                        delta_holdout,
                        gate_verdict,
                        gated_at,
                    })
                }
            }
        } else {
            None
        };

    Ok(Json(FlywheelResponse::Enabled {
        enabled: true,
        cohort_count,
        threshold: cfg.dspy_pattern_cohort_threshold,
        compiled_pattern_count,
        latest_optimization_run_id,
        last_prompt_compile,
    }))
}

/// Load the autooptimizer config for the flywheel endpoint. Returns the default
/// when the config file is absent.
fn load_autooptimizer_config_for_flywheel(
) -> Result<xvision_engine::autooptimizer::config::AutoOptimizerConfig, DashboardError> {
    use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
    let path = AutoOptimizerConfig::default_path()?;
    if path.exists() {
        AutoOptimizerConfig::load(&path).map_err(DashboardError::Internal)
    } else {
        Ok(AutoOptimizerConfig::default())
    }
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/stats  (P3-W1)
//
// Per-cycle aggregate statistics for the optimizer UI. Each row represents
// one optimizer cycle with:
//   - kept / suspect / dropped counts (derived from lineage_nodes status)
//   - cost_usd  (from cycle_cost, nullable)
//   - cum_cost_usd  (monotonically accumulating running sum in ts order)
//   - session_id  (from autooptimizer_events join; nullable for CLI cycles)
//   - ts  (MIN created_at of the cycle's lineage nodes)
//   - best_delta_holdout  (MAX delta_holdout from gate records for this cycle)
//
// Optional filters:
//   ?strategy_id=  — cycles from sessions targeting that strategy
//   ?session_id=   — cycles from a specific session
//   ?since=        — ISO-8601 lower bound on ts
// ---------------------------------------------------------------------------

/// One row returned by `GET /api/autooptimizer/stats`.
#[derive(Debug, Clone, Serialize)]
pub struct StatsRow {
    pub cycle_id: String,
    /// Session that owns this cycle; null for CLI-launched cycles without a
    /// session record.
    pub session_id: Option<String>,
    /// RFC-3339 timestamp of the first lineage node in the cycle (MIN created_at).
    pub ts: String,
    /// Lineage nodes with Active status (gate passed / kept).
    pub kept: i64,
    /// Lineage nodes with Quarantined status (Suspect — partial pass).
    pub suspect: i64,
    /// Lineage nodes with Rejected status (gate failed / dropped).
    pub dropped: i64,
    /// Maximum gate holdout delta across this cycle's nodes. Null when the
    /// cycle has no autooptimizer_gate_records rows.
    pub best_delta_holdout: Option<f64>,
    /// Best (max) candidate edge over the random baseline across this cycle's
    /// gate records — `child_score - random_baseline_score`. Null pre-061.
    pub best_edge_over_random: Option<f64>,
    /// Best (max) parent edge over the random baseline this cycle —
    /// `parent_score - random_baseline_score`. Tracks lineage health across
    /// generations (drifting toward 0 = decaying toward noise). Null pre-061.
    pub best_parent_edge: Option<f64>,
    /// Per-cycle realized cost in USD (null when not metered).
    pub cost_usd: Option<f64>,
    /// Monotonically accumulating running sum of cost_usd ordered by ts.
    /// Null until the first metered cycle in the result.
    pub cum_cost_usd: Option<f64>,
}

/// Query parameters for `GET /api/autooptimizer/stats`.
#[derive(Deserialize, Default)]
pub struct StatsQuery {
    /// Filter to cycles belonging to a specific strategy (via session bridge).
    pub strategy_id: Option<String>,
    /// ISO-8601 lower bound on `ts` (the cycle's first lineage node timestamp).
    pub since: Option<String>,
    /// Filter to cycles associated with a specific optimizer session.
    pub session_id: Option<String>,
}

/// GET /api/autooptimizer/stats
///
/// Returns per-cycle aggregates (kept/suspect/dropped/cost/cum_cost) ordered
/// by ts ascending. Supports optional filters: ?strategy_id, ?session_id,
/// ?since (ISO-8601).
pub async fn get_stats(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Vec<StatsRow>>, DashboardError> {
    let pool = &state.pool;

    // Fast-path: no lineage schema means no stats.
    if !table_exists(pool, "lineage_nodes").await? {
        return Ok(Json(Vec::new()));
    }

    // Validate the `since` filter up-front.
    let since_dt: Option<DateTime<Utc>> = match &q.since {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(s)
                .map_err(|e| DashboardError::Validation {
                    field: "since".into(),
                    msg: format!("invalid RFC-3339 timestamp: {e}"),
                })?
                .with_timezone(&Utc),
        ),
        None => None,
    };

    // When session / strategy filters are requested, resolve cycle_ids via the
    // autooptimizer_events bridge. When those tables are absent the result is
    // empty (no sessions → no matching cycles).
    let filtered_cycle_ids: Option<Vec<String>> = if q.strategy_id.is_some() || q.session_id.is_some() {
        if !table_exists(pool, "autooptimizer_session_state").await?
            || !table_exists(pool, "autooptimizer_events").await?
        {
            return Ok(Json(Vec::new()));
        }
        let ids = load_filtered_cycle_ids_stats(pool, &q).await?;
        if ids.is_empty() {
            return Ok(Json(Vec::new()));
        }
        Some(ids)
    } else {
        None
    };

    let rows = load_stats_rows(pool, since_dt, filtered_cycle_ids).await?;
    Ok(Json(rows))
}

/// Resolve cycle_ids matching the session / strategy filters via the events
/// bridge (`autooptimizer_events` links session_id → cycle_id).
async fn load_filtered_cycle_ids_stats(
    pool: &sqlx::SqlitePool,
    q: &StatsQuery,
) -> Result<Vec<String>, DashboardError> {
    use sqlx::Row;

    let rows = match (&q.session_id, &q.strategy_id) {
        (Some(sid), Some(strat)) => sqlx::query(
            "SELECT DISTINCT ae.cycle_id \
             FROM autooptimizer_events ae \
             JOIN autooptimizer_session_state ss ON ss.session_id = ae.session_id \
             WHERE ae.session_id = ? AND ss.strategy_id = ? AND ae.cycle_id IS NOT NULL",
        )
        .bind(sid)
        .bind(strat)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?,

        (Some(sid), None) => sqlx::query(
            "SELECT DISTINCT cycle_id FROM autooptimizer_events \
             WHERE session_id = ? AND cycle_id IS NOT NULL",
        )
        .bind(sid)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?,

        (None, Some(strat)) => sqlx::query(
            "SELECT DISTINCT ae.cycle_id \
             FROM autooptimizer_events ae \
             JOIN autooptimizer_session_state ss ON ss.session_id = ae.session_id \
             WHERE ss.strategy_id = ? AND ae.cycle_id IS NOT NULL",
        )
        .bind(strat)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?,

        (None, None) => return Ok(Vec::new()),
    };

    rows.into_iter()
        .map(|row| {
            row.try_get::<String, _>("cycle_id")
                .map_err(|e| DashboardError::Internal(e.into()))
        })
        .collect()
}

/// Build per-cycle aggregate rows and compute cumulative cost in Rust
/// (avoids a window-function dependency on SQLite 3.25+).
async fn load_stats_rows(
    pool: &sqlx::SqlitePool,
    since_dt: Option<DateTime<Utc>>,
    filtered_cycle_ids: Option<Vec<String>>,
) -> Result<Vec<StatsRow>, DashboardError> {
    use sqlx::Row;

    let since_str = since_dt.map(|dt| dt.to_rfc3339());

    // Core aggregation: group lineage_nodes by cycle_id.
    // LEFT JOINs supply cost (cycle_cost), session link (autooptimizer_events),
    // and best holdout delta (autooptimizer_gate_records).
    // The tables are all optional — LEFT JOIN gracefully yields NULLs when
    // absent or empty, so the query works even before cost/gate tables exist.
    let base = "\
        SELECT ln.cycle_id, \
               SUM(CASE WHEN ln.status = 'active'      THEN 1 ELSE 0 END) AS kept, \
               SUM(CASE WHEN ln.status = 'quarantined' THEN 1 ELSE 0 END) AS suspect, \
               SUM(CASE WHEN ln.status = 'rejected'    THEN 1 ELSE 0 END) AS dropped, \
               MIN(ln.created_at) AS ts, \
               MAX(agr.delta_holdout) AS best_delta_holdout, \
               MAX(agr.edge_over_random) AS best_edge_over_random, \
               MAX(agr.parent_edge) AS best_parent_edge, \
               cc.cost_usd AS cost_usd, \
               MIN(ae.session_id) AS session_id \
        FROM lineage_nodes ln \
        LEFT JOIN cycle_cost cc ON cc.cycle_id = ln.cycle_id \
        LEFT JOIN autooptimizer_gate_records agr ON agr.bundle_hash = ln.bundle_hash \
        LEFT JOIN autooptimizer_events ae ON ae.cycle_id = ln.cycle_id";

    let rows: Vec<sqlx::sqlite::SqliteRow> = match (&since_str, &filtered_cycle_ids) {
        (Some(since), Some(ids)) => {
            let ph = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "{base} WHERE ln.cycle_id IS NOT NULL AND ln.cycle_id IN ({ph}) \
                 AND ln.created_at >= ? GROUP BY ln.cycle_id ORDER BY ts ASC"
            );
            let mut q = sqlx::query(&sql);
            for id in ids {
                q = q.bind(id);
            }
            q.bind(since)
                .fetch_all(pool)
                .await
                .map_err(|e| DashboardError::Internal(e.into()))?
        }
        (None, Some(ids)) => {
            let ph = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "{base} WHERE ln.cycle_id IS NOT NULL AND ln.cycle_id IN ({ph}) \
                 GROUP BY ln.cycle_id ORDER BY ts ASC"
            );
            let mut q = sqlx::query(&sql);
            for id in ids {
                q = q.bind(id);
            }
            q.fetch_all(pool)
                .await
                .map_err(|e| DashboardError::Internal(e.into()))?
        }
        (Some(since), None) => sqlx::query(&format!(
            "{base} WHERE ln.cycle_id IS NOT NULL AND ln.created_at >= ? \
             GROUP BY ln.cycle_id ORDER BY ts ASC"
        ))
        .bind(since)
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?,

        (None, None) => sqlx::query(&format!(
            "{base} WHERE ln.cycle_id IS NOT NULL \
             GROUP BY ln.cycle_id ORDER BY ts ASC"
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?,
    };

    // Map rows and accumulate cumulative cost.
    let mut cum: f64 = 0.0;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let cycle_id: String = row
            .try_get("cycle_id")
            .map_err(|e| DashboardError::Internal(e.into()))?;
        let kept: i64 = row
            .try_get("kept")
            .map_err(|e| DashboardError::Internal(e.into()))?;
        let suspect: i64 = row
            .try_get("suspect")
            .map_err(|e| DashboardError::Internal(e.into()))?;
        let dropped: i64 = row
            .try_get("dropped")
            .map_err(|e| DashboardError::Internal(e.into()))?;
        let ts: String = row
            .try_get("ts")
            .map_err(|e| DashboardError::Internal(e.into()))?;
        let best_delta_holdout: Option<f64> = row.try_get("best_delta_holdout").ok().flatten();
        let best_edge_over_random: Option<f64> = row.try_get("best_edge_over_random").ok().flatten();
        let best_parent_edge: Option<f64> = row.try_get("best_parent_edge").ok().flatten();
        let cost_usd: Option<f64> = row.try_get("cost_usd").ok().flatten();
        let session_id: Option<String> = row.try_get("session_id").ok().flatten();

        let cum_cost_usd = if let Some(c) = cost_usd {
            cum += c;
            Some(cum)
        } else if cum > 0.0 {
            Some(cum)
        } else {
            None
        };

        result.push(StatsRow {
            cycle_id,
            session_id,
            ts,
            kept,
            suspect,
            dropped,
            best_delta_holdout,
            best_edge_over_random,
            best_parent_edge,
            cost_usd,
            cum_cost_usd,
        });
    }

    Ok(result)
}

pub(super) async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> Result<bool, DashboardError> {
    use sqlx::Row;
    let found: Option<String> =
        sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1")
            .bind(table)
            .fetch_optional(pool)
            .await
            .map_err(|e| DashboardError::Internal(e.into()))?
            .map(|row| row.try_get("name"))
            .transpose()
            .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(found.is_some())
}

fn row_to_lineage_node(row: sqlx::sqlite::SqliteRow) -> Result<LineageNode, DashboardError> {
    use sqlx::Row;
    use xvision_engine::autooptimizer::{gate::GateVerdict, ContentHash};

    let bundle_hex: String = row
        .try_get("bundle_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let parent_hex: Option<String> = row
        .try_get("parent_hash")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let gate_str: String = row
        .try_get("gate_verdict")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let status_str: String = row
        .try_get("status")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let cycle_id: Option<String> = row
        .try_get("cycle_id")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let created_str: String = row
        .try_get("created_at")
        .map_err(|e| DashboardError::Internal(e.into()))?;
    let diversity_score: Option<f64> = row
        .try_get("diversity_score")
        .map_err(|e| DashboardError::Internal(e.into()))?;

    let bundle_hash = ContentHash::from_hex(&bundle_hex).map_err(|e| DashboardError::Internal(e))?;
    let parent_hash = parent_hex
        .map(|h| ContentHash::from_hex(&h))
        .transpose()
        .map_err(|e| DashboardError::Internal(e))?;
    let gate_verdict = GateVerdict::from_str(&gate_str).map_err(|e| DashboardError::Internal(e))?;
    let status = match status_str.as_str() {
        "active" => LineageStatus::Active,
        "quarantined" => LineageStatus::Quarantined,
        "rejected" => LineageStatus::Rejected,
        other => {
            return Err(DashboardError::Internal(anyhow::anyhow!(
                "unknown lineage status: {other}"
            )))
        }
    };
    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("parse created_at: {e}")))?
        .with_timezone(&Utc);

    Ok(LineageNode {
        bundle_hash,
        parent_hash,
        gate_verdict,
        status,
        cycle_id,
        created_at,
        diversity_score,
    })
}

/// One row returned by `GET /api/autooptimizer/river`.
#[derive(Serialize, sqlx::FromRow)]
pub struct RiverNode {
    pub bundle_hash: String,
    pub parent_hash: Option<String>,
    pub cycle_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub child_day_score: Option<f64>,
    pub delta_day: Option<f64>,
}

/// GET /api/autooptimizer/river
///
/// Feed for the lineage-river chart: every lineage node with its gate scores
/// joined in, oldest-first so the frontend can build generations in order.
///
/// Recorded spec deviation: spec §7 allows "a possible events-by-cycle read
/// endpoint" (Task 1). This second read endpoint is required data-plumbing
/// for the §3a river (Y = Sharpe per node) and adds no new computation, but
/// it exceeds §7's literal single-endpoint allowance. Surfaced in PR for
/// operator sign-off. Implementation is read-only LEFT JOIN; spec §8.2.
pub async fn get_river(State(state): State<AppState>) -> Result<Json<Vec<RiverNode>>, DashboardError> {
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Ok(Json(Vec::new()));
    }
    let has_gates = table_exists(&state.pool, "autooptimizer_gate_records").await?;
    let sql = if has_gates {
        "SELECT n.bundle_hash, n.parent_hash, n.cycle_id, n.status, n.created_at,
                g.child_day_score, g.delta_day
         FROM lineage_nodes n
         LEFT JOIN autooptimizer_gate_records g ON g.bundle_hash = n.bundle_hash
         ORDER BY n.created_at ASC LIMIT 2000"
    } else {
        "SELECT bundle_hash, parent_hash, cycle_id, status, created_at,
                NULL AS child_day_score, NULL AS delta_day
         FROM lineage_nodes ORDER BY created_at ASC LIMIT 2000"
    };
    let nodes: Vec<RiverNode> = sqlx::query_as(sql)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(Json(nodes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::autooptimizer::{
        content_hash::ContentHash,
        evidence::{ensure_evidence_schema, persist_finding, persist_gate_record, GateRecord},
        judge::{Finding, FindingSeverity},
        lineage::ensure_lineage_schema,
    };

    /// Helper: open an in-memory pool with lineage + evidence schemas.
    async fn open_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool");
        ensure_lineage_schema(&pool).await.expect("ensure_lineage_schema");
        ensure_evidence_schema(&pool)
            .await
            .expect("ensure_evidence_schema");
        pool
    }

    /// Insert a lineage node for use as the base of detail-endpoint tests.
    async fn insert_lineage_node(pool: &sqlx::SqlitePool, bundle_hash: &str) {
        sqlx::query(
            "INSERT INTO lineage_nodes \
             (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
             VALUES (?, NULL, 'pass', 'active', 'cycle-001', '2026-01-01T00:00:00Z')",
        )
        .bind(bundle_hash)
        .execute(pool)
        .await
        .expect("insert lineage_node");
    }

    // ─── test_findings_endpoint_returns_data ─────────────────────────────────

    /// After inserting a finding directly via `persist_finding`, `load_findings`
    /// returns it — proving the GET /findings/:hash endpoint would no longer
    /// return []. (We test the DB layer directly since axum router setup is heavy.)
    #[tokio::test]
    async fn test_findings_endpoint_returns_data() {
        let pool = open_pool().await;
        let hash = ContentHash::of_bytes(b"test-findings-endpoint").to_hex();

        // Insert a finding for this hash.
        persist_finding(
            &pool,
            &hash,
            &Finding {
                code: "test_code".into(),
                severity: FindingSeverity::Warn,
                summary: "Test summary".into(),
                detail: Some("Test detail".into()),
            },
            Some("openai/gpt-4"),
        )
        .await
        .expect("persist_finding");

        // Verify via the load helper (the same one get_findings calls).
        let rows = load_findings(&pool, &hash).await.expect("load_findings");
        assert_eq!(
            rows.len(),
            1,
            "GET /findings/:hash must return 1 row after persist"
        );
        assert_eq!(rows[0].code, "test_code");
        assert_eq!(rows[0].severity, "warn");
        assert_eq!(rows[0].summary, "Test summary");
        assert_eq!(rows[0].model.as_deref(), Some("openai/gpt-4"));
    }

    // ─── test_experiments_detail_returns_5_fields ────────────────────────────

    /// GET /experiments/:hash/detail: after seeding lineage_node + gate_record +
    /// finding + regime_results, `get_experiment_detail` returns all 5 fields.
    #[tokio::test]
    async fn test_experiments_detail_returns_5_fields() {
        use xvision_engine::autooptimizer::{
            config::RegimeSide,
            regime_results::{insert_regime_results_standalone, RegimeResultRow},
        };
        use xvision_engine::eval::run::MetricsSummary;

        let pool = open_pool().await;
        let hash = ContentHash::of_bytes(b"test-experiment-detail").to_hex();

        // Seed lineage node.
        insert_lineage_node(&pool, &hash).await;

        // Seed gate record with rationale.
        persist_gate_record(
            &pool,
            GateRecord {
                bundle_hash: &hash,
                parent_day_score: Some(1.0),
                child_day_score: Some(1.3),
                parent_holdout_score: Some(0.8),
                child_holdout_score: Some(1.0),
                gate_epsilon: Some(0.05),
                delta_day: Some(0.3),
                delta_holdout: Some(0.2),
                drawdown_ratio: Some(1.1),
                verdict: "passed",
                reason: None,
                rationale: Some("Raised stop-loss multiplier from 1.5 to 2.0"),
                edge_over_random: Some(0.5),
                parent_edge: Some(0.2),
                edge_delta: Some(0.3),
            },
        )
        .await
        .expect("persist_gate_record");

        // Seed a finding.
        persist_finding(
            &pool,
            &hash,
            &Finding {
                code: "param_change".into(),
                severity: FindingSeverity::Info,
                summary: "Stop-loss tightened".into(),
                detail: None,
            },
            None,
        )
        .await
        .expect("persist_finding");

        // Seed a regime result.
        insert_regime_results_standalone(
            &pool,
            &hash,
            &[RegimeResultRow {
                regime_label: "bull_2024".to_string(),
                side: RegimeSide::Bull,
                metrics_day: MetricsSummary {
                    sharpe: 1.3,
                    ..Default::default()
                },
                metrics_untouched: MetricsSummary {
                    sharpe: 1.0,
                    ..Default::default()
                },
                delta_sharpe: 0.3,
                verdict: "pass".to_string(),
            }],
            "2026-01-01T00:00:00Z",
        )
        .await
        .expect("insert_regime_results_standalone");

        // Exercise load_gate_record + load_findings + load_regime_results
        // (same queries the handler uses).
        let gate_record = load_gate_record(&pool, &hash)
            .await
            .expect("load_gate_record")
            .expect("gate_record must exist");
        assert_eq!(
            gate_record.rationale.as_deref(),
            Some("Raised stop-loss multiplier from 1.5 to 2.0"),
            "field 2: rationale"
        );

        let findings = load_findings(&pool, &hash).await.expect("load_findings");
        assert_eq!(findings.len(), 1, "field 4: findings");

        let regime_rows = xvision_engine::autooptimizer::regime_results::load_regime_results(&pool, &hash)
            .await
            .expect("load_regime_results");
        assert_eq!(regime_rows.len(), 1, "field 5: regime_results");

        // Verify lineage node is retrievable.
        let store = LineageStore::new(pool.clone());
        let ch = ContentHash::from_hex(&hash).unwrap();
        let node = store.get(&ch).await.expect("store.get").expect("node must exist");
        assert_eq!(node.bundle_hash.to_hex(), hash, "field 1: lineage_node");
    }

    // ─── test_experiments_detail_404 ─────────────────────────────────────────

    /// `load_gate_record` and `load_findings` return None/empty for a hash that
    /// doesn't exist in lineage_nodes — proving the 404 branch works.
    #[tokio::test]
    async fn test_experiments_detail_404() {
        let pool = open_pool().await;
        let nonexistent = ContentHash::of_bytes(b"nonexistent-experiment").to_hex();

        // Lineage store returns None for an unknown hash.
        let store = LineageStore::new(pool.clone());
        let ch = ContentHash::from_hex(&nonexistent).unwrap();
        let node = store.get(&ch).await.expect("store.get should not err");
        assert!(node.is_none(), "node for nonexistent hash must be None");

        // Gate record also None.
        let gate = load_gate_record(&pool, &nonexistent)
            .await
            .expect("load_gate_record");
        assert!(gate.is_none());

        // Findings empty.
        let findings = load_findings(&pool, &nonexistent).await.expect("load_findings");
        assert!(findings.is_empty());
    }

    // ─── original test (preserved) ───────────────────────────────────────────

    /// Verify that `row_to_lineage_node` accepts a persisted "quarantined" status
    /// (Fix 1: previously the match had no quarantined arm and returned Internal
    /// 500 for any quarantined node, breaking the lineage list endpoint).
    #[tokio::test]
    async fn quarantined_row_parsed_without_error() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool");
        ensure_lineage_schema(&pool).await.expect("ensure_lineage_schema");

        let hash = ContentHash::of_bytes(b"quarantined-test").to_hex();

        sqlx::query(
            "INSERT INTO lineage_nodes \
             (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
             VALUES (?, NULL, 'pass', 'quarantined', 'cycle-q-001', '2026-01-01T00:00:00Z')",
        )
        .bind(&hash)
        .execute(&pool)
        .await
        .expect("insert quarantined node");

        let rows = sqlx::query(
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, \
             created_at, diversity_score FROM lineage_nodes WHERE status = 'quarantined'",
        )
        .fetch_all(&pool)
        .await
        .expect("fetch quarantined rows");

        assert_eq!(rows.len(), 1);
        let node = row_to_lineage_node(rows.into_iter().next().unwrap())
            .expect("row_to_lineage_node must not error on quarantined status");
        assert_eq!(node.status, LineageStatus::Quarantined);
    }

    // ─── Flywheel endpoint tests (P3-W2) ─────────────────────────────────────

    /// Helper: open a bare in-memory pool (no lineage/evidence schema) and
    /// create the agent_slot_optimizations table with all 054 columns.
    async fn open_flywheel_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool");

        // Migration 051: base agent_slot_optimizations table.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS agent_slot_optimizations (
                optimization_id          TEXT PRIMARY KEY,
                target_agent_id          TEXT NOT NULL,
                child_agent_id           TEXT,
                slot                     TEXT NOT NULL,
                method                   TEXT NOT NULL,
                demo_source              TEXT NOT NULL,
                reproducible             INTEGER NOT NULL,
                holdout_split            TEXT NOT NULL,
                cohort_query             TEXT NOT NULL,
                train_observation_ids_json   TEXT NOT NULL,
                dev_observation_ids_json     TEXT NOT NULL,
                holdout_observation_ids_json TEXT NOT NULL,
                train_hash               TEXT NOT NULL,
                dev_hash                 TEXT NOT NULL,
                holdout_hash             TEXT NOT NULL,
                prompt_prefix_chars      INTEGER NOT NULL,
                status                   TEXT NOT NULL,
                created_at               TEXT NOT NULL,
                -- Migration 054 gate columns
                dev_metric               TEXT,
                holdout_metric           TEXT,
                parent_dev_score         REAL,
                child_dev_score          REAL,
                parent_holdout_score     REAL,
                child_holdout_score      REAL,
                gate_epsilon             REAL,
                delta_dev                REAL,
                delta_holdout            REAL,
                gate_verdict             TEXT,
                gate_reason              TEXT,
                gated_at                 TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("create agent_slot_optimizations");

        // Session state table (migration 057).
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_session_state (
                session_id  TEXT PRIMARY KEY,
                strategy_id TEXT NOT NULL,
                state       TEXT NOT NULL,
                mode        TEXT NOT NULL,
                cycles_planned   INTEGER,
                cycles_completed INTEGER NOT NULL DEFAULT 0,
                kept_count       INTEGER NOT NULL DEFAULT 0,
                suspect_count    INTEGER NOT NULL DEFAULT 0,
                dropped_count    INTEGER NOT NULL DEFAULT 0,
                errored_count    INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create autooptimizer_session_state");

        pool
    }

    // ─── test_flywheel_disabled ───────────────────────────────────────────────

    /// When dspy_enabled=false, the flywheel query logic is exercised via the
    /// helper functions directly (config gating checked via the public function
    /// tested at the integration level). We verify that `load_autooptimizer_config_for_flywheel`
    /// returns a config with dspy_enabled=false by default (no config file on disk).
    #[tokio::test]
    async fn test_flywheel_disabled_when_no_config_file() {
        use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
        // When no config file exists, default() is used. Default has dspy_enabled=false.
        let cfg = AutoOptimizerConfig::default();
        assert!(
            !cfg.dspy_enabled,
            "AutoOptimizerConfig::default must have dspy_enabled=false"
        );
    }

    // ─── test_flywheel_enabled_no_data ────────────────────────────────────────

    /// When dspy is enabled but no rows exist, the endpoint should return
    /// cohort_count=0, compiled_pattern_count=0, latest_optimization_run_id=None,
    /// last_prompt_compile=None.
    #[tokio::test]
    async fn test_flywheel_enabled_no_data() {
        let pool = open_flywheel_pool().await;

        // Verify cohort_count: table exists (via memory_items absence) →
        // since our test pool has no memory_items table, cohort_count = 0.
        let has_memory_items = table_exists(&pool, "memory_items").await.unwrap();
        assert!(!has_memory_items, "no memory_items in test pool");

        // compiled_pattern_count: table exists but no rows → 0.
        let has_aso = table_exists(&pool, "agent_slot_optimizations").await.unwrap();
        assert!(has_aso, "agent_slot_optimizations must exist");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_slot_optimizations WHERE gate_verdict IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(count, 0, "no rows → compiled_pattern_count = 0");

        // latest_optimization_run_id: table exists but empty → None.
        let run_id: Option<String> = sqlx::query_scalar(
            "SELECT session_id FROM autooptimizer_session_state ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query_optional");
        assert!(run_id.is_none(), "empty sessions table → None");

        // last_prompt_compile: no rows → None.
        let compile_row = sqlx::query(
            "SELECT dev_metric, parent_dev_score, child_dev_score, delta_dev, \
             parent_holdout_score, child_holdout_score, delta_holdout, \
             gate_verdict, gated_at \
             FROM agent_slot_optimizations \
             WHERE gate_verdict IS NOT NULL \
             ORDER BY gated_at DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query_optional");
        assert!(
            compile_row.is_none(),
            "no gated rows → last_prompt_compile = None"
        );
    }

    // ─── test_flywheel_with_compile_data ─────────────────────────────────────

    /// After inserting a row into agent_slot_optimizations with gate_verdict set,
    /// the last_prompt_compile fields should match.
    #[tokio::test]
    async fn test_flywheel_with_compile_data() {
        use sqlx::Row;
        let pool = open_flywheel_pool().await;

        // Insert a gated optimization row.
        sqlx::query(
            "INSERT INTO agent_slot_optimizations \
             (optimization_id, target_agent_id, slot, method, demo_source, reproducible, \
              holdout_split, cohort_query, train_observation_ids_json, dev_observation_ids_json, \
              holdout_observation_ids_json, train_hash, dev_hash, holdout_hash, \
              prompt_prefix_chars, status, created_at, \
              dev_metric, parent_dev_score, child_dev_score, delta_dev, \
              parent_holdout_score, child_holdout_score, delta_holdout, \
              gate_verdict, gated_at) \
             VALUES (?, ?, 'trader', 'bootstrap', 'observations', 1, \
              '0.2', 'q', '[]', '[]', '[]', 'h1', 'h2', 'h3', 0, 'gated', \
              '2026-06-07T10:00:00Z', \
              'score_delta', 0.68, 0.72, 0.04, \
              0.65, 0.69, 0.04, \
              'kept', '2026-06-07T12:00:00Z')",
        )
        .bind("opt-001")
        .bind("agent-abc")
        .execute(&pool)
        .await
        .expect("insert optimization row");

        // Insert a session for latest_optimization_run_id.
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, state, mode, cycles_completed, \
              kept_count, suspect_count, dropped_count, created_at, updated_at) \
             VALUES (?, 'strat-1', 'completed', 'optimize', 0, 0, 0, 0, \
             '2026-06-07T09:00:00Z', '2026-06-07T09:00:00Z')",
        )
        .bind("01HXXX123")
        .execute(&pool)
        .await
        .expect("insert session");

        // Verify compiled_pattern_count = 1.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_slot_optimizations WHERE gate_verdict IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(count, 1, "one gated row → compiled_pattern_count = 1");

        // Verify latest_optimization_run_id.
        let run_id: Option<String> = sqlx::query_scalar(
            "SELECT session_id FROM autooptimizer_session_state ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query_optional");
        assert_eq!(run_id.as_deref(), Some("01HXXX123"));

        // Verify last_prompt_compile fields.
        let row = sqlx::query(
            "SELECT dev_metric, parent_dev_score, child_dev_score, delta_dev, \
             parent_holdout_score, child_holdout_score, delta_holdout, \
             gate_verdict, gated_at \
             FROM agent_slot_optimizations \
             WHERE gate_verdict IS NOT NULL \
             ORDER BY gated_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("fetch row");

        let dev_metric: Option<String> = row.try_get("dev_metric").unwrap();
        let parent_dev_score: Option<f64> = row.try_get("parent_dev_score").unwrap();
        let child_dev_score: Option<f64> = row.try_get("child_dev_score").unwrap();
        let delta_dev: Option<f64> = row.try_get("delta_dev").unwrap();
        let parent_holdout_score: Option<f64> = row.try_get("parent_holdout_score").unwrap();
        let child_holdout_score: Option<f64> = row.try_get("child_holdout_score").unwrap();
        let delta_holdout: Option<f64> = row.try_get("delta_holdout").unwrap();
        let gate_verdict: String = row.try_get("gate_verdict").unwrap();
        let gated_at: String = row.try_get("gated_at").unwrap();

        assert_eq!(dev_metric.as_deref(), Some("score_delta"));
        assert!((parent_dev_score.unwrap() - 0.68).abs() < 1e-9);
        assert!((child_dev_score.unwrap() - 0.72).abs() < 1e-9);
        assert!((delta_dev.unwrap() - 0.04).abs() < 1e-9);
        assert!((parent_holdout_score.unwrap() - 0.65).abs() < 1e-9);
        assert!((child_holdout_score.unwrap() - 0.69).abs() < 1e-9);
        assert!((delta_holdout.unwrap() - 0.04).abs() < 1e-9);
        assert_eq!(gate_verdict, "kept");
        assert_eq!(gated_at, "2026-06-07T12:00:00Z");
    }

    // =========================================================================
    // Stats endpoint tests (P3-W1)
    // =========================================================================

    /// Helper: open an in-memory pool with the full lineage schema, evidence
    /// schema (creates autooptimizer_gate_records), autooptimizer_events, and
    /// autooptimizer_session_state tables.
    async fn open_stats_pool() -> sqlx::SqlitePool {
        use xvision_engine::autooptimizer::evidence::ensure_evidence_schema;
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool");
        ensure_lineage_schema(&pool).await.expect("ensure_lineage_schema");
        ensure_evidence_schema(&pool)
            .await
            .expect("ensure_evidence_schema");
        // Session state table (migration 057 subset).
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_session_state (
                session_id   TEXT PRIMARY KEY,
                strategy_id  TEXT NOT NULL,
                config_json  TEXT NOT NULL DEFAULT '{}',
                state        TEXT NOT NULL DEFAULT 'running',
                mode         TEXT NOT NULL DEFAULT 'once',
                cycles_planned   INTEGER,
                cycles_completed INTEGER NOT NULL DEFAULT 0,
                kept_count       INTEGER NOT NULL DEFAULT 0,
                suspect_count    INTEGER NOT NULL DEFAULT 0,
                dropped_count    INTEGER NOT NULL DEFAULT 0,
                errored_count    INTEGER NOT NULL DEFAULT 0,
                created_at   TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create autooptimizer_session_state");
        // Events table (migration 057 subset).
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_events (
                seq         INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                cycle_id    TEXT,
                kind        TEXT NOT NULL DEFAULT 'cycle_started',
                payload_json TEXT NOT NULL DEFAULT '{}',
                ts          TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create autooptimizer_events");
        pool
    }

    /// Insert N lineage nodes for the given cycle_id with the given statuses.
    async fn seed_cycle_nodes(
        pool: &sqlx::SqlitePool,
        cycle_id: &str,
        statuses: &[(&str, &str)], // (bundle_hash_seed, status)
        created_at: &str,
    ) {
        for (seed, status) in statuses {
            let hash = ContentHash::of_bytes(seed.as_bytes()).to_hex();
            sqlx::query(
                "INSERT OR IGNORE INTO lineage_nodes \
                 (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
                 VALUES (?, NULL, 'pass', ?, ?, ?)",
            )
            .bind(&hash)
            .bind(status)
            .bind(cycle_id)
            .bind(created_at)
            .execute(pool)
            .await
            .expect("insert lineage_node");
        }
    }

    /// Insert a cycle_cost row.
    async fn seed_cycle_cost(pool: &sqlx::SqlitePool, cycle_id: &str, cost_usd: f64, created_at: &str) {
        sqlx::query(
            "INSERT OR REPLACE INTO cycle_cost \
             (cycle_id, input_tokens, output_tokens, cost_usd, unpriced_calls, created_at) \
             VALUES (?, 100, 50, ?, 0, ?)",
        )
        .bind(cycle_id)
        .bind(cost_usd)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("insert cycle_cost");
    }

    // ─── test_stats_returns_per_cycle_aggregates ──────────────────────────────

    /// After inserting lineage nodes + cycle_cost rows for a single cycle,
    /// load_stats_rows should return the correct kept/suspect/dropped counts
    /// and cost.
    #[tokio::test]
    async fn test_stats_returns_per_cycle_aggregates() {
        let pool = open_stats_pool().await;

        // Cycle: 1 kept, 1 suspect, 2 dropped, cost = $0.12
        seed_cycle_nodes(
            &pool,
            "cycle-agg-01",
            &[
                ("agg-active", "active"),
                ("agg-quarantined", "quarantined"),
                ("agg-rejected-1", "rejected"),
                ("agg-rejected-2", "rejected"),
            ],
            "2026-06-01T00:00:00Z",
        )
        .await;
        seed_cycle_cost(&pool, "cycle-agg-01", 0.12, "2026-06-01T00:00:00Z").await;

        let rows = load_stats_rows(&pool, None, None).await.expect("load_stats_rows");
        assert_eq!(rows.len(), 1, "expected one cycle row");
        let r = &rows[0];
        assert_eq!(r.cycle_id, "cycle-agg-01");
        assert_eq!(r.kept, 1, "kept count");
        assert_eq!(r.suspect, 1, "suspect count");
        assert_eq!(r.dropped, 2, "dropped count");
        assert!((r.cost_usd.unwrap() - 0.12).abs() < 1e-9, "cost_usd");
        assert!((r.cum_cost_usd.unwrap() - 0.12).abs() < 1e-9, "cum_cost_usd");
    }

    // ─── test_stats_cum_cost_accumulates ─────────────────────────────────────

    /// Three cycles with cost 0.10, 0.20, 0.15 — cum_cost_usd must be
    /// 0.10, 0.30, 0.45 in ts order.
    #[tokio::test]
    async fn test_stats_cum_cost_accumulates() {
        let pool = open_stats_pool().await;

        let cycles = [
            ("cycle-cum-01", 0.10_f64, "2026-06-01T00:00:00Z"),
            ("cycle-cum-02", 0.20_f64, "2026-06-02T00:00:00Z"),
            ("cycle-cum-03", 0.15_f64, "2026-06-03T00:00:00Z"),
        ];

        for (cid, cost, ts) in &cycles {
            seed_cycle_nodes(&pool, cid, &[(cid, "active")], ts).await;
            seed_cycle_cost(&pool, cid, *cost, ts).await;
        }

        let rows = load_stats_rows(&pool, None, None).await.expect("load_stats_rows");
        assert_eq!(rows.len(), 3, "expected three cycle rows");

        let expected_cum = [0.10_f64, 0.30_f64, 0.45_f64];
        let expected_cost = [0.10_f64, 0.20_f64, 0.15_f64];
        for (i, row) in rows.iter().enumerate() {
            assert!(
                (row.cost_usd.unwrap() - expected_cost[i]).abs() < 1e-9,
                "cycle {} cost_usd: expected {}, got {:?}",
                i,
                expected_cost[i],
                row.cost_usd
            );
            assert!(
                (row.cum_cost_usd.unwrap() - expected_cum[i]).abs() < 1e-9,
                "cycle {} cum_cost_usd: expected {}, got {:?}",
                i,
                expected_cum[i],
                row.cum_cost_usd
            );
        }
    }

    // ─── test_stats_strategy_id_filter ───────────────────────────────────────

    /// Two strategies A + B with one cycle each. ?strategy_id=A must return
    /// only A's cycle.
    #[tokio::test]
    async fn test_stats_strategy_id_filter() {
        let pool = open_stats_pool().await;

        // Strategy A → session-A → cycle-A
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sess-A', 'strat-A', '{}', 'running', 'once', '2026-06-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO autooptimizer_events \
             (session_id, cycle_id, kind, payload_json, ts) \
             VALUES ('sess-A', 'cycle-A', 'cycle_started', '{}', '2026-06-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_cycle_nodes(&pool, "cycle-A", &[("node-A", "active")], "2026-06-01T00:00:00Z").await;

        // Strategy B → session-B → cycle-B
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sess-B', 'strat-B', '{}', 'running', 'once', '2026-06-02T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO autooptimizer_events \
             (session_id, cycle_id, kind, payload_json, ts) \
             VALUES ('sess-B', 'cycle-B', 'cycle_started', '{}', '2026-06-02T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_cycle_nodes(
            &pool,
            "cycle-B",
            &[("node-B", "rejected")],
            "2026-06-02T00:00:00Z",
        )
        .await;

        let q = StatsQuery {
            strategy_id: Some("strat-A".to_string()),
            ..Default::default()
        };
        let ids = load_filtered_cycle_ids_stats(&pool, &q)
            .await
            .expect("load_filtered_cycle_ids_stats");
        assert_eq!(
            ids,
            vec!["cycle-A"],
            "strategy_id filter must return only A's cycle"
        );

        let rows = load_stats_rows(&pool, None, Some(ids))
            .await
            .expect("load_stats_rows with strategy filter");
        assert_eq!(rows.len(), 1, "only cycle-A must be in result");
        assert_eq!(rows[0].cycle_id, "cycle-A");
        assert_eq!(rows[0].kept, 1);
    }

    // ─── test_cycle_strategy_id_resolves_via_bridge (UI3) ─────────────────────

    /// `cycle_strategy_id` resolves the optimized strategy through the
    /// events → session bridge; a cycle with no session row yields `None`.
    #[tokio::test]
    async fn test_cycle_strategy_id_resolves_via_bridge() {
        let pool = open_stats_pool().await;

        // strat-A → sess-A → cycle-A (bridged).
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sess-A', 'strat-A', '{}', 'running', 'once', '2026-06-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO autooptimizer_events \
             (session_id, cycle_id, kind, payload_json, ts) \
             VALUES ('sess-A', 'cycle-A', 'cycle_started', '{}', '2026-06-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let resolved = cycle_strategy_id(&pool, "cycle-A").await.unwrap();
        assert_eq!(resolved.as_deref(), Some("strat-A"));

        // A CLI cycle with no session bridge row → None (renders as "—").
        let unbridged = cycle_strategy_id(&pool, "cycle-cli-only").await.unwrap();
        assert_eq!(unbridged, None);
    }

    // ─── test_stats_since_filter ──────────────────────────────────────────────

    /// Two cycles — one before and one after a `since` boundary. Only the
    /// newer cycle should appear in the result.
    #[tokio::test]
    async fn test_stats_since_filter() {
        let pool = open_stats_pool().await;

        // Old cycle: created before the boundary.
        seed_cycle_nodes(
            &pool,
            "cycle-old",
            &[("node-old", "active")],
            "2026-05-01T00:00:00Z",
        )
        .await;
        // New cycle: created after the boundary.
        seed_cycle_nodes(
            &pool,
            "cycle-new",
            &[("node-new", "rejected")],
            "2026-06-05T00:00:00Z",
        )
        .await;

        let since = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let rows = load_stats_rows(&pool, Some(since), None)
            .await
            .expect("load_stats_rows with since filter");
        assert_eq!(rows.len(), 1, "since filter must exclude old cycle");
        assert_eq!(rows[0].cycle_id, "cycle-new");
    }

    // ─── Control Tower S0: session digest enrichment (O1b/O1c/O3) ─────────────

    /// Seed a session with two cycles, link them via events, attach costs to
    /// both and an honesty check to the newer cycle. The session-level
    /// aggregations must sum cost across both cycles, report the newest
    /// honesty outcome, and surface the newest cycle id for pause/resume.
    #[tokio::test]
    async fn test_session_enrichment_cost_honesty_and_active_cycle() {
        let pool = open_stats_pool().await;

        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sess-ct', 'strat-ct', '{}', 'running', 'once', '2026-06-07T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Two cycles, newer one has the higher seq (autoincrement insert order).
        for (cid, ts) in [
            ("cycle-ct-01", "2026-06-07T00:00:00Z"),
            ("cycle-ct-02", "2026-06-07T01:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO autooptimizer_events \
                 (session_id, cycle_id, kind, payload_json, ts) \
                 VALUES ('sess-ct', ?, 'cycle_started', '{}', ?)",
            )
            .bind(cid)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Cost on both cycles → session total = 0.10 + 0.05 = 0.15.
        seed_cycle_cost(&pool, "cycle-ct-01", 0.10, "2026-06-07T00:00:00Z").await;
        seed_cycle_cost(&pool, "cycle-ct-02", 0.05, "2026-06-07T01:00:00Z").await;

        // Honesty check FAILED on the newer cycle (passed = 0).
        sqlx::query(
            "INSERT INTO cycle_honesty_checks \
             (cycle_id, passed, sabotage_variant, message, gate_verdict, parent_hash, created_at) \
             VALUES ('cycle-ct-02', 0, 'shuffle', 'caught', 'Vetoed', 'p', '2026-06-07T01:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let cost = session_cost_usd(&pool, "sess-ct").await.expect("cost");
        assert!(
            (cost.unwrap() - 0.15).abs() < 1e-9,
            "session cost must sum both cycles, got {cost:?}"
        );

        let honesty = session_latest_honesty(&pool, "sess-ct").await.expect("honesty");
        assert_eq!(honesty, Some(false), "newest cycle's honesty check failed");

        let cycle = active_session_cycle_id(&pool, "sess-ct")
            .await
            .expect("active cycle");
        assert_eq!(
            cycle.as_deref(),
            Some("cycle-ct-02"),
            "active cycle must be the newest by event seq"
        );
    }

    /// A session with no priced cycles and no honesty check yields `None` for
    /// both (renders as "$?" / "—"), never a misleading "$0.00" or "passed".
    #[tokio::test]
    async fn test_session_enrichment_absent_signals_are_none() {
        let pool = open_stats_pool().await;
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sess-empty', 'strat-x', '{}', 'finished', 'once', '2026-06-07T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(session_cost_usd(&pool, "sess-empty").await.unwrap(), None);
        assert_eq!(session_latest_honesty(&pool, "sess-empty").await.unwrap(), None);
        assert_eq!(active_session_cycle_id(&pool, "sess-empty").await.unwrap(), None);
    }

    // ─── test_list_cycles_session_id_filter ──────────────────────────────────
    //
    // Verifies GET /api/autooptimizer/cycles?session_id=X filters out cycles
    // that belong to other sessions and returns only the cycles from session X.

    /// Two sessions (sess-1 owns cycle-1a + cycle-1b; sess-2 owns cycle-2).
    /// Calling `list_cycles` with `session_id=sess-1` must return exactly the
    /// two cycles from sess-1 and exclude cycle-2.
    #[tokio::test]
    async fn test_list_cycles_session_id_filter() {
        let pool = open_stats_pool().await;

        // Seed sessions.
        for (sid, strat, ts) in [
            ("sess-1", "strat-alpha", "2026-06-01T00:00:00Z"),
            ("sess-2", "strat-beta", "2026-06-02T00:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?, ?, '{}', 'finished', 'once', ?)",
            )
            .bind(sid)
            .bind(strat)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Link cycles to sessions via autooptimizer_events.
        for (sid, cid, ts) in [
            ("sess-1", "cycle-1a", "2026-06-01T00:00:00Z"),
            ("sess-1", "cycle-1b", "2026-06-01T01:00:00Z"),
            ("sess-2", "cycle-2", "2026-06-02T00:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO autooptimizer_events \
                 (session_id, cycle_id, kind, payload_json, ts) \
                 VALUES (?, ?, 'cycle_started', '{}', ?)",
            )
            .bind(sid)
            .bind(cid)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Seed one lineage node per cycle so list_cycle_runs has rows.
        // cycle-2 is newest overall; the scoped query below uses limit=1,
        // so filtering after pagination would incorrectly return an empty
        // page for sess-1.
        for (cid, ts) in [
            ("cycle-1a", "2026-06-01T00:05:00Z"),
            ("cycle-1b", "2026-06-01T01:05:00Z"),
            ("cycle-2", "2026-06-02T00:05:00Z"),
        ] {
            seed_cycle_nodes(&pool, cid, &[(cid, "active")], ts).await;
        }

        // Build an AppState-less query by exercising the bridge logic directly:
        // resolve allowed cycle ids for sess-1.
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT DISTINCT cycle_id FROM autooptimizer_events \
             WHERE session_id = ? AND cycle_id IS NOT NULL",
        )
        .bind("sess-1")
        .fetch_all(&pool)
        .await
        .unwrap();
        let ids: Vec<String> = rows
            .into_iter()
            .map(|r| r.try_get::<String, _>("cycle_id").unwrap())
            .collect();

        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["cycle-1a", "cycle-1b"],
            "bridge must return both sess-1 cycles"
        );

        // Unfiltered list sees all cycles, newest first.
        let all_runs = list_cycle_runs(&pool, 50, 0).await.expect("list_cycle_runs");
        assert_eq!(all_runs.len(), 3, "unfiltered list must have all 3 cycles");

        let filtered = list_cycle_runs_filtered(&pool, &ids, 50, 0)
            .await
            .expect("filtered list_cycle_runs");
        assert_eq!(filtered.len(), 2, "session filter must keep exactly 2 cycles");
        let cycle_ids: Vec<&str> = filtered.iter().map(|r| r.cycle_id.as_str()).collect();
        assert!(cycle_ids.contains(&"cycle-1a"), "cycle-1a must be in result");
        assert!(cycle_ids.contains(&"cycle-1b"), "cycle-1b must be in result");
        assert!(!cycle_ids.contains(&"cycle-2"), "cycle-2 must be excluded");

        let first_page = list_cycle_runs_filtered(&pool, &ids, 1, 0)
            .await
            .expect("filtered first page");
        assert_eq!(
            first_page.iter().map(|r| r.cycle_id.as_str()).collect::<Vec<_>>(),
            vec!["cycle-1b"],
            "session filtering must happen before LIMIT/OFFSET"
        );

        // No-session case: unknown session id returns empty bridge result.
        let empty_rows = sqlx::query(
            "SELECT DISTINCT cycle_id FROM autooptimizer_events \
             WHERE session_id = ? AND cycle_id IS NOT NULL",
        )
        .bind("sess-nonexistent")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            empty_rows.is_empty(),
            "unknown session must yield empty cycle list"
        );
    }
}

#[cfg(test)]
mod river_tests {
    use super::*;
    use axum::extract::State;
    use tempfile::TempDir;

    /// Spin up a fresh `AppState` backed by a temp dir.
    /// `AppState::new` runs all engine migrations (048, 057, 058, …) so
    /// `lineage_nodes` and `autooptimizer_gate_records` already exist.
    async fn fresh_state() -> (crate::state::AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let cfg =
            std::fs::read_to_string("../../config/default.toml").expect("read workspace config/default.toml");
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = crate::state::AppState::new(xvn_home)
            .await
            .expect("AppState::new");
        (state, tmp)
    }

    // ── test_get_river_joins_scores_and_orders_by_created_at ─────────────────

    /// Two lineage nodes: hash-a (no gate record) and hash-b (gate record with
    /// child_day_score=1.52, delta_day=0.21). The river endpoint must return
    /// both, with hash-a first (older), and hash-b with scores populated.
    #[tokio::test]
    async fn test_get_river_joins_scores_and_orders_by_created_at() {
        let (state, _tmp) = fresh_state().await;

        sqlx::query(
            "INSERT INTO lineage_nodes (bundle_hash, parent_hash, cycle_id, status, gate_verdict, created_at)
             VALUES ('hash-a', NULL, 'cyc-1', 'active', 'Pass', '2026-06-10T00:00:00Z'),
                    ('hash-b', 'hash-a', 'cyc-2', 'rejected', '{\"Fail\":{\"reason\":\"overfit\"}}', '2026-06-11T00:00:00Z')",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO autooptimizer_gate_records (bundle_hash, child_day_score, delta_day, verdict, created_at)
             VALUES ('hash-b', 1.52, 0.21, 'Fail', '2026-06-11T00:00:00Z')",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let resp = get_river(State(state)).await.unwrap();
        let nodes = resp.0;
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].bundle_hash, "hash-a");
        assert_eq!(nodes[0].child_day_score, None);
        assert_eq!(nodes[1].child_day_score, Some(1.52));
        assert_eq!(nodes[1].delta_day, Some(0.21));
        assert_eq!(nodes[1].parent_hash.as_deref(), Some("hash-a"));
    }
}
