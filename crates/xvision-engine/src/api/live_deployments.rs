//! `LiveDeploymentSummary` — the single shared read contract for a live or
//! paper trading deployment, sourced *exclusively* from broker / execution
//! truth (CT5, Epic s78 Wave 3). See
//! `docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md`.
//!
//! ## What a "deployment" is (no new entity)
//!
//! A deployment is an `eval_runs` row with `mode = RunMode::Forward` (forward-test).
//! There is no separate deployment table: [`list_live_deployments`] is a **filtered,
//! honesty-constrained projection** over `eval_runs WHERE mode='fwd'`, joined
//! from `agent_runs`/`RunSummary` so the dashboard never *infers* live status
//! from a trace record (§8.9 acceptance c).
//!
//! ## HONESTY MANDATE (§8.1 / §8.9)
//!
//! Every field is sourced from broker / execution state. An unsourceable value
//! surfaces as `None` (rendered "—" / "no data" in the UI), **NEVER** a
//! fabricated `0`. In particular:
//!
//! * `realized_pnl_usd` is `SUM(eval_decisions.pnl_realized)` (the engine book's
//!   realized PnL), NEVER the Alpaca `equity - last_equity` proxy and NEVER
//!   Orderly's hardcoded `0.0` portfolio realized (which surfaces as `None`).
//! * `deployed_capital_usd` is open-position notional from the book / broker,
//!   NEVER `live_config.capital.initial` (that is the launch envelope).
//! * `last_decision_at` is `MAX(eval_decisions.timestamp)` — a real recorded
//!   decision — or `None`, NEVER `started_at` as a stand-in.
//! * `drawdown_pct` is execution-equity-derived (in-memory session peak /
//!   equity-curve max), NEVER the eval `max_drawdown_pct` finalized metric.
//! * `mode` (paper/live) comes from `live_config.venue_label`, NOT inferred.
//!
//! ## ts-rs export
//!
//! `LiveDeploymentSummary` + its enums are exported to
//! `frontend/web/src/api/types.gen/` (mirrors `RunSummary`). `DeploymentSource`
//! is the same enum the run model uses — re-exported here, never re-defined.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::eval::run::{Run, RunMode, RunStatus};
use crate::eval::store::RunStore;

pub use crate::eval::run::DeploymentSource;

/// Paper vs. live (real-money) venue. Sourced from `live_config.venue_label`,
/// NOT inferred from `agent_runs`. `VenueLabel::Live` is rejected at validation
/// today, so in Wave 3 every deployment renders `Paper` in practice; both
/// values exist for forward-compatibility.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    Paper,
    Live,
}

/// Operator-facing lifecycle of a deployment, derived from `eval_runs.status`
/// overlaid with the per-run + global pause flags. Live runs are long-lived;
/// `Completed`/`Cancelled` for a live run means the operator stopped it.
///
/// Derivation (§2.2): `Queued → Starting`; `Running` (or `Paused` if the run
/// is paused OR the global safety gate is paused); `Completed`/`Cancelled →
/// Stopped`; `Failed → Failed`. A paused system NEVER renders green "running"
/// (§8) — the global safety pause forces the effective status to `Paused`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Starting,
    Running,
    Paused,
    Stopped,
    Failed,
}

