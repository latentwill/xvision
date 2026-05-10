//! `/api/health` engine surface — probes the local dependencies a v1 install
//! actually relies on (xvn home dir, sqlite pool, bundle store).
//!
//! Probes that need credentials or external network (alpaca paper, llm) are
//! intentionally deferred — they show up in plan 2 once the providers and
//! brokers config land. Until then the report covers everything that should
//! pass on a fresh install with no API keys.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::api::{ApiContext, ApiError, ApiResult};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub probes: Vec<Probe>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Down,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub name: String,
    pub status: HealthStatus,
    pub detail: Option<String>,
    pub latency_ms: Option<u32>,
}

/// Run every local probe and roll the worst status up to the report level.
/// Errors short-circuit the report rather than aborting the request — a probe
/// failing is the *content* of the response, not a 500.
pub async fn check(ctx: &ApiContext) -> ApiResult<HealthReport> {
    let mut probes = Vec::with_capacity(3);
    probes.push(probe_data_dir(ctx));
    probes.push(probe_db(ctx).await);
    probes.push(probe_bundles(ctx));

    Ok(HealthReport {
        status: aggregate(&probes),
        probes,
    })
}

fn probe_data_dir(ctx: &ApiContext) -> Probe {
    let path = &ctx.xvn_home;
    let exists = path.is_dir();
    Probe {
        name: "data_dir".into(),
        status: if exists {
            HealthStatus::Ok
        } else {
            HealthStatus::Down
        },
        detail: Some(path.display().to_string()),
        latency_ms: None,
    }
}

async fn probe_db(ctx: &ApiContext) -> Probe {
    let started = Instant::now();
    match sqlx::query("SELECT 1 as one")
        .fetch_one(&ctx.db)
        .await
        .and_then(|r| r.try_get::<i64, _>("one"))
    {
        Ok(1) => Probe {
            name: "db".into(),
            status: HealthStatus::Ok,
            detail: None,
            latency_ms: Some(started.elapsed().as_millis() as u32),
        },
        Ok(other) => Probe {
            name: "db".into(),
            status: HealthStatus::Degraded,
            detail: Some(format!("unexpected SELECT 1 result: {other}")),
            latency_ms: Some(started.elapsed().as_millis() as u32),
        },
        Err(e) => Probe {
            name: "db".into(),
            status: HealthStatus::Down,
            detail: Some(e.to_string()),
            latency_ms: None,
        },
    }
}

fn probe_bundles(ctx: &ApiContext) -> Probe {
    let bundles = ctx.xvn_home.join("bundles");
    if !bundles.exists() {
        // Fresh install: not an error, just empty.
        return Probe {
            name: "bundles".into(),
            status: HealthStatus::Ok,
            detail: Some("0 (no bundles dir yet)".into()),
            latency_ms: None,
        };
    }
    match std::fs::read_dir(&bundles) {
        Ok(rd) => {
            let count = rd
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|x| x == "json")
                        .unwrap_or(false)
                })
                .count();
            Probe {
                name: "bundles".into(),
                status: HealthStatus::Ok,
                detail: Some(format!("{count}")),
                latency_ms: None,
            }
        }
        Err(e) => Probe {
            name: "bundles".into(),
            status: HealthStatus::Degraded,
            detail: Some(e.to_string()),
            latency_ms: None,
        },
    }
}

fn aggregate(probes: &[Probe]) -> HealthStatus {
    let worst = probes
        .iter()
        .map(|p| match p.status {
            HealthStatus::Ok => 0,
            HealthStatus::Degraded => 1,
            HealthStatus::Down => 2,
        })
        .max()
        .unwrap_or(0);
    match worst {
        0 => HealthStatus::Ok,
        1 => HealthStatus::Degraded,
        _ => HealthStatus::Down,
    }
}

// `ApiError` is unused here (probes never bubble errors as 500s — they go
// into `Probe::detail`), but the `ApiResult` return type keeps the signature
// shape consistent with every other api fn so downstream wrappers don't need
// a special case.
#[allow(dead_code)]
fn _api_error_anchor(_e: ApiError) {}
