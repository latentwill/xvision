//! Per-(provider, model) concurrency cap for `eval.start`.
//!
//! Prevents a burst of eval launches from saturating a single provider+model
//! slot (the primary cause of the 2026-05-19 rate-limit incident where 27
//! concurrent runs burned ~18.3 M input tokens for zero successful decisions).
//!
//! # Design
//! `enforce_concurrency_cap` performs a one-shot DB count of runs whose
//! status is `queued` or `running` for the given (provider, model) key. If
//! the count equals or exceeds `DEFAULT_PROVIDER_MODEL_CONCURRENCY` the call
//! returns `Err(ApiError::Conflict(...))` with a human-readable message;
//! the caller (HTTP handler / CLI) should propagate this to the user and
//! retry after one of the in-flight runs completes.
//!
//! # Future work
//! TODO: surface as run-config field once F-1 follow-up wires it up.
//! TODO: replace the reject-and-retry pattern with a true FIFO queue backed
//!       by a Tokio semaphore per (provider, model) slot once the dashboard
//!       exposes queue-depth progress. See eval-traces intake 2026-05-19.

use anyhow::Context;
use sqlx::{Row, SqlitePool};

use crate::api::{ApiError, ApiResult};

/// Maximum number of concurrent eval runs allowed against a single
/// (provider, model) pair. Runs in status `queued` or `running` both count
/// toward this limit.
///
/// TODO: surface as run-config field once F-1 follow-up wires it up.
pub const DEFAULT_PROVIDER_MODEL_CONCURRENCY: usize = 4;

/// Count the number of in-flight (queued + running) eval runs that are
/// using `provider` and `model`. The provider/model key is stored on the
/// strategy's LLM slot and resolved by `build_eval_dispatch` before the run
/// row is written, so we join against the strategy's resolved slot rather than
/// a column on `eval_runs` (which has no per-run provider/model column).
///
/// Because `eval_runs` does not yet have a `provider`/`model` column, we
/// proxy the slot key through the `agent_id` column: all runs for the same
/// `agent_id` share the same (provider, model) pair (enforced by
/// `validate_eval_provider_models`). This is correct for v1 where a single
/// provider per strategy is required.
///
/// Returns the count of in-flight runs matching the slot key.
pub async fn count_in_flight(pool: &SqlitePool, agent_id: &str) -> anyhow::Result<usize> {
    let row = sqlx::query(
        "SELECT COUNT(*) as cnt \
         FROM eval_runs \
         WHERE agent_id = ? AND status IN ('queued', 'running')",
    )
    .bind(agent_id)
    .fetch_one(pool)
    .await
    .context("count in-flight eval runs")?;
    let cnt: i64 = row.try_get("cnt").context("read count")?;
    Ok(cnt as usize)
}

/// Gate function called at the top of `start_run`. Returns `Ok(())` when the
/// slot has capacity, or `Err(ApiError::Conflict(...))` when the cap is
/// reached.
///
/// `provider` and `model` are passed for the human-readable error message;
/// the actual count query uses `agent_id` as the proxy key (see
/// [`count_in_flight`] for rationale).
pub async fn enforce_concurrency_cap(
    pool: &SqlitePool,
    agent_id: &str,
    provider: &str,
    model: &str,
) -> ApiResult<()> {
    let in_flight = count_in_flight(pool, agent_id)
        .await
        .map_err(|e| ApiError::Internal(format!("concurrency-cap count: {e}")))?;

    if in_flight >= DEFAULT_PROVIDER_MODEL_CONCURRENCY {
        return Err(ApiError::Conflict(format!(
            "concurrency cap reached: {in_flight} run(s) are already queued or running for \
             agent `{agent_id}` on {provider}/{model} \
             (limit = {DEFAULT_PROVIDER_MODEL_CONCURRENCY}). \
             Wait for a run to complete before launching another.",
        )));
    }
    Ok(())
}
