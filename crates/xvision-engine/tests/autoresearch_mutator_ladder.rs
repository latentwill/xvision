use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::mutator_ladder::{
    compute_ladder, record_outcome, record_proposal, MutatorScore,
};

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/048_autoresearch.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/050_mutator_attribution.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn record_proposal_compute_ladder_round_trip() {
    let pool = fresh_pool().await;
    let hash = ContentHash::of_bytes(b"bundle-rt");
    record_proposal(&pool, &hash, "openai", "gpt-4o", "v1")
        .await
        .unwrap();

    let since = Utc::now() - Duration::hours(1);
    let ladder = compute_ladder(&pool, since).await.unwrap();

    assert_eq!(ladder.len(), 1);
    assert_eq!(ladder[0].provider, "openai");
    assert_eq!(ladder[0].model, "gpt-4o");
    assert_eq!(ladder[0].prompt_version, "v1");
    assert_eq!(ladder[0].proposals, 1);
    assert_eq!(ladder[0].accepted, 0);
    assert_eq!(ladder[0].rejected_overfit, 0);
}

#[tokio::test]
async fn ladder_sorted_by_avg_delta_sharpe() {
    let pool = fresh_pool().await;
    let hash_a = ContentHash::of_bytes(b"bundle-low");
    let hash_b = ContentHash::of_bytes(b"bundle-high");
    let now_str = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO lineage_nodes \
         (bundle_hash, gate_verdict, status, created_at) VALUES (?, 'passed', 'active', ?)",
    )
    .bind(hash_a.to_hex())
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO lineage_nodes \
         (bundle_hash, gate_verdict, status, created_at) VALUES (?, 'passed', 'active', ?)",
    )
    .bind(hash_b.to_hex())
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    record_proposal(&pool, &hash_a, "openai", "gpt-4o", "v1")
        .await
        .unwrap();
    record_proposal(&pool, &hash_b, "anthropic", "claude-3", "v1")
        .await
        .unwrap();

    record_outcome(&pool, &hash_a, 0.5).await.unwrap();
    record_outcome(&pool, &hash_b, 0.9).await.unwrap();

    let since = Utc::now() - Duration::hours(1);
    let ladder = compute_ladder(&pool, since).await.unwrap();

    assert_eq!(ladder.len(), 2);
    assert!(
        ladder[0].avg_delta_sharpe > ladder[1].avg_delta_sharpe,
        "ladder must be sorted descending by avg_delta_sharpe"
    );
    assert_eq!(ladder[0].provider, "anthropic");
    assert_eq!(ladder[0].accepted, 1);
}

#[test]
fn acceptance_rate_zero_proposals() {
    let score = MutatorScore {
        provider: "p".into(),
        model: "m".into(),
        prompt_version: "v".into(),
        proposals: 0,
        accepted: 0,
        rejected_overfit: 0,
        avg_delta_sharpe: 0.0,
    };
    assert_eq!(score.acceptance_rate(), 0.0);
}

#[tokio::test]
async fn ladder_since_filter_excludes_old() {
    let pool = fresh_pool().await;
    let hash_old = ContentHash::of_bytes(b"bundle-old");
    let hash_new = ContentHash::of_bytes(b"bundle-new");

    let old_time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap().to_rfc3339();
    let new_time = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap().to_rfc3339();
    let since = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();

    sqlx::query(
        "INSERT INTO mutator_attribution \
         (bundle_hash, provider, model, prompt_version, proposed_at) VALUES (?, 'p1', 'm1', 'v1', ?)",
    )
    .bind(hash_old.to_hex())
    .bind(&old_time)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO mutator_attribution \
         (bundle_hash, provider, model, prompt_version, proposed_at) VALUES (?, 'p2', 'm2', 'v1', ?)",
    )
    .bind(hash_new.to_hex())
    .bind(&new_time)
    .execute(&pool)
    .await
    .unwrap();

    let ladder = compute_ladder(&pool, since).await.unwrap();

    assert_eq!(ladder.len(), 1, "only proposals after 'since' should appear");
    assert_eq!(ladder[0].provider, "p2");
}
