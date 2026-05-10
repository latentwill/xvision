//! `/api/settings/brokers` — read-only env / config snapshot for the two
//! brokers v1 actually wires (Alpaca paper, Orderly live). No mutation here;
//! credential editing is part of the onboarding flow.

use std::env;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokersReport {
    pub alpaca: BrokerEntry,
    pub orderly: BrokerEntry,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerEntry {
    /// Display name ("Alpaca", "Orderly Network").
    pub name: String,
    /// Stable kind tag for the frontend ("alpaca" | "orderly").
    pub kind: String,
    /// Per-required-env-var presence; values are never returned.
    pub credentials: Vec<CredentialRef>,
    /// Roll-up: are *all* required credentials set?
    pub configured: bool,
    /// Optional base URL; surfaces the override if set, else default.
    pub base_url: Option<String>,
    /// Short note for v1 ("paper trading", "live only — post-v1", etc.).
    pub note: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    /// Env var name (e.g. "APCA_API_KEY_ID"). Safe to display.
    pub env_var: String,
    /// True if the env var is set (and non-empty). Value never leaks.
    pub is_set: bool,
}

pub async fn get(ctx: &ApiContext) -> ApiResult<BrokersReport> {
    let started = Instant::now();
    let result = get_inner();

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "settings",
        "brokers.get",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

fn get_inner() -> ApiResult<BrokersReport> {
    Ok(BrokersReport {
        alpaca: alpaca_entry(),
        orderly: orderly_entry(),
    })
}

fn alpaca_entry() -> BrokerEntry {
    let credentials = vec![
        cred("APCA_API_KEY_ID"),
        cred("APCA_API_SECRET_KEY"),
    ];
    let configured = credentials.iter().all(|c| c.is_set);
    BrokerEntry {
        name: "Alpaca".into(),
        kind: "alpaca".into(),
        credentials,
        configured,
        base_url: env::var("APCA_API_BASE_URL").ok().filter(|s| !s.is_empty()),
        note: Some("paper trading (v1 default)".into()),
    }
}

fn orderly_entry() -> BrokerEntry {
    let credentials = vec![
        cred("ORDERLY_KEY"),
        cred("ORDERLY_SECRET"),
        cred("ORDERLY_ACCOUNT_ID"),
    ];
    let configured = credentials.iter().all(|c| c.is_set);
    BrokerEntry {
        name: "Orderly Network".into(),
        kind: "orderly".into(),
        credentials,
        configured,
        base_url: env::var("ORDERLY_BASE_URL").ok().filter(|s| !s.is_empty()),
        note: Some("live only — disabled in v1 paper mode".into()),
    }
}

fn cred(env_var: &str) -> CredentialRef {
    CredentialRef {
        env_var: env_var.into(),
        is_set: env::var(env_var).map(|v| !v.is_empty()).unwrap_or(false),
    }
}

#[allow(dead_code)]
fn _api_error_anchor(_e: ApiError) {}
