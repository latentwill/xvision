//! Tests for the Phase 3.C findings extractor + persistence.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::findings::extractor::extract_findings;
use xvision_engine::eval::findings::{Finding, Severity};
use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus, RunStore};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn finalized_run() -> Run {
    let mut r = Run::new_queued(
        "bundle-h".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    r.status = RunStatus::Completed;
    r.completed_at = Some(Utc.with_ymd_and_hms(2025, 4, 1, 12, 0, 0).unwrap());
    r.metrics = Some(MetricsSummary {
        total_return_pct: -3.2,
        sharpe: -0.4,
        max_drawdown_pct: 18.0,
        win_rate: 0.41,
        n_trades: 12,
        n_decisions: 30,
    });
    r
}

#[test]
fn severity_serializes_snake_case() {
    assert_eq!(serde_json::to_string(&Severity::Info).unwrap(), "\"info\"");
    assert_eq!(
        serde_json::to_string(&Severity::Warning).unwrap(),
        "\"warning\""
    );
    assert_eq!(
        serde_json::to_string(&Severity::Critical).unwrap(),
        "\"critical\""
    );
}

#[test]
fn severity_round_trips_for_every_variant() {
    for sev in [Severity::Info, Severity::Warning, Severity::Critical] {
        let s = serde_json::to_string(&sev).unwrap();
        let back: Severity = serde_json::from_str(&s).unwrap();
        assert_eq!(back, sev);
    }
}

#[tokio::test]
async fn extract_findings_parses_clean_json_array() {
    let canned = r#"[
        {"kind":"underperformance","severity":"warning","summary":"Total return below baseline","evidence":{"metric_name":"total_return_pct","value":-3.2}},
        {"kind":"drawdown_concentration","severity":"critical","summary":"18% drawdown in flat regime","evidence":{"metric_name":"max_drawdown_pct","value":18.0}}
    ]"#;
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let run = finalized_run();
    let summary = serde_json::json!({"long_open_count": 6, "flat_count": 24});
    let equity = serde_json::json!({"start": 10000.0, "end": 9680.0});

    let findings = extract_findings(
        &run,
        summary,
        equity,
        dispatch,
        "claude-haiku-4-5-20251001",
    )
    .await
    .expect("extraction must succeed");

    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].kind, "underperformance");
    assert_eq!(findings[0].severity, Severity::Warning);
    assert_eq!(findings[1].kind, "drawdown_concentration");
    assert_eq!(findings[1].severity, Severity::Critical);
    // Each finding must carry the run id.
    for f in &findings {
        assert_eq!(f.run_id, run.id);
        assert_eq!(f.schema_version, "1");
        assert!(!f.id.is_empty());
    }
}

#[tokio::test]
async fn extract_findings_empty_array_returns_empty_vec() {
    let canned = "[]";
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let findings = extract_findings(
        &finalized_run(),
        serde_json::json!({}),
        serde_json::json!({}),
        dispatch,
        "claude-haiku-4-5-20251001",
    )
    .await
    .unwrap();
    assert!(findings.is_empty());
}

#[tokio::test]
async fn extract_findings_strips_prose_around_json_array() {
    // Real models sometimes wrap the JSON in prose despite "ONLY the JSON
    // array" instruction. The extractor must locate the array in the text.
    let canned = r#"Sure! Here are the findings:
[
  {"kind":"overtrading","severity":"info","summary":"30 decisions in 4h","evidence":{"metric_name":"n_decisions","value":30}}
]
Hope this helps."#;
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let findings = extract_findings(
        &finalized_run(),
        serde_json::json!({}),
        serde_json::json!({}),
        dispatch,
        "claude-haiku-4-5-20251001",
    )
    .await
    .unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, "overtrading");
}

#[tokio::test]
async fn extract_findings_unparseable_json_returns_error() {
    // We surface parse errors to the caller rather than swallowing them —
    // the caller can decide whether to retry with a different model or
    // record a "no findings extracted" sentinel row.
    let canned = "definitely not a json array";
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let result = extract_findings(
        &finalized_run(),
        serde_json::json!({}),
        serde_json::json!({}),
        dispatch,
        "claude-haiku-4-5-20251001",
    )
    .await;
    assert!(result.is_err(), "unparseable LLM output should error");
}

#[tokio::test]
async fn run_store_record_finding_and_read_findings_round_trip() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);

    let run = finalized_run();
    store.create(&run).await.unwrap();
    store
        .finalize(&run.id, run.metrics.as_ref().unwrap())
        .await
        .unwrap();

    let f1 = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: "underperformance".into(),
        severity: Severity::Warning,
        summary: "below baseline".into(),
        evidence: serde_json::json!({"value": -3.2}),
        extracted_at: Utc::now(),
        schema_version: "1".into(),
    };
    let f2 = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run.id.clone(),
        kind: "drawdown_concentration".into(),
        severity: Severity::Critical,
        summary: "18% in calm regime".into(),
        evidence: serde_json::json!({"value": 18.0}),
        extracted_at: Utc::now(),
        schema_version: "1".into(),
    };
    store.record_finding(&f1).await.unwrap();
    store.record_finding(&f2).await.unwrap();

    let read = store.read_findings(&run.id).await.unwrap();
    assert_eq!(read.len(), 2);
    let kinds: Vec<&str> = read.iter().map(|f| f.kind.as_str()).collect();
    assert!(kinds.contains(&"underperformance"));
    assert!(kinds.contains(&"drawdown_concentration"));
}

#[tokio::test]
async fn run_store_read_findings_empty_for_unknown_run() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let read = store.read_findings("does-not-exist").await.unwrap();
    assert!(read.is_empty());
}

#[test]
fn finding_serde_round_trip() {
    let f = Finding {
        id: "01TEST".into(),
        run_id: "01RUN".into(),
        kind: "tail_risk".into(),
        severity: Severity::Critical,
        summary: "fat tail".into(),
        evidence: serde_json::json!({"sigma": 4.2}),
        extracted_at: Utc.with_ymd_and_hms(2025, 4, 1, 12, 0, 0).unwrap(),
        schema_version: "1".into(),
    };
    let json = serde_json::to_string(&f).unwrap();
    let back: Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, f.id);
    assert_eq!(back.severity, Severity::Critical);
    assert_eq!(back.evidence, f.evidence);
}