/// Slim wire shape of one live/paper deployment. Every nullable field renders
/// "—" / "no data" in the UI, never a fabricated `0` (HONESTY MANDATE §8.1).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDeploymentSummary {
    /// `eval_runs.id` (ULID). The run is the deployment.
    pub deployment_id: String,
    /// `eval_runs.agent_id` (strategy bundle hash). Persisted at run start.
    pub strategy_id: String,
    /// Resolved display name (`live_config.display_name`); `None` if unresolved.
    /// NOT fabricated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_name: Option<String>,
    /// `live_config.venue_label` (execution config), NOT inferred.
    pub mode: DeploymentMode,
    /// Derived from `eval_runs.status` overlaid with pause flags.
    pub status: DeploymentStatus,
    /// `eval_runs.started_at`. Execution lifecycle.
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub started_at: DateTime<Utc>,
    /// `MAX(eval_decisions.timestamp)` — a real recorded broker-fed decision.
    /// `None` if no decision recorded yet (NOT `started_at`, NOT faked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_decision_at: Option<String>,
    /// Resolved venue id ("alpaca-paper" / "orderly" / …) from
    /// `live_config.broker_creds_ref`. Execution config.
    pub venue: String,
    /// Live reachability of the execution venue. `false` ⇒ capital fields go
    /// `null`.
    pub venue_connected: bool,
    /// Σ open-position notional from broker/book state. `None` when no live
    /// snapshot is available. NOT `live_config.capital.initial`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_capital_usd: Option<f64>,
    /// `SUM(eval_decisions.pnl_realized)` (engine book realized). `None` if no
    /// decision/fill history yet. NEVER the Alpaca proxy, NEVER a faked `0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_pnl_usd: Option<f64>,
    /// Per-run mark-to-market (`eval_runs.unrealized_pnl_usd`). `None` when
    /// unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl_usd: Option<f64>,
    /// `(peak_equity - current_equity) / peak_equity * 100` from the in-memory
    /// per-session peak. `None` when no peak yet. Execution-layer-sourced, NOT
    /// the eval `max_drawdown_pct` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawdown_pct: Option<f64>,
    /// Exact headroom before the enforced daily-loss kill fires. `None` when no
    /// kill policy or no day baseline yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_loss_limit_remaining_usd: Option<f64>,
    /// Daily-loss budget denominator for buffer percentage. `None` when no
    /// daily-loss kill policy exists or the budget is not yet sourced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_loss_budget_usd: Option<f64>,
    /// Wall-clock stop deadline (RFC3339). `None` when the stop policy is not
    /// time-bounded or the deadline is not yet sourced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_at: Option<String>,
    /// Count of risk vetoes since the operator's last visit. `None` until
    /// last-visit tracking lands (Wave 5) — render `None`, not `0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_veto_count_since_last_visit: Option<u32>,
    /// `eval_runs.paused` (per-run pause; execution control).
    pub paused: bool,
    /// `eval_runs.flatten_requested` (execution control).
    pub flatten_requested: bool,
    /// `GET /api/safety/state.paused` (SafetyManager). Surfaced so a deployment
    /// never shows green "running" while writes are globally paused (§8).
    pub global_safety_paused: bool,
    /// `eval_runs.source` (CT5 migration 065). Drives `awm`'s Cancel-gate.
    pub source: DeploymentSource,
    /// Populated when `venue_connected=false` or a capital snapshot is
    /// unavailable. Connection-as-data, mirrors `VenueAccountDto.reason`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

/// The execution-layer truth joined onto a `Run` to build a
/// [`LiveDeploymentSummary`]. Separated from the projection so the projection
/// is a pure function — testable without a DB or a live venue.
///
/// Every field defaults to the HONESTY-MANDATE-safe "no data" value (`None` /
/// `false`), so a deployment with no live snapshot projects with `null` capital
/// fields rather than fabricated zeros.
#[derive(Debug, Clone, Default)]
pub struct ExecutionTruth {
    /// `MAX(eval_decisions.timestamp)`, verbatim RFC3339 text. `None` ⇒ no
    /// decision recorded yet.
    pub last_decision_at: Option<String>,
    /// `SUM(eval_decisions.pnl_realized)` / `PortfolioBook::realized()`. `None`
    /// ⇒ no realized history (NEVER faked `0`).
    pub realized_pnl_usd: Option<f64>,
    /// Σ open-position notional from broker/book. `None` ⇒ no live snapshot.
    pub deployed_capital_usd: Option<f64>,
    /// `(peak - current)/peak*100`. `None` ⇒ no session peak yet.
    pub drawdown_pct: Option<f64>,
    /// Headroom before the daily-loss kill. `None` ⇒ no kill policy / day
    /// baseline.
    pub daily_loss_limit_remaining_usd: Option<f64>,
    /// Daily-loss budget denominator for the strip's buffer percentage.
    pub daily_loss_budget_usd: Option<f64>,
    /// Wall-clock deadline for time-bounded stop policies.
    pub stop_at: Option<String>,
    /// Live reachability probe. `false` ⇒ capital fields are forced `null`.
    pub venue_connected: bool,
    /// Why the venue is unavailable / no snapshot (connection-as-data).
    pub unavailable_reason: Option<String>,
    /// `GET /api/safety/state.paused` snapshot for this list build.
    pub global_safety_paused: bool,
    /// bead s78.2: count of REAL recorded `role='risk'` supervisor notes since
    /// the operator's last-visit boundary (`COUNT(*)` from
    /// [`RunStore::count_risk_vetoes_since`]). `None` when NO `?since` boundary
    /// was supplied — counting "since an unknown time" is not a knowable fact —
    /// OR when the count query errors (degrade to unknown, never a fabricated
    /// affirmative zero); either way the field renders "—". A boundary WITH zero
    /// matching notes is a real, honest `Some(0)` ("0 vetoes since you were last
    /// here"), NEVER `None`.
    pub risk_veto_count_since_last_visit: Option<u32>,
}

