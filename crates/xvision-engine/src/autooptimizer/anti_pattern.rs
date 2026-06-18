//! Anti-pattern memory: when the same finding recurs across ≥ 3 cycles,
//! promote it to a preflight blockade. Modeled on the AutoResearch self-play
//! paper's "baked the lesson into operating constraints" pattern (Chen 2026).

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

/// A recurring failure pattern observed across optimizer cycles.
#[derive(Debug, Clone)]
pub struct AntiPattern {
    /// Content-hash of the finding (code + canonicalized summary).
    pub pattern_hash: String,
    /// Human-readable description from first occurrence.
    pub description: String,
    /// Finding code (e.g. "SIMPLICITY", "REGIME_DEGRADED", "JUDGE_MISMATCH").
    pub code: String,
    /// How many cycles have produced this finding.
    pub occurrence_count: u64,
    /// When first observed.
    pub first_seen: DateTime<Utc>,
    /// When last observed.
    pub last_seen: DateTime<Utc>,
    /// Promoted to preflight blockade (≥ 3 occurrences).
    pub auto_reject: bool,
}

/// Ensure the `autooptimizer_anti_patterns` table exists (migration 058).
pub async fn ensure_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_anti_patterns (
            pattern_hash TEXT PRIMARY KEY,
            description TEXT NOT NULL DEFAULT '',
            code TEXT NOT NULL DEFAULT '',
            occurrence_count INTEGER NOT NULL DEFAULT 1,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            auto_reject INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a finding in the anti-pattern registry. If the same pattern has
/// been seen ≥ 3 times, it is promoted to `auto_reject = true`.
pub async fn record_finding(
    pool: &SqlitePool,
    code: &str,
    summary: &str,
) -> anyhow::Result<()> {
    let hash = hash_finding(code, summary);
    let now = Utc::now().to_rfc3339();

    let existing: Option<(u64, bool)> = sqlx::query_as(
        "SELECT occurrence_count, auto_reject FROM autooptimizer_anti_patterns WHERE pattern_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await?
    .map(|(c, ar): (i64, bool)| (c as u64, ar));

    match existing {
        Some((count, auto_reject)) => {
            let new_count = count.saturating_add(1);
            let promote = !auto_reject && new_count >= 3;
            sqlx::query(
                "UPDATE autooptimizer_anti_patterns SET \
                 occurrence_count = ?, last_seen = ?, \
                 auto_reject = MAX(auto_reject, ?) \
                 WHERE pattern_hash = ?",
            )
            .bind(new_count as i64)
            .bind(&now)
            .bind(promote as i32)
            .bind(&hash)
            .execute(pool)
            .await?;
            if promote {
                tracing::warn!(
                    pattern_hash = %hash,
                    code,
                    occurrences = new_count,
                    "anti-pattern promoted to auto-reject blockade"
                );
            }
        }
        None => {
            sqlx::query(
                "INSERT INTO autooptimizer_anti_patterns \
                 (pattern_hash, description, code, occurrence_count, first_seen, last_seen, auto_reject) \
                 VALUES (?, ?, ?, 1, ?, ?, 0)",
            )
            .bind(&hash)
            .bind(summary)
            .bind(code)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Query all anti-patterns that have been promoted to `auto_reject`.
pub async fn load_auto_reject_patterns(pool: &SqlitePool) -> anyhow::Result<Vec<AntiPattern>> {
    let rows = sqlx::query_as::<_, (String, String, String, i64, String, String, bool)>(
        "SELECT pattern_hash, description, code, occurrence_count, \
         first_seen, last_seen, auto_reject \
         FROM autooptimizer_anti_patterns WHERE auto_reject = 1 \
         ORDER BY occurrence_count DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(h, desc, code, cnt, fs, ls, ar)| AntiPattern {
            pattern_hash: h,
            description: desc,
            code,
            occurrence_count: cnt as u64,
            first_seen: DateTime::parse_from_rfc3339(&fs)
                .unwrap_or_default()
                .with_timezone(&Utc),
            last_seen: DateTime::parse_from_rfc3339(&ls)
                .unwrap_or_default()
                .with_timezone(&Utc),
            auto_reject: ar,
        })
        .collect())
}

/// Simple content hash: combine code + canonicalized summary into a stable hash.
fn hash_finding(code: &str, summary: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut h);
    // Canonicalize: lowercase, trim, collapse whitespace
    let canonical = summary.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    canonical.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        ensure_schema(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn first_occurrence_inserts() {
        let pool = test_pool().await;
        record_finding(&pool, "TEST", "something broke").await.unwrap();
        let all = load_auto_reject_patterns(&pool).await.unwrap();
        assert!(all.is_empty(), "first occurrence not promoted");
    }

    #[tokio::test]
    async fn third_occurrence_promotes() {
        let pool = test_pool().await;
        for _ in 0..3 {
            record_finding(&pool, "SIMPLICITY", "parameter explosion").await.unwrap();
        }
        let all = load_auto_reject_patterns(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].auto_reject);
        assert_eq!(all[0].occurrence_count, 3);
    }

    #[tokio::test]
    async fn hash_is_stable_for_same_input() {
        let h1 = hash_finding("SIMPLICITY", "parameter explosion");
        let h2 = hash_finding("SIMPLICITY", "parameter explosion");
        assert_eq!(h1, h2);
    }
}
