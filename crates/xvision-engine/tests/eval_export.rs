mod support;

use xvision_engine::eval::export::build_export;
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

#[tokio::test]
async fn eval_export_includes_route_trace_summary_and_full_events() {
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    let mut run = Run::new_queued(
        "route-strategy".into(),
        "flash-crash-aug-2024".into(),
        RunMode::Backtest,
    );
    run.status = RunStatus::Completed;
    store.create(&run).await.unwrap();
    store
        .update_status(&run.id, RunStatus::Completed, None)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (id, objective, strategy_id, eval_run_id, status, started_at, retention_mode) \
         VALUES (?, 'route trace export', 'route-strategy', ?, 'completed', '2026-07-01T00:00:00Z', 'hash_only')",
    )
    .bind(&run.id)
    .bind(&run.id)
    .execute(&ctx.db)
    .await
    .unwrap();

    let compact = serde_json::json!({
        "event": "route.decision",
        "lane": "backtest",
        "router_role": "router",
        "selected_target_role": "trend_trader",
        "selected_path": ["router", "trend_trader", "risk_reviewer"],
        "skipped_target_roles": ["range_trader"],
        "gated_target_roles": ["range_trader"],
        "summary": "trend regime",
        "final_trader_role": "trend_trader",
        "final_action": "buy",
        "intended_route": { "router_role": "router", "branch_targets": ["trend_trader", "range_trader"] },
        "actual_vs_intended": "matched",
        "lane_label": "backtest"
    });
    let full = serde_json::json!({
        "event": "route.decision.full",
        "route_target_identifiers": [{"role":"trend_trader","agent_id":"trend-agent"}],
        "predicate_results": [],
        "graph_skips": [{"source_role":"router","target_role":"range_trader","reason":"unselected_branch_target"}],
        "token_cost": {"input_tokens": 100, "output_tokens": 20, "cost_usd": 0.001},
        "errors": [],
        "route_definition_hash": "abc123",
        "route_definition": {"router_role":"router","branches":[],"graph_edges":[]}
    });

    for (id, kind, payload) in [
        ("evt-route-compact", "route.decision", compact),
        ("evt-route-full", "route.decision.full", full),
    ] {
        sqlx::query(
            "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
             VALUES (?, ?, NULL, ?, ?, '2026-07-01T00:00:01Z')",
        )
        .bind(id)
        .bind(&run.id)
        .bind(kind)
        .bind(payload.to_string())
        .execute(&ctx.db)
        .await
        .unwrap();
    }

    let export = build_export(&ctx, &run.id).await.unwrap();
    assert_eq!(export.route_trace_summary.len(), 1);
    assert_eq!(
        export.route_trace_summary[0]["selected_path"],
        serde_json::json!(["router", "trend_trader", "risk_reviewer"])
    );
    assert_eq!(export.route_trace_summary[0]["final_trader_role"], "trend_trader");
    assert_eq!(export.route_trace_summary[0]["final_action"], "buy");
    assert_eq!(export.route_trace_summary[0]["actual_vs_intended"], "matched");
    assert_eq!(export.route_trace_full.len(), 1);
    assert_eq!(export.route_trace_full[0]["route_definition_hash"], "abc123");
}
