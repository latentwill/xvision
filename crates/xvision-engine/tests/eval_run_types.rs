use xvision_engine::eval::{MetricsSummary, Run, RunMode, RunStatus};

#[test]
fn run_new_queued_starts_in_queued_state() {
    let r = Run::new_queued(
        "strategy-hash-x".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    assert_eq!(r.agent_id, "strategy-hash-x");
    assert_eq!(r.scenario_id, "crypto-bull-q1-2025");
    assert_eq!(r.mode, RunMode::Backtest);
    assert_eq!(r.status, RunStatus::Queued);
    assert!(r.completed_at.is_none());
    assert!(r.metrics.is_none());
    assert!(r.error.is_none());
}

#[test]
fn run_new_queued_id_is_a_ulid() {
    let r = Run::new_queued("h".into(), "s".into(), RunMode::Backtest);
    // ULIDs are 26 chars Crockford base32, monotonic by encoded prefix.
    assert_eq!(r.id.len(), 26, "ULID is 26 chars");
    assert!(
        ulid::Ulid::from_string(&r.id).is_ok(),
        "run ID should parse as a ULID"
    );
}

#[test]
fn run_status_round_trips_for_every_variant() {
    assert_eq!(serde_json::to_string(&RunStatus::Queued).unwrap(), "\"queued\"");
    assert_eq!(serde_json::to_string(&RunStatus::Running).unwrap(), "\"running\"");
    assert_eq!(
        serde_json::to_string(&RunStatus::Completed).unwrap(),
        "\"completed\""
    );
    assert_eq!(serde_json::to_string(&RunStatus::Failed).unwrap(), "\"failed\"");
    assert_eq!(
        serde_json::to_string(&RunStatus::Cancelled).unwrap(),
        "\"cancelled\""
    );

    for status in [
        RunStatus::Queued,
        RunStatus::Running,
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Cancelled,
    ] {
        let s = serde_json::to_string(&status).unwrap();
        let back: RunStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, status, "round-trip failed for {status:?}");
    }
}

#[test]
fn run_mode_round_trips() {
    assert_eq!(serde_json::to_string(&RunMode::Backtest).unwrap(), "\"backtest\"");
    assert_eq!(serde_json::to_string(&RunMode::Forward).unwrap(), "\"fwd\"");

    for mode in [RunMode::Backtest, RunMode::Forward] {
        let s = serde_json::to_string(&mode).unwrap();
        let back: RunMode = serde_json::from_str(&s).unwrap();
        assert_eq!(back, mode);
    }
}

#[test]
fn metrics_summary_round_trips() {
    let m = MetricsSummary {
        total_return_pct: 12.5,
        sharpe: 1.42,
        max_drawdown_pct: -8.3,
        win_rate: 0.58,
        n_trades: 17,
        n_decisions: 42,
        baselines: None,
        ..Default::default()
    };
    let s = serde_json::to_string(&m).unwrap();
    let back: MetricsSummary = serde_json::from_str(&s).unwrap();
    assert_eq!(back, m);
}
