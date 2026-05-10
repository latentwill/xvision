//! Append-only operations log writer. Every `engine::api::*` function calls
//! `record(...)` on completion (both ok and error paths) so we have a
//! single audit trail across CLI, MCP, agent-runner, and scheduler callers.

use crate::api::{ApiContext, ApiResult};
use chrono::Utc;
use ulid::Ulid;

#[derive(Debug)]
pub enum Outcome {
    Ok,
    Error(String),
}

#[allow(clippy::too_many_arguments)]
pub async fn record(
    ctx: &ApiContext,
    domain: &str,
    operation: &str,
    target: Option<&str>,
    args_json: Option<&str>,
    outcome: Outcome,
    duration_ms: i64,
) -> ApiResult<()> {
    let id = Ulid::new().to_string();
    let now = Utc::now().to_rfc3339();
    let (outcome_str, error) = match outcome {
        Outcome::Ok => ("ok", None),
        Outcome::Error(e) => ("error", Some(e)),
    };

    sqlx::query(
        "INSERT INTO api_audit \
         (id, occurred_at, actor, actor_id, domain, operation, target, args_json, outcome, error, duration_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(now)
    .bind(ctx.actor.kind())
    .bind(ctx.actor.id())
    .bind(domain)
    .bind(operation)
    .bind(target)
    .bind(args_json)
    .bind(outcome_str)
    .bind(error)
    .bind(duration_ms)
    .execute(&ctx.db)
    .await?;
    Ok(())
}
