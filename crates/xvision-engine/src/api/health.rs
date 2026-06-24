//! `/api/health` engine surface — probes the local dependencies a v1 install
//! actually relies on (xvn home dir, sqlite pool, strategy store).
//!
//! Probes that need credentials or external network (alpaca paper, llm) are
//! intentionally deferred — they show up in plan 2 once the providers and
//! brokers config land. Until then the report covers everything that should
//! pass on a fresh install with no API keys.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::strategies::store::strategy_store_dir;

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
    let mut probes = Vec::with_capacity(5);
    probes.push(probe_data_dir(ctx));
    probes.push(probe_db(ctx).await);
    probes.push(probe_strategies(ctx));
    probes.push(probe_agent_sidecar().await);
    probes.push(probe_provider_active(ctx).await);

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

fn probe_strategies(ctx: &ApiContext) -> Probe {
    let dir = strategy_store_dir(&ctx.xvn_home);
    if !dir.exists() {
        return Probe {
            name: "strategies".into(),
            status: HealthStatus::Ok,
            detail: Some("0 (no strategies dir yet)".into()),
            latency_ms: None,
        };
    }
    match std::fs::read_dir(&dir) {
        Ok(rd) => {
            let count = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                .count();
            Probe {
                name: "strategies".into(),
                status: HealthStatus::Ok,
                detail: Some(format!("{count}")),
                latency_ms: None,
            }
        }
        Err(e) => Probe {
            name: "strategies".into(),
            status: HealthStatus::Degraded,
            detail: Some(e.to_string()),
            latency_ms: None,
        },
    }
}
/// Check that the Cline sidecar (xvision-agentd) binary is reachable.
/// Reads `XVN_AGENTD_BIN` from the environment and verifies the file
/// exists. If the env var is unset, the probe returns Degraded with a
/// hint to set it — eval runs will fail without it.
async fn probe_agent_sidecar() -> Probe {
    match std::env::var("XVN_AGENTD_BIN") {
        Ok(path) if !path.trim().is_empty() => {
            let p = std::path::Path::new(&path);
            if p.exists() {
                Probe {
                    name: "agent_sidecar".into(),
                    status: HealthStatus::Ok,
                    detail: Some(path),
                    latency_ms: None,
                }
            } else {
                Probe {
                    name: "agent_sidecar".into(),
                    status: HealthStatus::Degraded,
                    detail: Some(format!("XVN_AGENTD_BIN={path} but file not found")),
                    latency_ms: None,
                }
            }
        }
        _ => Probe {
            name: "agent_sidecar".into(),
            status: HealthStatus::Degraded,
            detail: Some("XVN_AGENTD_BIN not set — eval runs require the Cline sidecar".into()),
            latency_ms: None,
        },
    }
}

