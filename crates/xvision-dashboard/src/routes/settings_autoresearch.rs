//! Settings routes for the autoresearch subsystem.
//!
//! - `GET  /api/settings/autoresearch` — read all 8 autoresearch.* config keys
//!   (returns defaults for unset keys). Registered in `readonly_router`.
//! - `POST /api/settings/autoresearch` — write one or more keys (validates
//!   per-key ranges). Registered in `mutating_router + require_auth_middleware`.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use xvision_engine::nanochat::config_store::{
    get_config, set_config, DEFAULT_MAX_PNL_REGRESSION, DEFAULT_MIN_CYCLE_COUNT,
    DEFAULT_MIN_PRECISION_LIFT_PP, DEFAULT_PRICE_FORWARD_THRESHOLD, DEFAULT_PROMOTION_ACC_FLOOR,
    DEFAULT_PROMOTION_EPSILON, DEFAULT_PROMOTION_MIN_HOLDOUT, DEFAULT_TRAIN_WALL_CLOCK_SEC,
};

use crate::error::DashboardError;
use crate::state::AppState;

/// The 8 autoresearch config keys, expressed as their field names in JSON.
/// This is the canonical shape exposed to the frontend (`/api/settings/autoresearch`).
/// Each field maps to the `autoresearch.<field_name>` config key in `xvn_config`.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AutoresearchConfigResponse {
    pub min_precision_lift_pp: f64,
    pub max_pnl_regression: f64,
    pub promotion_epsilon: f64,
    pub promotion_acc_floor: f64,
    pub promotion_min_holdout: i64,
    pub min_cycle_count: i64,
    pub train_wall_clock_sec: i64,
    pub price_forward_threshold: f64,
}

/// GET /api/settings/autoresearch
/// Returns current values for all 8 autoresearch.* keys; falls back to the
/// documented default for any key that has never been set.
pub async fn get_autoresearch_config(
    State(state): State<AppState>,
) -> Result<Json<AutoresearchConfigResponse>, DashboardError> {
    async fn read_f64(pool: &sqlx::SqlitePool, key: &str, default: f64) -> anyhow::Result<f64> {
        match get_config(pool, key).await? {
            Some(v) => Ok(v.parse::<f64>().unwrap_or(default)),
            None => Ok(default),
        }
    }
    async fn read_i64(pool: &sqlx::SqlitePool, key: &str, default: i64) -> anyhow::Result<i64> {
        match get_config(pool, key).await? {
            Some(v) => Ok(v.parse::<i64>().unwrap_or(default)),
            None => Ok(default),
        }
    }

    let cfg = AutoresearchConfigResponse {
        min_precision_lift_pp: read_f64(
            &state.pool,
            "autoresearch.min_precision_lift_pp",
            DEFAULT_MIN_PRECISION_LIFT_PP,
        )
        .await
        .map_err(DashboardError::Internal)?,
        max_pnl_regression: read_f64(
            &state.pool,
            "autoresearch.max_pnl_regression",
            DEFAULT_MAX_PNL_REGRESSION,
        )
        .await
        .map_err(DashboardError::Internal)?,
        promotion_epsilon: read_f64(
            &state.pool,
            "autoresearch.promotion_epsilon",
            DEFAULT_PROMOTION_EPSILON,
        )
        .await
        .map_err(DashboardError::Internal)?,
        promotion_acc_floor: read_f64(
            &state.pool,
            "autoresearch.promotion_acc_floor",
            DEFAULT_PROMOTION_ACC_FLOOR,
        )
        .await
        .map_err(DashboardError::Internal)?,
        promotion_min_holdout: read_i64(
            &state.pool,
            "autoresearch.promotion_min_holdout",
            DEFAULT_PROMOTION_MIN_HOLDOUT,
        )
        .await
        .map_err(DashboardError::Internal)?,
        min_cycle_count: read_i64(
            &state.pool,
            "autoresearch.min_cycle_count",
            DEFAULT_MIN_CYCLE_COUNT,
        )
        .await
        .map_err(DashboardError::Internal)?,
        train_wall_clock_sec: read_i64(
            &state.pool,
            "autoresearch.train_wall_clock_sec",
            DEFAULT_TRAIN_WALL_CLOCK_SEC,
        )
        .await
        .map_err(DashboardError::Internal)?,
        price_forward_threshold: read_f64(
            &state.pool,
            "autoresearch.price_forward_threshold",
            DEFAULT_PRICE_FORWARD_THRESHOLD,
        )
        .await
        .map_err(DashboardError::Internal)?,
    };

    Ok(Json(cfg))
}

/// Partial update body — each field is optional so the caller may send
/// only the keys they want to change. Unknown JSON keys are ignored.
#[derive(Debug, Deserialize, Default)]
pub struct SetAutoresearchConfigBody {
    pub min_precision_lift_pp: Option<f64>,
    pub max_pnl_regression: Option<f64>,
    pub promotion_epsilon: Option<f64>,
    pub promotion_acc_floor: Option<f64>,
    pub promotion_min_holdout: Option<i64>,
    pub min_cycle_count: Option<i64>,
    pub train_wall_clock_sec: Option<i64>,
    pub price_forward_threshold: Option<f64>,
}

/// POST /api/settings/autoresearch
/// Validates and writes only the provided keys. Returns the full config after write.
pub async fn set_autoresearch_config(
    State(state): State<AppState>,
    Json(body): Json<SetAutoresearchConfigBody>,
) -> Result<Json<AutoresearchConfigResponse>, DashboardError> {
    // Validate + write each provided key using the Task 2.4 config store.
    // set_config already enforces numeric ranges; an Err becomes a 400.
    macro_rules! maybe_set {
        ($field:expr, $key:expr) => {
            if let Some(v) = $field {
                set_config(&state.pool, $key, &v.to_string())
                    .await
                    .map_err(|e| DashboardError::Validation {
                        field: $key.to_string(),
                        msg: e.to_string(),
                    })?;
            }
        };
    }

    maybe_set!(body.min_precision_lift_pp, "autoresearch.min_precision_lift_pp");
    maybe_set!(body.max_pnl_regression, "autoresearch.max_pnl_regression");
    maybe_set!(body.promotion_epsilon, "autoresearch.promotion_epsilon");
    maybe_set!(body.promotion_acc_floor, "autoresearch.promotion_acc_floor");
    maybe_set!(body.promotion_min_holdout, "autoresearch.promotion_min_holdout");
    maybe_set!(body.min_cycle_count, "autoresearch.min_cycle_count");
    maybe_set!(body.train_wall_clock_sec, "autoresearch.train_wall_clock_sec");
    maybe_set!(
        body.price_forward_threshold,
        "autoresearch.price_forward_threshold"
    );

    // Return the full config (mirrors the GET response).
    get_autoresearch_config(State(state)).await
}
