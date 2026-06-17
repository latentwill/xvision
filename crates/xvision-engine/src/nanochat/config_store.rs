// crates/xvision-engine/src/nanochat/config_store.rs
//
// Operator-settable config for the autoresearch subsystem, stored in the
// `xvn_config` table (migration 069). Keys are "autoresearch.<field>".
// Validation rules are enforced on set; get returns the stored value or the
// documented default.

use chrono::Utc;
use sqlx::SqlitePool;

/// Defaults — these are the single source of truth for initial values.
pub const DEFAULT_MIN_PRECISION_LIFT_PP: f64 = 3.0;
pub const DEFAULT_MAX_PNL_REGRESSION: f64 = 0.0;
pub const DEFAULT_PROMOTION_EPSILON: f64 = 0.01;
pub const DEFAULT_PROMOTION_ACC_FLOOR: f64 = 0.52;
pub const DEFAULT_PROMOTION_MIN_HOLDOUT: i64 = 200;
pub const DEFAULT_MIN_CYCLE_COUNT: i64 = 500;
pub const DEFAULT_TRAIN_WALL_CLOCK_SEC: i64 = 300;
/// Default `price_forward` label threshold (fractional move): > +0.02 → LONG,
/// < -0.02 → SHORT, else NEUTRAL. Matches the price_forward label fixtures.
pub const DEFAULT_PRICE_FORWARD_THRESHOLD: f64 = 0.02;

/// Get a config value by key. Returns `Ok(None)` when the key has never been set
/// (caller uses the documented default).
pub async fn get_config(pool: &SqlitePool, key: &str) -> anyhow::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM xvn_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(v,)| v))
}

