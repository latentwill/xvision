//! `/api/settings/daemon` — daemon status. v1 ships without a long-running
//! live daemon; this surface returns a stub explaining that, so the Settings
//! tab can render the canonical "Not in v1" message instead of a broken page.
//! Replace with real telemetry when the live-deploy plan ships.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiResult,
};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonReport {
    pub status: DaemonStatus,
    /// Operator-facing explanation for the current state.
    pub note: String,
    /// Plan that will activate the daemon, or `None` if it's available now.
    pub deferred_to_plan: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonStatus {
    NotApplicable,
    Stopped,
    Running,
}

pub async fn get(ctx: &ApiContext) -> ApiResult<DaemonReport> {
    let started = Instant::now();
    let report = DaemonReport {
        status: DaemonStatus::NotApplicable,
        note: "v1 is single-shot eval + paper trading; no long-running live \
               daemon ships in this release."
            .into(),
        deferred_to_plan: Some(
            "2026-05-08-strategy-engine-2c-scheduler-live-exec.md".into(),
        ),
    };

    let _ = audit::record(
        ctx,
        "settings",
        "daemon.get",
        None,
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}
