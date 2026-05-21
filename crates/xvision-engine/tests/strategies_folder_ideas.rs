//! Integration tests for `xvision_engine::strategies_folder::ideas`.
//!
//! Coverage matches the contract acceptance list at
//! `team/contracts/strategy-ideas-tool-surface.md`:
//! - Happy-path filter by category.
//! - Filter by indicator.
//! - Empty-library handling (no `library/templates/` directory).
//! - Malformed JSON skips that entry while valid entries still return.
//! - Limit clamping (`limit=500` is capped at `MAX_LIMIT=100`).

use std::path::Path;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use tempfile::TempDir;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::strategies_folder::folder_root;
use xvision_engine::strategies_folder::ideas::{list_ideas, IdeaFilter, MAX_LIMIT};

/// Build a fresh `ApiContext` backed by an empty sqlite file and a
/// fresh tempdir for `xvn_home`. The ideas surface only touches the
/// filesystem under `<xvn_home>/strategies/`, so we skip migrations
/// (mirrors `strategies_folder.rs`'s test fixture).
async fn fresh_ctx() -> (ApiContext, TempDir) {
    let td = tempfile::tempdir().unwrap();
    let db_path = td.path().join("xvn.db");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await.unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "ideas-test".into(),
        },
        td.path().to_path_buf(),
    );
    (ctx, td)
}

/// Helper to write a template JSON under
/// `<xvn_home>/strategies/library/templates/<category>/<filename>`.
async fn write_template(xvn_home: &Path, category: &str, filename: &str, body: &str) {
    let dir = folder_root(xvn_home)
        .join("library")
        .join("templates")
        .join(category);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join(filename), body).await.unwrap();
}

fn ema_pullback_template() -> &'static str {
    r#"{
  "name": "ema_pullback_bounce",
  "display_name": "EMA Pullback Bounce",
  "family": "EMA",
  "schema_version": "xvision.strategy_template.v1",
  "required_tools": ["ohlcv", "indicator_panel"],
  "plain_summary": "Pullback to fast EMA in established trend.",
  "sections": {
    "thesis": "In an established trend, price pulls back to a fast EMA and bounces.",
    "inputs": "- `IndicatorPanel.ema_21`\n- `IndicatorPanel.ema_200`\n- `IndicatorPanel.atr_14`"
  }
}"#
}

fn ema_golden_cross_template() -> &'static str {
    r#"{
  "name": "ema_50_200_golden_cross",
  "display_name": "EMA 50/200 Golden Cross",
  "family": "EMA",
  "schema_version": "xvision.strategy_template.v1",
  "required_tools": ["ohlcv", "indicator_panel"],
  "plain_summary": "Classic golden / death cross.",
  "sections": {
    "thesis": "EMA 50 crossing 200 confirms a regime change.",
    "inputs": "- `IndicatorPanel.ema_50`\n- `IndicatorPanel.ema_200`"
  }
}"#
}

fn rsi_capitulation_template() -> &'static str {
    r#"{
  "name": "rsi_capitulation_long",
  "display_name": "RSI Capitulation Long",
  "family": "rsi_volume",
  "schema_version": "xvision.strategy_template.v1",
  "required_tools": ["ohlcv", "indicator_panel"],
  "plain_summary": "Deep RSI oversold + volume capitulation reversal.",
  "sections": {
    "thesis": "Extreme RSI oversold with high volume marks capitulation.",
    "inputs": "- `IndicatorPanel.rsi_14`\n- `PriceFrame.volume`"
  }
}"#
}