/// Set a config value by key. Validates the value per key-specific rules before
/// writing. Returns `Err` if the key is unknown or the value is out of range.
pub async fn set_config(pool: &SqlitePool, key: &str, value: &str) -> anyhow::Result<()> {
    let canonical = validate_config_value(key, value).map_err(|e| anyhow::anyhow!("{e}"))?;
    let updated_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO xvn_config (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(&canonical)
    .bind(&updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Validate `value` for the given `key`. Returns the canonical string form on
/// success (e.g. trimmed numeric string), or an error message.
pub fn validate_config_value(key: &str, value: &str) -> Result<String, String> {
    // The canonical stored form is the original trimmed string.
    // We parse to validate range, but store the operator's own notation
    // so "0.60" round-trips as "0.60" rather than being re-stringified as "0.6".
    let trimmed = value.trim().to_string();
    match key {
        "autoresearch.min_precision_lift_pp" => {
            let v: f64 = trimmed
                .parse()
                .map_err(|_| format!("autoresearch.min_precision_lift_pp must be a number, got {value:?}"))?;
            if v >= 0.0 {
                Ok(trimmed)
            } else {
                Err(format!(
                    "autoresearch.min_precision_lift_pp must be >= 0, got {v}"
                ))
            }
        }
        "autoresearch.max_pnl_regression" => {
            let _v: f64 = trimmed
                .parse()
                .map_err(|_| format!("autoresearch.max_pnl_regression must be a number, got {value:?}"))?;
            Ok(trimmed)
        }
        "autoresearch.promotion_epsilon" => {
            let v: f64 = trimmed
                .parse()
                .map_err(|_| format!("autoresearch.promotion_epsilon must be a number, got {value:?}"))?;
            if v > 0.0 {
                Ok(trimmed)
            } else {
                Err(format!("autoresearch.promotion_epsilon must be > 0, got {v}"))
            }
        }
        "autoresearch.promotion_acc_floor" => {
            let v: f64 = trimmed
                .parse()
                .map_err(|_| format!("autoresearch.promotion_acc_floor must be a number, got {value:?}"))?;
            if v > 0.0 && v <= 1.0 {
                Ok(trimmed)
            } else {
                Err(format!(
                    "autoresearch.promotion_acc_floor must be in range (0.0, 1.0], got {v}"
                ))
            }
        }
        "autoresearch.promotion_min_holdout" => {
            let v: i64 = trimmed.parse().map_err(|_| {
                format!("autoresearch.promotion_min_holdout must be an integer, got {value:?}")
            })?;
            if v > 0 {
                Ok(trimmed)
            } else {
                Err(format!("autoresearch.promotion_min_holdout must be > 0, got {v}"))
            }
        }
        "autoresearch.min_cycle_count" => {
            let v: i64 = trimmed
                .parse()
                .map_err(|_| format!("autoresearch.min_cycle_count must be an integer, got {value:?}"))?;
            if v > 0 {
                Ok(trimmed)
            } else {
                Err(format!("autoresearch.min_cycle_count must be > 0, got {v}"))
            }
        }
        "autoresearch.train_wall_clock_sec" => {
            let v: i64 = trimmed.parse().map_err(|_| {
                format!("autoresearch.train_wall_clock_sec must be an integer, got {value:?}")
            })?;
            if v > 0 {
                Ok(trimmed)
            } else {
                Err(format!("autoresearch.train_wall_clock_sec must be > 0, got {v}"))
            }
        }
        "autoresearch.price_forward_threshold" => {
            let v: f64 = trimmed.parse().map_err(|_| {
                format!("autoresearch.price_forward_threshold must be a number, got {value:?}")
            })?;
            if v > 0.0 {
                Ok(trimmed)
            } else {
                Err(format!(
                    "autoresearch.price_forward_threshold must be > 0, got {v}"
                ))
            }
        }
        other => Err(format!(
            "unknown config key {other:?}; known autoresearch keys: \
             autoresearch.min_precision_lift_pp, autoresearch.max_pnl_regression, \
             autoresearch.promotion_epsilon, autoresearch.promotion_acc_floor, \
             autoresearch.promotion_min_holdout, autoresearch.min_cycle_count, \
             autoresearch.train_wall_clock_sec, autoresearch.price_forward_threshold"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn open_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS xvn_config \
             (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn get_unset_key_returns_none() {
        let pool = open_pool().await;
        let v = get_config(&pool, "autoresearch.promotion_epsilon").await.unwrap();
        assert_eq!(v, None);
    }

    #[tokio::test]
    async fn set_and_read_back_valid_key() {
        let pool = open_pool().await;
        set_config(&pool, "autoresearch.promotion_epsilon", "0.02")
            .await
            .unwrap();
        let v = get_config(&pool, "autoresearch.promotion_epsilon").await.unwrap();
        assert_eq!(v.as_deref(), Some("0.02"));
    }

    #[tokio::test]
    async fn set_overwrites_existing_value() {
        let pool = open_pool().await;
        set_config(&pool, "autoresearch.promotion_acc_floor", "0.55")
            .await
            .unwrap();
        set_config(&pool, "autoresearch.promotion_acc_floor", "0.60")
            .await
            .unwrap();
        let v = get_config(&pool, "autoresearch.promotion_acc_floor")
            .await
            .unwrap();
        assert_eq!(v.as_deref(), Some("0.60"));
    }

    #[tokio::test]
    async fn unknown_key_rejected() {
        let pool = open_pool().await;
        let err = set_config(&pool, "autoresearch.nonexistent_key", "1.0")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown"), "{err}");
    }

    #[tokio::test]
    async fn out_of_range_value_rejected() {
        let pool = open_pool().await;
        // promotion_acc_floor must be in (0.0, 1.0].
        let err = set_config(&pool, "autoresearch.promotion_acc_floor", "-0.5")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("range") || err.to_string().contains("must be"),
            "{err}"
        );
    }

    #[test]
    fn validate_promotion_epsilon_range() {
        assert!(validate_config_value("autoresearch.promotion_epsilon", "0.01").is_ok());
        assert!(validate_config_value("autoresearch.promotion_epsilon", "0.0").is_err());
        assert!(validate_config_value("autoresearch.promotion_epsilon", "-0.01").is_err());
        assert!(validate_config_value("autoresearch.promotion_epsilon", "abc").is_err());
    }

    #[test]
    fn validate_promotion_min_holdout_range() {
        assert!(validate_config_value("autoresearch.promotion_min_holdout", "200").is_ok());
        assert!(validate_config_value("autoresearch.promotion_min_holdout", "0").is_err());
        assert!(validate_config_value("autoresearch.promotion_min_holdout", "-1").is_err());
    }

    #[test]
    fn validate_min_cycle_count_range() {
        assert!(validate_config_value("autoresearch.min_cycle_count", "100").is_ok());
        assert!(validate_config_value("autoresearch.min_cycle_count", "0").is_err());
    }

    #[test]
    fn validate_train_wall_clock_sec_range() {
        assert!(validate_config_value("autoresearch.train_wall_clock_sec", "300").is_ok());
        assert!(validate_config_value("autoresearch.train_wall_clock_sec", "0").is_err());
        assert!(validate_config_value("autoresearch.train_wall_clock_sec", "-1").is_err());
    }

    #[test]
    fn validate_price_forward_threshold_range() {
        assert!(validate_config_value("autoresearch.price_forward_threshold", "0.02").is_ok());
        assert!(validate_config_value("autoresearch.price_forward_threshold", "0.0").is_err());
        assert!(validate_config_value("autoresearch.price_forward_threshold", "-0.01").is_err());
        assert!(validate_config_value("autoresearch.price_forward_threshold", "abc").is_err());
    }

    #[test]
    fn validate_min_precision_lift_pp_range() {
        // Rule: >= 0 (zero is allowed).
        assert!(validate_config_value("autoresearch.min_precision_lift_pp", "0").is_ok());
        assert!(validate_config_value("autoresearch.min_precision_lift_pp", "3.0").is_ok());
        assert!(validate_config_value("autoresearch.min_precision_lift_pp", "-1").is_err());
        assert!(validate_config_value("autoresearch.min_precision_lift_pp", "abc").is_err());
    }

    #[test]
    fn validate_promotion_acc_floor_boundaries() {
        // Rule: (0.0, 1.0] — reject 0.0, accept 1.0, reject above 1.0.
        assert!(validate_config_value("autoresearch.promotion_acc_floor", "0.0").is_err());
        assert!(validate_config_value("autoresearch.promotion_acc_floor", "1.0").is_ok());
        assert!(validate_config_value("autoresearch.promotion_acc_floor", "1.5").is_err());
    }
}