/// Check that at least one LLM provider has an API key configured.
/// Degraded when no providers have keys — eval runs will fail.
async fn probe_provider_active(ctx: &ApiContext) -> Probe {
    let config_path = xvision_core::config::runtime_config_path(&ctx.xvn_home);
    let providers = crate::api::settings::providers::effective_providers_with_paths(
        &ctx.xvn_home,
        &config_path,
    )
    .await
    .unwrap_or_default();

    let active_count = providers.iter().filter(|p| p.has_key).count();
    if active_count > 0 {
        Probe {
            name: "providers".into(),
            status: HealthStatus::Ok,
            detail: Some(format!("{active_count} with API key")),
            latency_ms: None,
        }
    } else {
        Probe {
            name: "providers".into(),
            status: HealthStatus::Degraded,
            detail: Some("no LLM provider configured — add an API key in Settings → Providers".into()),
            latency_ms: None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;

    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "operator".into(),
            },
        )
        .await
        .unwrap();
        (ctx, dir)
    }

    /// Spec G.2 (v1 gaps Track G): a fresh `xvn_home` with the migrations
    /// applied passes every probe — `data_dir` exists (we just created it),
    /// `db` answers `SELECT 1`, `strategies` is absent and that's reported as
    /// the empty-but-ok shape. The aggregate rolls up to `Ok`.
    #[tokio::test]
    async fn check_returns_ok_on_fresh_xvn_home() {
        let (ctx, _dir) = fresh_ctx().await;
        let report = check(&ctx).await.unwrap();
        assert_eq!(report.status, HealthStatus::Ok);
        assert_eq!(report.probes.len(), 3);
        for p in &report.probes {
            assert_eq!(
                p.status,
                HealthStatus::Ok,
                "probe {} should be Ok, got {:?} ({:?})",
                p.name,
                p.status,
                p.detail,
            );
        }
    }

    /// Spec G.2: when the sqlite pool is unusable, the `db` probe must
    /// surface as `Down` (not error out the whole report). Closing the pool
    /// is the cleanest way to simulate "db can't be opened" without leaking
    /// the test env onto a real path. Aggregate falls to `Down`.
    #[tokio::test]
    async fn check_flags_db_when_pool_closed() {
        let (ctx, _dir) = fresh_ctx().await;
        ctx.db.close().await;

        let report = check(&ctx).await.unwrap();
        let db = report
            .probes
            .iter()
            .find(|p| p.name == "db")
            .expect("db probe present");
        assert_eq!(db.status, HealthStatus::Down, "detail: {:?}", db.detail);
        assert_eq!(report.status, HealthStatus::Down);
    }

    /// Spec G.2: when the `strategies/` directory does not exist (e.g. an
    /// `ApiContext` built via `new()` without running migrations or seed), the
    /// probe must still render as Ok — it returns the empty-but-ok shape
    /// `"0 (no strategies dir yet)"`. Catches the regression where someone
    /// "fixes" the missing-dir branch to return Degraded.
    ///
    /// Note: `ApiContext::open()` creates `xvn_home`, so we use
    /// `ApiContext::new()` here to keep the strategies dir genuinely absent.
    #[tokio::test]
    async fn check_flags_missing_strategies_dir_renders_zero_count_ok() {
        use sqlx::SqlitePool;
        let dir = tempfile::tempdir().unwrap();
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "operator".into(),
            },
            dir.path().to_path_buf(),
        );
        let report = check(&ctx).await.unwrap();
        let probe = report
            .probes
            .iter()
            .find(|p| p.name == "strategies")
            .expect("strategies probe present");
        assert_eq!(probe.status, HealthStatus::Ok);
        assert_eq!(
            probe.detail.as_deref(),
            Some("0 (no strategies dir yet)"),
            "expected the missing-dir empty shape, got {:?}",
            probe.detail,
        );
    }

    /// Spec G.2: `HealthReport` is the wire shape served by the dashboard's
    /// `/api/health`; the round-trip guards against a future field rename
    /// breaking the JSON contract. Asserts every variant of `HealthStatus`
    /// + every `Probe` field survives serialize → deserialize.
    #[test]
    fn health_report_serialization_round_trip() {
        let report = HealthReport {
            status: HealthStatus::Degraded,
            probes: vec![
                Probe {
                    name: "data_dir".into(),
                    status: HealthStatus::Ok,
                    detail: Some("/tmp/xvn".into()),
                    latency_ms: None,
                },
                Probe {
                    name: "db".into(),
                    status: HealthStatus::Degraded,
                    detail: Some("slow".into()),
                    latency_ms: Some(420),
                },
                Probe {
                    name: "strategies".into(),
                    status: HealthStatus::Down,
                    detail: None,
                    latency_ms: None,
                },
            ],
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: HealthReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, report.status);
        assert_eq!(parsed.probes.len(), report.probes.len());
        for (a, b) in parsed.probes.iter().zip(report.probes.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.status, b.status);
            assert_eq!(a.detail, b.detail);
            assert_eq!(a.latency_ms, b.latency_ms);
        }
        // serde rename_all = "snake_case" — assert the wire form too.
        assert!(json.contains("\"degraded\""));
        assert!(json.contains("\"ok\""));
        assert!(json.contains("\"down\""));
    }
}