/// Resolve the venue id label from a live run's `broker_creds_ref`. The
/// credential ref is a lookup key (e.g. `"alpaca"` / `"alpaca_paper_default"`
/// → "alpaca-paper", `"orderly"` → "orderly"). Falls back to the raw ref when
/// unrecognized so the operator still sees *something* sourced (never faked).
fn resolve_venue(broker_creds_ref: &str) -> String {
    let lower = broker_creds_ref.to_ascii_lowercase();
    if lower.contains("orderly") {
        "orderly".to_string()
    } else if lower.contains("alpaca") {
        "alpaca-paper".to_string()
    } else if broker_creds_ref.trim().is_empty() {
        "unknown".to_string()
    } else {
        broker_creds_ref.to_string()
    }
}

/// Derive the operator-facing [`DeploymentStatus`] from the run's lifecycle
/// state overlaid with the per-run + global pause flags. A paused system NEVER
/// renders green "running" (§8): when `global_safety_paused` is set, a running
/// deployment's effective status is `Paused`.
fn derive_status(status: RunStatus, paused: bool, global_safety_paused: bool) -> DeploymentStatus {
    match status {
        RunStatus::Queued => DeploymentStatus::Starting,
        RunStatus::Running => {
            if paused || global_safety_paused {
                DeploymentStatus::Paused
            } else {
                DeploymentStatus::Running
            }
        }
        RunStatus::Completed | RunStatus::Cancelled | RunStatus::Disconnected => DeploymentStatus::Stopped,
        RunStatus::Failed => DeploymentStatus::Failed,
    }
}

/// Pure projection: build a [`LiveDeploymentSummary`] from a `Run` and the
/// execution-layer truth joined onto it. This is the HONESTY core — it sources
/// every field from the run/execution inputs and NEVER fabricates a capital /
/// P&L number. Capital fields are forced `null` when the venue is unreachable.
///
/// `unrealized_pnl_usd` comes from the persisted per-run column (`Run`), set by
/// the live loop's equity flush; it stays `None` pre-first-fill.
pub fn project_deployment(run: Run, truth: ExecutionTruth) -> LiveDeploymentSummary {
    let (mode, venue, strategy_name) = match run.live_config.as_ref() {
        Some(cfg) => {
            let mode = match cfg.venue_label {
                crate::safety::VenueLabel::Live => DeploymentMode::Live,
                _ => DeploymentMode::Paper,
            };
            let name = if cfg.display_name.trim().is_empty() {
                None
            } else {
                Some(cfg.display_name.clone())
            };
            (mode, resolve_venue(&cfg.broker_creds_ref), name)
        }
        // A live run with no live_config is anomalous; surface honestly as
        // Paper / unknown venue rather than fabricating a label.
        None => (DeploymentMode::Paper, "unknown".to_string(), None),
    };

    // Connection-as-data: when the venue is unreachable, the capital snapshot
    // is not trustworthy, so the deployed/drawdown/daily-loss fields go `null`.
    // Realized PnL is book-derived (persisted decisions) and is NOT gated on
    // venue reachability — it is the run's own recorded history.
    let (deployed_capital_usd, drawdown_pct, daily_loss_limit_remaining_usd) = if truth.venue_connected {
        (
            truth.deployed_capital_usd,
            truth.drawdown_pct,
            truth.daily_loss_limit_remaining_usd,
        )
    } else {
        (None, None, None)
    };

    LiveDeploymentSummary {
        deployment_id: run.id,
        strategy_id: run.agent_id,
        strategy_name,
        mode,
        status: derive_status(run.status, run.paused, truth.global_safety_paused),
        started_at: run.started_at,
        last_decision_at: truth.last_decision_at,
        venue,
        venue_connected: truth.venue_connected,
        deployed_capital_usd,
        realized_pnl_usd: truth.realized_pnl_usd,
        unrealized_pnl_usd: run.unrealized_pnl_usd,
        drawdown_pct,
        daily_loss_limit_remaining_usd,
        daily_loss_budget_usd: truth.daily_loss_budget_usd,
        stop_at: truth.stop_at,
        // bead s78.2: a REAL count of recorded risk-veto supervisor notes since
        // the last-visit boundary. `None` when no `?since` was supplied (can't
        // count "since an unknown time"); `Some(0)` is an honest real zero when
        // a boundary IS supplied but no veto landed after it. Never a faked `0`.
        risk_veto_count_since_last_visit: truth.risk_veto_count_since_last_visit,
        paused: run.paused,
        flatten_requested: run.flatten_requested,
        global_safety_paused: truth.global_safety_paused,
        source: run.source,
        unavailable_reason: truth.unavailable_reason,
    }
}

