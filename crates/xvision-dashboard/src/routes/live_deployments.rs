//! `GET /api/live/deployments` — list of live/paper deployments (poll, ~5s).
//! `GET /api/live/deployments/:id/stream` — per-deployment SSE.
//!
//! CT5 (Epic s78 Wave 3). A deployment is an `eval_runs` row with `mode='live'`;
//! these handlers are a filtered, honesty-constrained projection over
//! `eval_runs WHERE mode='live'`, joined with execution-layer truth. The
//! endpoint + DTO names deliberately differ from `agent_runs`/`RunSummary` so
//! the dashboard never *infers* live status from a trace record (§8.9).
//!
//! HONESTY MANDATE (§8.1): every capital / P&L field is sourced from broker /
//! execution state; unsourceable values surface as `null` ("—" in the UI),
//! never a fabricated `0`. The projection lives in
//! `xvision_engine::api::live_deployments`; these handlers wire it to the pool,
//! the global safety state, and the eval event bus.
//!
//! SSE-vs-poll capital scope (CT5, honest as-built): the per-deployment SSE
//! carries equity ticks (`event: metrics`, equity-only) plus lifecycle/terminal
//! `status` frames. The FULL capital metrics block (`deployed_capital_usd`,
//! `unrealized_pnl_usd`, `realized_pnl_usd`, `daily_loss_limit_remaining_usd`,
//! `drawdown_pct`) and `risk_veto` counts are read via the 5s poll above, NOT
//! over the stream — they are emitted on the engine `ProgressBus` but not yet
//! projected onto the `RunChartEvent` the SSE reads. Per-tick capital streaming
//! is a DEFERRED follow-up (requires widening `RunChartEvent`).

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::live_deployments::{
    get_live_deployment, list_live_deployments, DeploymentStatus, LiveDeploymentSummary,
};
use xvision_engine::api::safety::routes::get_state as safety_get_state;
use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::store::RunStore;

use crate::error::DashboardError;
use crate::sse::live_deployment_sse::live_deployment_sse;
use crate::state::AppState;

/// Query params for the deployments list.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// Filter by DEPLOYMENT status — the operator-facing vocabulary, NOT the
    /// raw `RunStatus`. Accepts a comma-separated list drawn from
    /// `running | paused | stopped | starting | failed` (e.g. the n0k default
    /// `running,paused`). The default (absent) returns all live deployments.
    /// Each token maps to one or more `RunStatus` values (and `paused`
    /// additionally post-filters on the per-run / global pause flags):
    /// `running→Running`, `starting→Queued`, `stopped→{Completed,Cancelled}`,
    /// `failed→Failed`, `paused→Running + (paused || global_safety_paused)`.
    /// Tokens outside this set surface as a validation error (HTTP 400).
    pub status: Option<String>,
    /// Filter by mode ("paper" | "live"). Defensive; the projection already
    /// carries the mode so this is a post-projection filter.
    pub mode: Option<String>,
}

/// Parsed deployment-status filter: the set of `RunStatus` values to pull from
/// the store, plus whether the operator asked for `running` and/or `paused`
/// (which both map to `RunStatus::Running` but differ on the pause overlay, so
/// they are disambiguated by a post-projection filter on the summary's
/// effective status). `None` everywhere = no filter (all live deployments).
#[derive(Debug, Default)]
struct DeploymentStatusFilter {
    /// `RunStatus` values to match (deduped). `RunStatus` is a small `Copy`
    /// enum that does not implement `Ord`/`Hash`, so a `Vec` + `contains` is the
    /// right container here (membership is over at most five variants).
    run_statuses: Vec<RunStatus>,
    /// Operator asked for `running` (an active, non-paused deployment).
    want_running: bool,
    /// Operator asked for `paused` (a running run with a pause overlay).
    want_paused: bool,
}