#[tokio::test]
async fn list_ideas_filters_by_category() {
    let (ctx, td) = fresh_ctx().await;
    write_template(
        td.path(),
        "EMA",
        "ema_pullback_bounce.json",
        ema_pullback_template(),
    )
    .await;
    write_template(
        td.path(),
        "EMA",
        "ema_50_200_golden_cross.json",
        ema_golden_cross_template(),
    )
    .await;
    write_template(
        td.path(),
        "rsi_volume",
        "rsi_capitulation_long.json",
        rsi_capitulation_template(),
    )
    .await;

    // Filter by category=ema → only the two EMA ideas, not the rsi one.
    let ideas = list_ideas(
        &ctx,
        IdeaFilter {
            category: Some("ema".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ideas.len(), 2, "expected 2 ema ideas, got {ideas:?}");
    assert!(
        ideas.iter().all(|i| i.category == "ema"),
        "every row should report category=ema, got {ideas:?}"
    );
    let ids: Vec<&str> = ideas.iter().map(|i| i.id.as_str()).collect();
    assert!(ids.contains(&"ema_pullback_bounce"));
    assert!(ids.contains(&"ema_50_200_golden_cross"));

    // Mixed-case category filter still hits the EMA folder.
    let ideas_upper = list_ideas(
        &ctx,
        IdeaFilter {
            category: Some("EMA".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ideas_upper.len(), 2);
}

#[tokio::test]
async fn list_ideas_filters_by_indicator_substring() {
    let (ctx, td) = fresh_ctx().await;
    write_template(
        td.path(),
        "EMA",
        "ema_pullback_bounce.json",
        ema_pullback_template(),
    )
    .await;
    write_template(
        td.path(),
        "EMA",
        "ema_50_200_golden_cross.json",
        ema_golden_cross_template(),
    )
    .await;
    write_template(
        td.path(),
        "rsi_volume",
        "rsi_capitulation_long.json",
        rsi_capitulation_template(),
    )
    .await;

    // Indicator filter `rsi` should hit only the rsi template.
    let rsi_only = list_ideas(
        &ctx,
        IdeaFilter {
            indicator: Some("rsi".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(rsi_only.len(), 1, "expected 1 rsi idea, got {rsi_only:?}");
    assert_eq!(rsi_only[0].id, "rsi_capitulation_long");
    assert!(
        rsi_only[0].indicators.iter().any(|i| i.contains("rsi")),
        "indicators must surface the rsi token: {:?}",
        rsi_only[0].indicators
    );

    // Indicator filter `ema_200` should hit both EMA templates (both
    // reference IndicatorPanel.ema_200 in `inputs`).
    let ema_200 = list_ideas(
        &ctx,
        IdeaFilter {
            indicator: Some("ema_200".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ema_200.len(), 2, "expected 2 ema_200 ideas, got {ema_200:?}");
    let ids: Vec<&str> = ema_200.iter().map(|i| i.id.as_str()).collect();
    assert!(ids.contains(&"ema_pullback_bounce"));
    assert!(ids.contains(&"ema_50_200_golden_cross"));

    // Case-insensitive: `RSI` matches the same row.
    let upper = list_ideas(
        &ctx,
        IdeaFilter {
            indicator: Some("RSI".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(upper.len(), 1);
}

#[tokio::test]
async fn list_ideas_returns_empty_when_library_missing() {
    let (ctx, _td) = fresh_ctx().await;
    // No `library/templates/` has been created. Should not error.
    let ideas = list_ideas(&ctx, IdeaFilter::default()).await.unwrap();
    assert!(ideas.is_empty(), "expected empty list, got {ideas:?}");
}

#[tokio::test]
async fn list_ideas_skips_malformed_json_but_returns_valid_entries() {
    let (ctx, td) = fresh_ctx().await;
    write_template(
        td.path(),
        "EMA",
        "ema_pullback_bounce.json",
        ema_pullback_template(),
    )
    .await;
    write_template(
        td.path(),
        "EMA",
        "ema_50_200_golden_cross.json",
        ema_golden_cross_template(),
    )
    .await;
    // Drop a malformed JSON file alongside the valid ones. It must not
    // poison the whole call — the two valid entries still come back.
    write_template(
        td.path(),
        "EMA",
        "broken.json",
        "{ this is definitely not valid json",
    )
    .await;
    // And one that's syntactically valid but missing the required
    // `name` field — also gets skipped via the parse path's
    // ApiError::Validation branch.
    write_template(
        td.path(),
        "EMA",
        "no_name.json",
        r#"{ "display_name": "anonymous", "schema_version": "x" }"#,
    )
    .await;

    let ideas = list_ideas(&ctx, IdeaFilter::default()).await.unwrap();
    let ids: Vec<&str> = ideas.iter().map(|i| i.id.as_str()).collect();
    assert!(
        ids.contains(&"ema_pullback_bounce"),
        "valid entry survived the malformed sibling: {ideas:?}"
    );
    assert!(ids.contains(&"ema_50_200_golden_cross"));
    assert_eq!(
        ideas.len(),
        2,
        "exactly the two valid entries should come back, got {ideas:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn list_ideas_skips_symlinked_json_templates() {
    use std::os::unix::fs::symlink;

    let (ctx, td) = fresh_ctx().await;
    let outside = td.path().join("outside-template.json");
    tokio::fs::write(&outside, ema_pullback_template()).await.unwrap();

    let templates_dir = folder_root(td.path())
        .join("library")
        .join("templates")
        .join("EMA");
    tokio::fs::create_dir_all(&templates_dir).await.unwrap();
    symlink(&outside, templates_dir.join("leaked.json")).unwrap();

    let ideas = list_ideas(&ctx, IdeaFilter::default()).await.unwrap();
    assert!(
        ideas.is_empty(),
        "symlinked templates must be skipped, got {ideas:?}"
    );
}

#[tokio::test]
async fn list_ideas_clamps_limit_to_max() {
    let (ctx, td) = fresh_ctx().await;
    // Seed enough valid templates to prove the over-cap request clamps at
    // MAX_LIMIT instead of just returning every available row.
    for i in 0..=MAX_LIMIT {
        let body = format!(
            r#"{{
  "name": "ema_test_{i}",
  "display_name": "EMA Test {i}",
  "family": "EMA",
  "schema_version": "xvision.strategy_template.v1",
  "required_tools": ["indicator_panel"],
  "plain_summary": "test {i}",
  "sections": {{
    "thesis": "test {i}",
    "inputs": "- `IndicatorPanel.ema_21`"
  }}
}}"#,
            i = i
        );
        write_template(td.path(), "EMA", &format!("ema_test_{i}.json"), &body).await;
    }

    // Request 500 → must clamp at MAX_LIMIT (100), leaving the extra
    // on-disk idea out of the response.
    let huge = list_ideas(
        &ctx,
        IdeaFilter {
            limit: Some(500),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(huge.len(), MAX_LIMIT as usize);

    // Request 2 → clamp doesn't kick in but limit is honored.
    let two = list_ideas(
        &ctx,
        IdeaFilter {
            limit: Some(2),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(two.len(), 2);

    // Sanity: MAX_LIMIT is what we documented.
    assert_eq!(MAX_LIMIT, 100);
}
