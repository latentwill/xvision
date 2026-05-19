use chrono::{TimeZone, Utc};
use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::{DecisionRow, Run, RunMode, RunStore};

struct TestCtx {
    ctx: ApiContext,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for TestCtx {
    type Target = ApiContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

async fn test_ctx() -> TestCtx {
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "chart-hold-marker-test".into(),
        },
    )
    .await
    .unwrap();
    TestCtx { ctx, _dir: dir }
}

async fn seed_cached_bars(ctx: &ApiContext, cache_key: &str, asset: &str, count: usize) {
    let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let mut blob = Vec::new();
    for i in 0..count {
        let ts = start + chrono::Duration::hours(i as i64);
        let base = 100.0 + i as f64;
        let line = serde_json::json!({
            "t": ts.to_rfc3339(),
            "o": base,
            "h": base + 2.0,
            "l": base - 1.0,
            "c": base + 1.0,
            "v": 1_000.0 + i as f64,
        });
        blob.extend(serde_json::to_vec(&line).unwrap());
        blob.push(b'\n');
    }

    sqlx::query(
        "INSERT OR REPLACE INTO bars_cache \
         (cache_key, asset, granularity, window_start, window_end, \
          data_source, fetched_at, bar_count, bars_blob, compression) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(cache_key)
    .bind(asset)
    .bind("1Hour")
    .bind(start.to_rfc3339())
    .bind((start + chrono::Duration::hours(count as i64)).to_rfc3339())
    .bind("alpaca-historical-v1")
    .bind("2026-05-14T00:00:00Z")
    .bind(count as i64)
    .bind(blob)
    .bind("none")
    .execute(&ctx.db)
    .await
    .unwrap();
}

fn hold_decision(run_id: &str, decision_index: u32, minutes_after_start: i64) -> DecisionRow {
    DecisionRow {
        run_id: run_id.into(),
        decision_index,
        timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()
            + chrono::Duration::minutes(minutes_after_start),
        asset: "BTC/USD".into(),
        action: "hold".into(),
        conviction: Some(0.55),
        justification: Some(format!("hold {decision_index}")),
        reasoning: None,
        order_size: None,
        fill_price: None,
        fill_size: None,
        fee: None,
        pnl_realized: None,
    }
}

#[tokio::test]
async fn hold_marker_with_missing_bar_timestamp_is_skipped_not_zero_priced() {
    let ctx = test_ctx().await;
    let scenario = xvision_engine::api::scenario::get(&ctx, "crypto-bull-q1-2025")
        .await
        .unwrap();
    seed_cached_bars(
        &ctx,
        &scenario.bar_cache_policy.cache_key,
        &scenario.asset[0].venue_symbol,
        3,
    )
    .await;

    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(
        "chart-hold-marker-agent".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    store
        .record_decision(&hold_decision(&run.id, 0, 0))
        .await
        .unwrap();
    store
        .record_decision(&hold_decision(&run.id, 1, 30))
        .await
        .unwrap();

    let payload = xvision_engine::api::chart::build_run_payload(&ctx, &run.id)
        .await
        .unwrap();

    assert_eq!(
        payload.markers.holds.len(),
        1,
        "only the hold decision aligned to a cached bar should render",
    );
    assert_eq!(payload.markers.holds[0].decision_index, 0);
    assert_eq!(payload.markers.holds[0].price, 101.0);
    assert!(
        payload.markers.holds.iter().all(|marker| marker.price != 0.0),
        "missing bar lookups must not produce zero-priced hold markers",
    );
}