impl DeploymentStatusFilter {
    /// Parse the comma-separated DEPLOYMENT-status vocabulary. Returns the
    /// offending token on the first value outside the accepted set so the
    /// handler can surface a precise 400.
    fn parse(raw: &str) -> Result<Self, String> {
        fn push_unique(v: &mut Vec<RunStatus>, s: RunStatus) {
            if !v.contains(&s) {
                v.push(s);
            }
        }
        let mut f = DeploymentStatusFilter::default();
        let mut any = false;
        for tok in raw.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            any = true;
            match tok.to_ascii_lowercase().as_str() {
                "running" => {
                    push_unique(&mut f.run_statuses, RunStatus::Running);
                    f.want_running = true;
                }
                "starting" => {
                    push_unique(&mut f.run_statuses, RunStatus::Queued);
                }
                "stopped" => {
                    push_unique(&mut f.run_statuses, RunStatus::Completed);
                    push_unique(&mut f.run_statuses, RunStatus::Cancelled);
                }
                "failed" => {
                    push_unique(&mut f.run_statuses, RunStatus::Failed);
                }
                "paused" => {
                    // Paused deployments are `RunStatus::Running` rows with a
                    // pause overlay; pull Running rows then post-filter.
                    push_unique(&mut f.run_statuses, RunStatus::Running);
                    f.want_paused = true;
                }
                other => return Err(format!("unknown deployment status '{other}'")),
            }
        }
        if !any {
            return Err("empty status filter".to_string());
        }
        Ok(f)
    }

    /// Keep a projected deployment iff it matches the requested status set.
    /// The store-level `RunStatus` filter already narrowed membership; this
    /// disambiguates `running` vs `paused` on the summary's effective status
    /// (running rows that are paused belong only to `want_paused`, and vice
    /// versa). Non-Running statuses are accepted as-is (already SQL-filtered).
    fn keep(&self, d: &LiveDeploymentSummary) -> bool {
        // A row whose effective status is Paused was a Running run with the
        // per-run OR global-safety pause overlay applied during projection.
        let is_paused = d.paused || d.global_safety_paused;
        match d.status {
            DeploymentStatus::Running => self.want_running && !is_paused,
            DeploymentStatus::Paused => self.want_paused && is_paused,
            // Starting / Stopped / Failed were already narrowed by the SQL-level
            // RunStatus set; if they are present they were requested.
            _ => true,
        }
    }
}

/// `{ items, total }` list envelope. Hand-written TS in `live-deployments.ts`
/// (mirrors the agent-runs convention); replace with generated bindings when
/// the backend lands ts-rs derives on the envelope.
#[derive(Debug, Serialize)]
pub struct ListDeploymentsResponse {
    pub items: Vec<LiveDeploymentSummary>,
    /// Count of live deployments matching the filter (pre any future limit).
    pub total: usize,
}

/// Read the global safety pause state once per list/snapshot build. A
/// deployment must never render green "running" while the global safety gate is
/// paused (§8), so this is folded into every projected row.
async fn global_safety_paused(state: &AppState) -> bool {
    safety_get_state(state.safety_manager()).await.paused
}

/// `GET /api/live/deployments` — list (poll, ~5s).
pub async fn list_deployments(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListDeploymentsResponse>, DashboardError> {
    // Parse the comma-separated DEPLOYMENT-status vocabulary (running | paused |
    // stopped | starting | failed). This is NOT the raw RunStatus vocabulary —
    // `stopped` fans out to {Completed, Cancelled} and `paused` is a Running row
    // with a pause overlay, so a single `RunStatus::parse` would 400 on the
    // contract's documented values.
    let status_filter = params
        .status
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            DeploymentStatusFilter::parse(s).map_err(|msg| DashboardError::Validation {
                field: "status".into(),
                msg,
            })
        })
        .transpose()?;

    let store = RunStore::new(state.pool.clone());
    let paused = global_safety_paused(&state).await;
    // Fetch ALL live deployments (no SQL-level status filter) and apply the
    // deployment-status filter on the projected summaries. The engine projection
    // only accepts a single `Option<RunStatus>`, but the deployment vocabulary
    // maps to multiple RunStatus values plus a pause overlay — so filtering on
    // the projected summary (which carries the derived status + pause flags) is
    // the faithful place to apply it.
    let mut items = list_live_deployments(&store, None, paused)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_live_deployments: {e}")))?;

    // Overlay the per-run `paused` flag. The engine's `list` path projects via
    // `row_to_run`, which does NOT read the `paused` column (only `RunStore::get`
    // overlays it). Without this, a paused deployment would falsely render as
    // `Running` on the poll path and the `paused` status filter could never
    // match. We re-overlay it here (and re-derive Running→Paused) so both the
    // returned DTO and the status filter below are honest. `global_safety_paused`
    // is already applied by the engine projection.
    for d in items.iter_mut() {
        let is_paused = store.is_paused(&d.deployment_id).await.unwrap_or(false);
        if is_paused {
            d.paused = true;
            if d.status == DeploymentStatus::Running {
                d.status = DeploymentStatus::Paused;
            }
        }
    }

    if let Some(filter) = status_filter.as_ref() {
        items.retain(|d| filter.run_statuses.contains(&run_status_of(d)) && filter.keep(d));
    }

    // Post-projection mode filter (mode is sourced from live_config, not SQL).
    if let Some(mode) = params.mode.as_deref().filter(|m| !m.trim().is_empty()) {
        let want = mode.to_ascii_lowercase();
        items.retain(|d| serde_mode(&d.mode) == want);
    }

    let total = items.len();
    Ok(Json(ListDeploymentsResponse { items, total }))
}