/// List live/paper deployments — the poll-path projection over
/// `eval_runs WHERE mode='live'`.
///
/// Membership: only runs with `mode == RunMode::Live` are deployments; backtest
/// runs are filtered out entirely. For each, the per-run execution truth is
/// joined from persisted state:
///
/// * `last_decision_at` ← `MAX(eval_decisions.timestamp)` (`None` if no decision).
/// * `realized_pnl_usd` ← `SUM(eval_decisions.pnl_realized)` (`None` if no
///   realized history — NEVER a faked `0`).
/// * `unrealized_pnl_usd` ← the persisted per-run column on the `Run`.
/// * `deployed_capital_usd` / `drawdown_pct` / `daily_loss_limit_remaining_usd`
///   stay `None` on the poll path today: the caller passes `venue_connected =
///   false` (no live snapshot is attached to the poll). These per-tick capital
///   values are computed in the live loop and emitted on the engine
///   `ProgressBus`, but they are NOT yet streamed to the dashboard SSE (which
///   reads `RunChartEvent`, equity-only) — wiring the in-memory session state
///   into this projection is a DEFERRED follow-up, so the honest poll value is
///   `None` until then.
///
/// `global_safety_paused` is read once per list build by the caller and applied
/// to every row.
///
/// Optional `status_filter` restricts membership to runs in that lifecycle
/// status (matching the ActiveTasksStrip "active only" default).
pub async fn list_live_deployments(
    store: &RunStore,
    status_filter: Option<RunStatus>,
    global_safety_paused: bool,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<Vec<LiveDeploymentSummary>> {
    use crate::eval::store::ListFilter;

    // Pull every run matching the status filter, then keep only live ones. The
    // store's SQL list has no `mode` column filter, so the live cut is applied
    // in Rust (the live-run population is small relative to backtests).
    let runs = store
        .list(ListFilter {
            // t4u8.1: ListFilter.status is now Option<Vec<RunStatus>> (multi-status
            // IN filter). A single status maps to a one-element vec.
            status: status_filter.map(|s| vec![s]),
            ..Default::default()
        })
        .await?;

    let mut out = Vec::new();
    for run in runs.into_iter().filter(|r| r.mode == RunMode::Forward) {
        let last_decision_at = store.max_decision_timestamp(&run.id).await.unwrap_or(None);
        let realized_pnl_usd = store.sum_realized_pnl(&run.id).await.unwrap_or(None);
        // bead s78.2: only count risk vetoes when a last-visit boundary is
        // supplied. No boundary ⇒ `None` (can't count "since an unknown time");
        // a boundary ⇒ a real `COUNT(*)`, including an honest `Some(0)`.
        let risk_veto_count_since_last_visit = match since {
            Some(since) => store.count_risk_vetoes_since(&run.id, since).await.ok(),
            None => None,
        };
        // The poll path has no attached live snapshot, so deployed capital /
        // drawdown / daily-loss stay `None` and the venue is reported as not
        // having a live snapshot. Per-tick values stream via the SSE path.
        let truth = ExecutionTruth {
            last_decision_at,
            realized_pnl_usd,
            deployed_capital_usd: None,
            drawdown_pct: None,
            daily_loss_limit_remaining_usd: None,
            daily_loss_budget_usd: None,
            stop_at: None,
            venue_connected: false,
            unavailable_reason: Some("no live snapshot (poll path)".to_string()),
            global_safety_paused,
            risk_veto_count_since_last_visit,
        };
        out.push(project_deployment(run, truth));
    }
    Ok(out)
}

/// Project a single deployment by run id for the SSE snapshot frame. Returns
/// `None` when the run is unknown OR is not a live run (a backtest is never a
/// deployment). Uses the same honesty-constrained join as
/// [`list_live_deployments`]; the capital fields stay `None` here because the
/// SSE body streams equity ticks (`RunChartEvent::Equity`) + lifecycle `status`
/// only — the full capital block is NOT streamed (it is read via the 5s poll;
/// per-tick capital streaming is a deferred follow-up).
pub async fn get_live_deployment(
    store: &RunStore,
    run_id: &str,
    global_safety_paused: bool,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<Option<LiveDeploymentSummary>> {
    let run = match store.get(run_id).await {
        Ok(run) => run,
        // Unknown run id ⇒ no deployment (the handler maps this to 404).
        Err(_) => return Ok(None),
    };
    if run.mode != RunMode::Forward {
        return Ok(None);
    }
    let last_decision_at = store.max_decision_timestamp(run_id).await.unwrap_or(None);
    let realized_pnl_usd = store.sum_realized_pnl(run_id).await.unwrap_or(None);
    // bead s78.2: see `list_live_deployments` — `None` without a boundary, a
    // real `COUNT(*)` (incl. honest `Some(0)`) with one. The SSE snapshot frame
    // passes `since = None` (risk-veto counts are read via the poll, not the
    // stream — see this module's doc + the dashboard stream handler).
    let risk_veto_count_since_last_visit = match since {
        Some(since) => store.count_risk_vetoes_since(run_id, since).await.ok(),
        None => None,
    };
    let truth = ExecutionTruth {
        last_decision_at,
        realized_pnl_usd,
        deployed_capital_usd: None,
        drawdown_pct: None,
        daily_loss_limit_remaining_usd: None,
        daily_loss_budget_usd: None,
        stop_at: None,
        venue_connected: false,
        unavailable_reason: Some("no live snapshot (poll path)".to_string()),
        global_safety_paused,
        risk_veto_count_since_last_visit,
    };
    Ok(Some(project_deployment(run, truth)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::live_config::{LiveConfig, StopPolicy};
    use crate::eval::scenario::{AssetClass, AssetRef};
    use crate::safety::VenueLabel;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;
    use xvision_core::Capital;
    use xvision_data::alpaca::BarGranularity;

    fn live_config(name: &str, creds: &str, venue: VenueLabel) -> LiveConfig {
        LiveConfig {
            strategy_id: "s_TEST".into(),
            assets: vec![AssetRef {
                class: AssetClass::Crypto,
                symbol: "BTC/USD".into(),
                venue_symbol: "BTC/USD".into(),
            }],
            capital: Capital {
                initial: 10_000.0,
                currency: "USD".into(),
            },
            broker_creds_ref: creds.into(),
            stop_policy: StopPolicy {
                time_limit_secs: Some(900),
                ..Default::default()
            },
            granularity: BarGranularity::Minute1,
            venue_label: venue,
            warmup_bars: None,
            safety_limits: None,
            display_name: name.into(),
            description: None,
            tags: vec![],
            notes: None,
        }
    }

    fn live_run(id: &str) -> Run {
        let mut run = Run::new_queued("agent-bundle-hash".into(), "scn".into(), RunMode::Forward);
        run.id = id.into();
        run.status = RunStatus::Running;
        run.live_config = Some(live_config(
            "My Deployment",
            "alpaca_paper_default",
            VenueLabel::Paper,
        ));
        run
    }

    // ── pure projection: the honesty core ──────────────────────────────────

    #[test]
    fn honesty_no_snapshot_yields_null_capital_fields() {
        // Given only a live run with NO live snapshot (venue not connected),
        // NO capital number is emitted — every capital field is None, never a
        // faked 0. This is the §8.1 honesty case.
        let run = live_run("dep1");
        let truth = ExecutionTruth {
            venue_connected: false,
            unavailable_reason: Some("no live snapshot".into()),
            ..Default::default()
        };
        let dto = project_deployment(run, truth);
        assert_eq!(dto.deployed_capital_usd, None);
        assert_eq!(dto.realized_pnl_usd, None);
        assert_eq!(dto.unrealized_pnl_usd, None);
        assert_eq!(dto.drawdown_pct, None);
        assert_eq!(dto.daily_loss_limit_remaining_usd, None);
        assert_eq!(dto.risk_veto_count_since_last_visit, None);
        assert!(!dto.venue_connected);
        assert_eq!(dto.unavailable_reason.as_deref(), Some("no live snapshot"));
    }

    #[test]
    fn last_decision_at_is_none_not_started_at_when_no_decision() {
        // last_decision_at must be None (not started_at) when no decision was
        // recorded — never started_at as a stand-in.
        let run = live_run("dep2");
        let started = run.started_at;
        let dto = project_deployment(run, ExecutionTruth::default());
        assert_eq!(dto.last_decision_at, None);
        // started_at is still surfaced separately and is NOT reused as the
        // decision timestamp.
        assert_eq!(dto.started_at, started);
    }

    #[test]
    fn last_decision_at_flows_through_when_recorded() {
        let run = live_run("dep3");
        let truth = ExecutionTruth {
            last_decision_at: Some("2026-06-13T12:00:00+00:00".into()),
            ..Default::default()
        };
        let dto = project_deployment(run, truth);
        assert_eq!(dto.last_decision_at.as_deref(), Some("2026-06-13T12:00:00+00:00"));
    }

    #[test]
    fn source_flows_through() {
        let mut run = live_run("dep4");
        run.source = DeploymentSource::Optimizer;
        let dto = project_deployment(run, ExecutionTruth::default());
        assert_eq!(dto.source, DeploymentSource::Optimizer);
    }

    #[test]
    fn mode_comes_from_venue_label_not_inferred() {
        let mut run = live_run("dep5");
        run.live_config = Some(live_config("D", "alpaca", VenueLabel::Paper));
        assert_eq!(
            project_deployment(run, ExecutionTruth::default()).mode,
            DeploymentMode::Paper
        );

        let mut run2 = live_run("dep5b");
        run2.live_config = Some(live_config("D", "orderly", VenueLabel::Live));
        assert_eq!(
            project_deployment(run2, ExecutionTruth::default()).mode,
            DeploymentMode::Live
        );
    }

    #[test]
    fn venue_resolves_from_broker_creds_ref() {
        let mut run = live_run("dep6");
        run.live_config = Some(live_config("D", "orderly", VenueLabel::Paper));
        assert_eq!(
            project_deployment(run, ExecutionTruth::default()).venue,
            "orderly"
        );

        let mut run2 = live_run("dep6b");
        run2.live_config = Some(live_config("D", "alpaca_paper_default", VenueLabel::Paper));
        assert_eq!(
            project_deployment(run2, ExecutionTruth::default()).venue,
            "alpaca-paper"
        );
    }

    #[test]
    fn global_safety_pause_forces_non_green_status() {
        // A paused system never renders green "running" (§8).
        let run = live_run("dep7"); // status = Running, not per-run paused
        let truth = ExecutionTruth {
            global_safety_paused: true,
            ..Default::default()
        };
        let dto = project_deployment(run, truth);
        assert_eq!(dto.status, DeploymentStatus::Paused);
        assert!(dto.global_safety_paused);
    }

    #[test]
    fn connected_snapshot_surfaces_capital_fields() {
        let run = live_run("dep8");
        let truth = ExecutionTruth {
            venue_connected: true,
            deployed_capital_usd: Some(5_000.0),
            realized_pnl_usd: Some(-120.0),
            drawdown_pct: Some(2.5),
            daily_loss_limit_remaining_usd: Some(380.0),
            ..Default::default()
        };
        let dto = project_deployment(run, truth);
        assert_eq!(dto.deployed_capital_usd, Some(5_000.0));
        assert_eq!(dto.realized_pnl_usd, Some(-120.0));
        assert_eq!(dto.drawdown_pct, Some(2.5));
        assert_eq!(dto.daily_loss_limit_remaining_usd, Some(380.0));
    }

    #[test]
    fn status_derivation_covers_all_lifecycle_states() {
        assert_eq!(
            derive_status(RunStatus::Queued, false, false),
            DeploymentStatus::Starting
        );
        assert_eq!(
            derive_status(RunStatus::Running, false, false),
            DeploymentStatus::Running
        );
        assert_eq!(
            derive_status(RunStatus::Running, true, false),
            DeploymentStatus::Paused
        );
        assert_eq!(
            derive_status(RunStatus::Completed, false, false),
            DeploymentStatus::Stopped
        );
        assert_eq!(
            derive_status(RunStatus::Cancelled, false, false),
            DeploymentStatus::Stopped
        );
        assert_eq!(
            derive_status(RunStatus::Failed, false, false),
            DeploymentStatus::Failed
        );
    }

    // ── async list projection: only mode='live' runs ───────────────────────

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open sqlite mem pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("apply migrations");
        pool
    }

    async fn seed_scenario(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO scenarios (id, source, display_name, body_json, created_at, created_by) \
             VALUES (?, 'built', 'fixture', '{}', '2026-01-01T00:00:00Z', 'test')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed scenarios row");
    }

    /// Raw INSERT of an eval_runs row with an explicit mode + source.
    async fn seed_run(pool: &SqlitePool, id: &str, mode: &str, source: &str) {
        sqlx::query(
            "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at, source) \
             VALUES (?, 'agent-x', 'scn', ?, 'running', '2026-06-13T00:00:00Z', ?)",
        )
        .bind(id)
        .bind(mode)
        .bind(source)
        .execute(pool)
        .await
        .expect("seed eval_runs row");
    }

    async fn seed_decision(pool: &SqlitePool, run_id: &str, idx: i64, ts: &str, pnl: Option<f64>) {
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
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        seed_run(&pool, "back1", "backtest", "human").await;
        seed_run(&pool, "live2", "live", "optimizer").await;
        let store = RunStore::new(pool);

        let deps = list_live_deployments(&store, None, false, None).await.unwrap();
        let ids: std::collections::BTreeSet<&str> = deps.iter().map(|d| d.deployment_id.as_str()).collect();
        assert_eq!(ids, ["live1", "live2"].into_iter().collect());
        // The backtest run is NEVER projected as a deployment.
        assert!(!ids.contains("back1"));
    }

    #[tokio::test]
    async fn list_honesty_no_decision_means_null_capital_and_last_decision() {
        // Given only a live eval_runs row (no decisions, no live snapshot),
        // NO capital number is emitted and last_decision_at is null.
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        let store = RunStore::new(pool);

        let deps = list_live_deployments(&store, None, false, None).await.unwrap();
        assert_eq!(deps.len(), 1);
        let d = &deps[0];
        assert_eq!(d.last_decision_at, None);
        assert_eq!(d.realized_pnl_usd, None, "no realized history ⇒ None, not 0");
        assert_eq!(d.deployed_capital_usd, None);
        assert_eq!(d.unrealized_pnl_usd, None);
        assert_eq!(d.drawdown_pct, None);
    }

    #[tokio::test]
    async fn list_last_decision_at_is_max_decision_timestamp() {
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        seed_decision(&pool, "live1", 0, "2026-06-13T10:00:00+00:00", Some(10.0)).await;
        seed_decision(&pool, "live1", 1, "2026-06-13T11:00:00+00:00", Some(-4.0)).await;
        let store = RunStore::new(pool);

        let deps = list_live_deployments(&store, None, false, None).await.unwrap();
        let d = &deps[0];
        // MAX(timestamp), not started_at.
        assert_eq!(d.last_decision_at.as_deref(), Some("2026-06-13T11:00:00+00:00"));
        // Realized is the SUM of recorded pnl_realized.
        assert_eq!(d.realized_pnl_usd, Some(6.0));
    }

    #[tokio::test]
    async fn list_realized_is_none_when_only_null_pnl_rows() {
        // Decisions with NULL pnl_realized must not fabricate a 0 realized.
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        seed_decision(&pool, "live1", 0, "2026-06-13T10:00:00+00:00", None).await;
        let store = RunStore::new(pool);

        let deps = list_live_deployments(&store, None, false, None).await.unwrap();
        let d = &deps[0];
        assert_eq!(d.realized_pnl_usd, None, "all-null pnl ⇒ None, never a faked 0");
        // The decision was still recorded, so last_decision_at IS set.
        assert_eq!(d.last_decision_at.as_deref(), Some("2026-06-13T10:00:00+00:00"));
    }

    #[tokio::test]
    async fn list_source_flows_through_from_column() {
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "optimizer").await;
        let store = RunStore::new(pool);

        let deps = list_live_deployments(&store, None, false, None).await.unwrap();
        assert_eq!(deps[0].source, DeploymentSource::Optimizer);
    }

    // ── bead s78.2: risk_veto_count_since_last_visit wiring ─────────────────

    /// Seed an `agent_runs` parent so `supervisor_notes.run_id` has a real
    /// parent (mirrors the live path's invariant). The note's `run_id` matches
    /// the deployment (eval_runs) id, exactly as the live executor records it
    /// via `record_supervisor_note(&run.id, "risk", ...)`.
    async fn seed_agent_run(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
             VALUES (?, 'obj', 'running', '2026-06-13T00:00:00Z', 'full_debug')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed agent_runs row");
    }

    /// Seed one `role='risk'` supervisor note with an explicit `created_at` so
    /// the count's inclusive boundary is deterministic.
    async fn seed_risk_note(pool: &SqlitePool, run_id: &str, created_at: &str, content: &str) {
        sqlx::query(
            "INSERT INTO supervisor_notes (id, run_id, role, content, severity, created_at) \
             VALUES (?, ?, 'risk', ?, 'warn', ?)",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(run_id)
        .bind(content)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("seed supervisor_notes risk row");
    }

    #[tokio::test]
    async fn list_risk_veto_count_is_none_without_a_since_boundary() {
        // No `?since` ⇒ the field is None (can't count "since an unknown time"),
        // even though risk vetoes EXIST on the run.
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        seed_agent_run(&pool, "live1").await;
        seed_risk_note(&pool, "live1", "2026-06-13T10:00:00+00:00", "veto").await;
        let store = RunStore::new(pool);

        let deps = list_live_deployments(&store, None, false, None).await.unwrap();
        assert_eq!(
            deps[0].risk_veto_count_since_last_visit, None,
            "no boundary ⇒ None, never a count (even with real vetoes present)"
        );
    }

    #[tokio::test]
    async fn list_risk_veto_count_excludes_notes_before_boundary() {
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        seed_agent_run(&pool, "live1").await;
        // One veto BEFORE the boundary (excluded), two AT/AFTER (counted).
        seed_risk_note(&pool, "live1", "2026-06-10T00:00:00+00:00", "old").await;
        seed_risk_note(&pool, "live1", "2026-06-12T00:00:00+00:00", "boundary").await;
        seed_risk_note(&pool, "live1", "2026-06-12T06:00:00+00:00", "after").await;
        let store = RunStore::new(pool);

        let since = DateTime::parse_from_rfc3339("2026-06-12T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let deps = list_live_deployments(&store, None, false, Some(since))
            .await
            .unwrap();
        assert_eq!(
            deps[0].risk_veto_count_since_last_visit,
            Some(2),
            "inclusive boundary counts the at/after vetoes, excludes the earlier one"
        );
    }

    #[tokio::test]
    async fn list_risk_veto_count_is_honest_zero_with_boundary_no_matching_notes() {
        // Boundary supplied, no veto after it ⇒ Some(0): an honest real zero
        // ("0 vetoes since you were last here"), NOT None.
        let pool = fresh_pool().await;
        seed_scenario(&pool, "scn").await;
        seed_run(&pool, "live1", "live", "human").await;
        seed_agent_run(&pool, "live1").await;
        seed_risk_note(&pool, "live1", "2026-06-01T00:00:00+00:00", "old only").await;
        let store = RunStore::new(pool);

        let since = DateTime::parse_from_rfc3339("2026-06-12T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let deps = list_live_deployments(&store, None, false, Some(since))
            .await
            .unwrap();
        assert_eq!(
            deps[0].risk_veto_count_since_last_visit,
            Some(0),
            "boundary with zero matching notes ⇒ honest Some(0), never None"
        );
    }

    #[test]
    fn serializes_snake_case_with_skipped_nulls() {
        let run = live_run("dep1");
        let dto = project_deployment(run, ExecutionTruth::default());
        let v = serde_json::to_value(&dto).unwrap();
        // snake_case keys present.
        assert!(v.get("deployment_id").is_some());
        assert!(v.get("venue_connected").is_some());
        assert!(v.get("global_safety_paused").is_some());
        assert_eq!(v.get("mode").and_then(|m| m.as_str()), Some("paper"));
        assert_eq!(v.get("status").and_then(|m| m.as_str()), Some("running"));
        assert_eq!(v.get("source").and_then(|m| m.as_str()), Some("human"));
        // None capital fields are skipped on the wire (rendered "—" by the UI).
        assert!(v.get("deployed_capital_usd").is_none());
        assert!(v.get("realized_pnl_usd").is_none());
        assert!(v.get("last_decision_at").is_none());
    }
}
