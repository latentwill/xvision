use sqlx::SqlitePool;
use xvision_engine::autooptimizer::{
    config::{AutoOptimizerConfig, LooseningSchedule},
    content_hash::hash_canonical_json,
    cycle_loosen::effective_min_improvement_for_cycle,
};

fn make_config(thresholds: Vec<f64>) -> AutoOptimizerConfig {
    AutoOptimizerConfig {
        min_improvement: 0.10,
        loosening_schedule: Some(LooseningSchedule {
            day_n_thresholds: thresholds,
        }),
        ..AutoOptimizerConfig::default()
    }
}

async fn mem_pool() -> SqlitePool {
    SqlitePool::connect(":memory:").await.unwrap()
}

#[tokio::test]
async fn sustained_zero_returns_base() {
    let config = make_config(vec![0.07, 0.05]);
    let pool = mem_pool().await;
    let r = effective_min_improvement_for_cycle(&pool, &config, 0, 0)
        .await
        .unwrap();
    assert_eq!(r.effective_min_improvement, config.min_improvement);
    assert_eq!(r.loosening_steps_applied, 0);
}

#[tokio::test]
async fn sustained_n_returns_nth_schedule_entry() {
    let config = make_config(vec![0.07, 0.05, 0.03]);
    let pool = mem_pool().await;

    let r1 = effective_min_improvement_for_cycle(&pool, &config, 0, 1)
        .await
        .unwrap();
    assert!((r1.effective_min_improvement - 0.07).abs() < f64::EPSILON);
    assert_eq!(r1.loosening_steps_applied, 1);

    let r2 = effective_min_improvement_for_cycle(&pool, &config, 0, 2)
        .await
        .unwrap();
    assert!((r2.effective_min_improvement - 0.05).abs() < f64::EPSILON);
    assert_eq!(r2.loosening_steps_applied, 2);
}

#[tokio::test]
async fn sustained_beyond_schedule_clamps_to_last_entry() {
    let config = make_config(vec![0.07, 0.05]);
    let pool = mem_pool().await;
    let r = effective_min_improvement_for_cycle(&pool, &config, 0, 99)
        .await
        .unwrap();
    assert!((r.effective_min_improvement - 0.05).abs() < f64::EPSILON);
    assert_eq!(r.loosening_steps_applied, 2);
}

#[tokio::test]
async fn schedule_hash_is_deterministic_and_matches_canonical() {
    let config = make_config(vec![0.07, 0.05]);
    let pool = mem_pool().await;
    let r = effective_min_improvement_for_cycle(&pool, &config, 0, 1)
        .await
        .unwrap();
    let expected =
        hash_canonical_json(&serde_json::to_value(config.loosening_schedule.as_ref().unwrap()).unwrap());
    assert_eq!(r.schedule_hash, expected);
}

#[tokio::test]
async fn identical_inputs_produce_identical_effective_gate_config() {
    let config = make_config(vec![0.07, 0.05]);
    let pool = mem_pool().await;
    let r1 = effective_min_improvement_for_cycle(&pool, &config, 5, 2)
        .await
        .unwrap();
    let r2 = effective_min_improvement_for_cycle(&pool, &config, 5, 2)
        .await
        .unwrap();
    assert_eq!(r1.base_min_improvement, r2.base_min_improvement);
    assert_eq!(r1.effective_min_improvement, r2.effective_min_improvement);
    assert_eq!(r1.loosening_steps_applied, r2.loosening_steps_applied);
    assert_eq!(r1.schedule_hash, r2.schedule_hash);
}