/// Recover the underlying `RunStatus` from a projected deployment so the
/// store-level status set can be matched against the summary (the projection
/// collapses Completed+Cancelled into `Stopped` and overlays pause onto
/// `Running`, so we invert that derivation here). `Stopped` is treated as a
/// match for BOTH Completed and Cancelled via the filter's set membership.
fn run_status_of(d: &LiveDeploymentSummary) -> RunStatus {
    match d.status {
        DeploymentStatus::Starting => RunStatus::Queued,
        // Both Running and Paused deployments are `RunStatus::Running` rows.
        DeploymentStatus::Running | DeploymentStatus::Paused => RunStatus::Running,
        DeploymentStatus::Failed => RunStatus::Failed,
        // A Stopped deployment is Completed or Cancelled. We can't recover which
        // from the summary, so report Completed and let the filter's set (which
        // contains BOTH when `stopped` was requested) accept it. To keep the
        // membership check honest for Cancelled-only requests, the filter always
        // inserts both for `stopped`, so reporting Completed is sufficient.
        DeploymentStatus::Stopped => RunStatus::Completed,
    }
}

/// Serialize a `DeploymentMode` to its wire string for the post-projection
/// filter, without re-importing the enum (it serde-serializes snake_case).
fn serde_mode(mode: &xvision_engine::api::live_deployments::DeploymentMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// `GET /api/live/deployments/:id/stream` — per-deployment SSE.
///
/// Mirrors `eval_runs::stream`:
/// 1. **Terminal pre-check** — fetch the run; if `status.is_terminal()` the
///    executor already dropped the bus channel, so subscribing would hand back a
///    fresh channel that never fires and a late subscriber would hang forever.
///    Instead we build the final snapshot and return an Sse that emits ONE
///    synthetic `status` frame and immediately ends.
/// 2. **Subscribe before snapshot** — for a still-live run, subscribe to the
///    eval event bus BEFORE building the snapshot so no event committed during
///    assembly is lost.
/// 3. **Snapshot-first frame**, then the per-event loop via [`live_deployment_sse`].
pub async fn stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, DashboardError>
{
    let store = RunStore::new(state.pool.clone());
    let paused = global_safety_paused(&state).await;

    // Terminal pre-check (mirrors `eval_runs::stream`). A run already in a
    // terminal state never delivers bus events — its channel was dropped by the
    // executor — so we must NOT enter the recv loop or a late subscriber hangs.
    // Unknown runs / non-live runs are handled by `get_live_deployment` below
    // (it returns None → 404). Only an existing, LIVE, terminal run short-circuits.
    let terminal = match store.get(&id).await {
        Ok(run) => run.mode == xvision_engine::eval::run::RunMode::Live && run.status.is_terminal(),
        Err(_) => false,
    };

    // Subscribe before snapshot (copy `agent_runs::stream`) so events committed
    // during snapshot assembly are still delivered. For a terminal run this
    // receiver is never read — the builder short-circuits — but subscribing is
    // harmless and keeps the single return type.
    let rx = state.event_bus.subscribe(&id).await;

    let snapshot = get_live_deployment(&store, &id, paused)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("get_live_deployment: {e}")))?
        .ok_or_else(|| DashboardError::NotFound(format!("live deployment '{id}' not found")))?;

    Ok(live_deployment_sse(snapshot, rx, terminal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;
    use xvision_engine::api::live_deployments::DeploymentSource;

    /// Spin up a fresh `AppState` backed by a temp dir, mirroring
    /// `agent_runs.rs::fresh_state`. `AppState::new` applies all migrations
    /// (incl. 065), so `eval_runs.source` / `unrealized_pnl_usd` exist.
    async fn fresh_state() -> (AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let cfg = std::fs::read_to_string("../../config/default.toml")
            .expect("read workspace config/default.toml");
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = AppState::new(xvn_home).await.expect("AppState::new");
        (state, tmp)
    }

    /// Seed one `eval_runs` row with an explicit mode + source. `scenario_id`
    /// stays NULL (allowed since migration 038), `live_config_json` carries a
    /// minimal live envelope so the projection resolves mode/venue/name.
    async fn seed_run(pool: &sqlx::SqlitePool, id: &str, mode: &str, source: &str) {
        let live_config = serde_json::json!({
            "strategy_id": "s_TEST",
            "assets": [{ "class": "Crypto", "symbol": "BTC/USD", "venue_symbol": "BTC/USD" }],
            "capital": { "initial": 10000.0, "currency": "USD" },
            "broker_creds_ref": "alpaca_paper_default",
            "stop_policy": { "time_limit_secs": 900 },
            "venue_label": "paper",
            "display_name": "Test Deployment"
        });
        sqlx::query(
            "INSERT INTO eval_runs \
             (id, agent_id, scenario_id, mode, status, started_at, source, live_config_json) \
             VALUES (?, 'bundle-hash', NULL, ?, 'running', '2026-06-13T00:00:00Z', ?, ?)",
        )
        .bind(id)
        .bind(mode)
        .bind(source)
        .bind(live_config.to_string())
        .execute(pool)
        .await
        .expect("seed eval_runs row");
    }

    async fn seed_decision(pool: &sqlx::SqlitePool, run_id: &str, idx: i64, ts: &str, pnl: Option<f64>) {
        sqlx::query(
            "INSERT INTO eval_decisions \
             (run_id, decision_index, timestamp, asset, action, conviction, justification, pnl_realized) \
             VALUES (?, ?, ?, 'BTC/USD', 'long', 0.8, 'x', ?)",
        )
        .bind(run_id)
        .bind(idx)
        .bind(ts)
        .bind(pnl)
        .execute(pool)
        .await
        .expect("seed eval_decisions row");
    }

    #[tokio::test]
    async fn list_returns_only_live_runs() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live1", "live", "human").await;
        seed_run(&state.pool, "back1", "backtest", "human").await;
        seed_run(&state.pool, "live2", "live", "optimizer").await;

        let resp = list_deployments(State(state), Query(ListParams::default()))
            .await
            .expect("list ok");
        let ids: std::collections::BTreeSet<&str> =
            resp.0.items.iter().map(|d| d.deployment_id.as_str()).collect();
        assert_eq!(ids, ["live1", "live2"].into_iter().collect());
        assert_eq!(resp.0.total, 2);
        // The backtest run is NEVER projected as a deployment (§8.9).
        assert!(!ids.contains("back1"));
    }

    #[tokio::test]
    async fn list_honesty_no_capital_when_only_eval_data() {
        // Given only a live eval_runs row (no decisions, no live snapshot), NO
        // capital number is emitted — the §8.1 honesty case.
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live1", "live", "human").await;

        let resp = list_deployments(State(state), Query(ListParams::default()))
            .await
            .expect("list ok");
        assert_eq!(resp.0.items.len(), 1);
        let d = &resp.0.items[0];
        assert_eq!(d.last_decision_at, None, "no decision ⇒ last_decision_at null (not started_at)");
        assert_eq!(d.realized_pnl_usd, None, "no realized history ⇒ None, not 0");
        assert_eq!(d.deployed_capital_usd, None);
        assert_eq!(d.unrealized_pnl_usd, None);
        assert_eq!(d.drawdown_pct, None);
        assert_eq!(d.daily_loss_limit_remaining_usd, None);
        // Sourced fields are still present.
        assert_eq!(d.source, DeploymentSource::Human);
        assert_eq!(d.venue, "alpaca-paper");
    }

    #[tokio::test]
    async fn list_last_decision_at_is_max_timestamp_not_started_at() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live1", "live", "human").await;
        seed_decision(&state.pool, "live1", 0, "2026-06-13T10:00:00+00:00", Some(10.0)).await;
        seed_decision(&state.pool, "live1", 1, "2026-06-13T11:00:00+00:00", Some(-4.0)).await;

        let resp = list_deployments(State(state), Query(ListParams::default()))
            .await
            .expect("list ok");
        let d = &resp.0.items[0];
        assert_eq!(d.last_decision_at.as_deref(), Some("2026-06-13T11:00:00+00:00"));
        assert_eq!(d.realized_pnl_usd, Some(6.0));
    }

    #[tokio::test]
    async fn list_source_flows_through() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live1", "live", "optimizer").await;

        let resp = list_deployments(State(state), Query(ListParams::default()))
            .await
            .expect("list ok");
        assert_eq!(resp.0.items[0].source, DeploymentSource::Optimizer);
    }

    #[tokio::test]
    async fn list_rejects_unknown_status() {
        let (state, _tmp) = fresh_state().await;
        let err = list_deployments(
            State(state),
            Query(ListParams {
                status: Some("bogus".into()),
                mode: None,
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DashboardError::Validation { .. }));
    }

    #[tokio::test]
    async fn stream_404_for_unknown_or_backtest_run() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "back1", "backtest", "human").await;

        // Unknown id ⇒ 404.
        let err = stream(State(state.clone()), Path("nope".into())).await.err();
        assert!(matches!(err, Some(DashboardError::NotFound(_))));

        // A backtest run is NOT a deployment ⇒ 404 (never inferred live).
        let err = stream(State(state), Path("back1".into())).await.err();
        assert!(matches!(err, Some(DashboardError::NotFound(_))));
    }

    #[tokio::test]
    async fn stream_opens_for_live_run() {
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live1", "live", "human").await;
        // A live run yields an Sse response (snapshot-first frame builds).
        let ok = stream(State(state), Path("live1".into())).await;
        assert!(ok.is_ok(), "stream must open for a live deployment");
    }

    /// Override a seeded run's lifecycle status (the base `seed_run` hardcodes
    /// 'running'); used to exercise the terminal pre-check + status filtering.
    async fn set_run_status(pool: &sqlx::SqlitePool, id: &str, status: &str) {
        sqlx::query("UPDATE eval_runs SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(pool)
            .await
            .expect("update eval_runs status");
    }

    /// Set the per-run pause flag via the real store API (drives the `paused`
    /// deployment-status filter), so the test exercises the same write path the
    /// live executor uses.
    async fn set_run_paused(pool: &sqlx::SqlitePool, id: &str, paused: bool) {
        RunStore::new(pool.clone())
            .set_paused(id, paused)
            .await
            .expect("set_paused");
    }

    /// Drain an `Sse` response body into the list of decoded SSE frames
    /// (each `event:`/`data:` block separated by a blank line). Used to assert
    /// the terminal pre-check emits exactly ONE frame and then ends, rather
    /// than hanging on a channel that will never fire.
    async fn drain_sse_frames<S>(sse: Sse<S>) -> Vec<String>
    where
        S: tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>> + Send + 'static,
    {
        use axum::response::IntoResponse;

        let resp = sse.into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("collect sse body");
        let text = String::from_utf8_lossy(&bytes).to_string();
        // SSE frames are separated by a blank line. Keep-alive comment lines
        // (": keep-alive") are not data frames; drop empty/comment-only blocks.
        text.split("\n\n")
            .map(str::trim)
            .filter(|block| !block.is_empty() && block.contains("event:"))
            .map(str::to_string)
            .collect()
    }

    #[tokio::test]
    async fn stream_terminal_run_emits_one_frame_and_ends() {
        // FIX 1 regression: a late subscriber to an already-stopped live run
        // must NOT hang on a freshly-recreated bus channel that never fires.
        // The terminal pre-check emits ONE synthetic `status` frame and ends.
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live_done", "live", "human").await;
        set_run_status(&state.pool, "live_done", "completed").await;

        let sse = stream(State(state), Path("live_done".into()))
            .await
            .expect("terminal live run still opens an Sse (it just ends fast)");

        // If the pre-check is missing this future never resolves (the bus
        // channel is fresh and silent), so the test would hang — the assertion
        // below only runs once the stream has fully ended.
        let frames = tokio::time::timeout(Duration::from_secs(5), drain_sse_frames(sse))
            .await
            .expect("terminal stream must END, not hang on a silent channel");

        assert_eq!(
            frames.len(),
            1,
            "terminal run emits exactly one frame, got: {frames:?}"
        );
        assert!(
            frames[0].contains("event: status"),
            "the single frame is a synthetic status frame, got: {}",
            frames[0]
        );
        // The frame carries the FINAL (terminal) snapshot, not a live tick.
        assert!(
            frames[0].contains("\"status\":\"stopped\""),
            "terminal frame carries the stopped snapshot, got: {}",
            frames[0]
        );
    }

    #[tokio::test]
    async fn list_accepts_paused_filter() {
        // FIX 2: 'paused' is a DEPLOYMENT status (running run + paused flag),
        // not a RunStatus. It must be accepted (not 400) and filter to paused
        // deployments only.
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live_run", "live", "human").await; // running, not paused
        seed_run(&state.pool, "live_paused", "live", "human").await;
        set_run_paused(&state.pool, "live_paused", true).await;

        let resp = list_deployments(
            State(state),
            Query(ListParams {
                status: Some("paused".into()),
                mode: None,
            }),
        )
        .await
        .expect("'paused' is a valid deployment status filter");
        let ids: Vec<&str> = resp.0.items.iter().map(|d| d.deployment_id.as_str()).collect();
        assert_eq!(ids, ["live_paused"], "only the paused deployment matches");
    }

    #[tokio::test]
    async fn list_accepts_stopped_filter() {
        // FIX 2: 'stopped' maps to {completed, cancelled} RunStatus.
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live_run", "live", "human").await; // running
        seed_run(&state.pool, "live_completed", "live", "human").await;
        set_run_status(&state.pool, "live_completed", "completed").await;
        seed_run(&state.pool, "live_cancelled", "live", "human").await;
        set_run_status(&state.pool, "live_cancelled", "cancelled").await;

        let resp = list_deployments(
            State(state),
            Query(ListParams {
                status: Some("stopped".into()),
                mode: None,
            }),
        )
        .await
        .expect("'stopped' is a valid deployment status filter");
        let ids: std::collections::BTreeSet<&str> =
            resp.0.items.iter().map(|d| d.deployment_id.as_str()).collect();
        assert_eq!(
            ids,
            ["live_cancelled", "live_completed"].into_iter().collect(),
            "stopped = completed + cancelled"
        );
    }

    #[tokio::test]
    async fn list_accepts_comma_separated_running_paused() {
        // FIX 2: the n0k default 'running,paused' must be accepted and union
        // the two deployment statuses.
        let (state, _tmp) = fresh_state().await;
        seed_run(&state.pool, "live_run", "live", "human").await; // running
        seed_run(&state.pool, "live_paused", "live", "human").await;
        set_run_paused(&state.pool, "live_paused", true).await;
        seed_run(&state.pool, "live_done", "live", "human").await; // stopped → excluded
        set_run_status(&state.pool, "live_done", "completed").await;

        let resp = list_deployments(
            State(state),
            Query(ListParams {
                status: Some("running,paused".into()),
                mode: None,
            }),
        )
        .await
        .expect("'running,paused' is the n0k default filter");
        let ids: std::collections::BTreeSet<&str> =
            resp.0.items.iter().map(|d| d.deployment_id.as_str()).collect();
        assert_eq!(
            ids,
            ["live_paused", "live_run"].into_iter().collect(),
            "running,paused excludes the stopped deployment"
        );
    }
}
