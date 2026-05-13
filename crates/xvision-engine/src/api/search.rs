//! Command-palette (⌘K) search surface — Plan #12 Phase A indexer hooks +
//! Phase B query wrapper.
//!
//! `search` is the audited query entry point the dashboard's `/api/search`
//! handler calls. The `upsert_*` helpers are called from the existing
//! mutation paths (api::strategy::create_strategy, eval::run finalize) so
//! the index stays current incrementally; `reindex_all` is the cold-start
//! walker the dashboard runs once at startup so a freshly-installed home
//! with pre-existing artifacts becomes searchable on first launch.
//!
//! Indexer failures NEVER bubble to the calling write path. Search hygiene
//! is a UX nicety; data integrity belongs to the underlying stores. We log
//! at `warn!` and move on so a bad index row can't break a strategy save.

use std::time::Instant;

use crate::api::audit::{self, Outcome};
use crate::api::{ApiContext, ApiError, ApiResult};
use crate::strategies::store::{strategy_store_dir, StrategyStore, FilesystemStore};
use crate::strategies::Strategy;
use crate::eval::findings::Finding;
use crate::eval::run::{Run, RunMode, RunStatus};
#[allow(deprecated)]
use crate::eval::scenario::canonical_scenarios;
use crate::eval::store::{ListFilter, RunStore};
use crate::search::{IndexEntry, SearchHit, SearchIndex, SearchKind, SearchQuery};

/// Query the FTS5 index. Empty `q` returns the most-recently-touched
/// artifacts so the palette has something to render the moment it opens.
/// Audited as `search/query` so we can spot pathological queries
/// (very long, very frequent) in the audit log later.
pub async fn search(
    ctx: &ApiContext,
    q: &str,
    opts: &SearchQuery,
) -> ApiResult<Vec<SearchHit>> {
    let started = Instant::now();
    let result = SearchIndex::search(&ctx.db, q, opts)
        .await
        .map_err(|e| ApiError::Internal(format!("search: {e}")));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "search",
        "query",
        None,
        Some(q),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// Upsert a strategy into the index. Best-effort — logs and returns
/// `Ok(())` on failure so the calling create/update path isn't blocked by
/// a transient index write error.
pub async fn upsert_strategy(ctx: &ApiContext, strategy: &Strategy) {
    let entry = strategy_entry(strategy);
    if let Err(e) = SearchIndex::upsert(&ctx.db, &entry).await {
        tracing::warn!(error = %e, agent_id = %strategy.manifest.id, "search index upsert (strategy) failed");
    }
}

/// Drop a strategy from the index. Called when a strategy is deleted.
pub async fn delete_strategy(ctx: &ApiContext, agent_id: &str) {
    if let Err(e) = SearchIndex::delete(&ctx.db, SearchKind::Strategy, agent_id).await {
        tracing::warn!(error = %e, agent_id, "search index delete (strategy) failed");
    }
}

/// Upsert an eval run into the index. Best-effort.
pub async fn upsert_run(ctx: &ApiContext, run: &Run) {
    let entry = run_entry(run);
    if let Err(e) = SearchIndex::upsert(&ctx.db, &entry).await {
        tracing::warn!(error = %e, run_id = %run.id, "search index upsert (run) failed");
    }
}

/// Upsert an eval finding into the index. Best-effort.
///
/// No production callsite calls this yet — `RunStore::record_finding`
/// has no orchestrator wired in v1. The hook is exposed so the future
/// findings-extraction path (Phase 3.C orchestration) can call it as a
/// one-liner when finalizing a finding. The cold-start `reindex_all`
/// walker already picks up any findings persisted directly via tests
/// or a future orchestrator without further coordination.
pub async fn upsert_finding(ctx: &ApiContext, finding: &Finding) {
    let entry = finding_entry(finding);
    if let Err(e) = SearchIndex::upsert(&ctx.db, &entry).await {
        tracing::warn!(error = %e, finding_id = %finding.id, "search index upsert (finding) failed");
    }
}

/// Index every canonical scenario. Scenarios are static at build time so
/// this is just a fixed list iteration — no separate "incremental" hook.
pub async fn upsert_scenarios(ctx: &ApiContext) {
    #[allow(deprecated)]
    let scenarios = canonical_scenarios();
    for s in scenarios {
        let asset_universe: Vec<String> =
            s.asset.iter().map(|a| a.venue_symbol.clone()).collect();
        // Extract legacy regime tags off the new combined `tags` field so the
        // ⌘K palette text stays consistent during the refactor.
        let regime_tags: Vec<String> = s
            .tags
            .iter()
            .filter_map(|t| t.strip_prefix("regime:").map(|r| r.to_string()))
            .collect();
        let entry = IndexEntry {
            artifact_id: s.id.clone(),
            kind: SearchKind::Scenario,
            title: s.display_name.clone(),
            summary: format!(
                "{} · {} days · regimes: {}",
                asset_universe.join(", "),
                (s.time_window.end - s.time_window.start).num_days(),
                regime_tags.join(", ")
            ),
            tags: regime_tags,
            updated_at: chrono::Utc::now(),
            href: format!("/eval-runs?scenario={}", s.id),
        };
        if let Err(e) = SearchIndex::upsert(&ctx.db, &entry).await {
            tracing::warn!(error = %e, scenario_id = %s.id, "search index upsert (scenario) failed");
        }
    }
}

/// Seed the static action list. Idempotent — re-running on every startup
/// just re-upserts the same six rows.
pub async fn seed_actions(ctx: &ApiContext) {
    const ACTIONS: &[(&str, &str, &str, &str)] = &[
        (
            "new-strategy",
            "New strategy from template…",
            "Open the wizard with a template picker",
            "/setup",
        ),
        (
            "new-run",
            "New eval run",
            "Pick a strategy + scenario and run a backtest",
            "/eval-runs",
        ),
        (
            "compare-runs",
            "Compare eval runs",
            "Side-by-side comparison of two or more completed runs",
            "/eval-runs/compare",
        ),
        (
            "settings-providers",
            "LLM providers",
            "Add or edit LLM provider credentials",
            "/settings/providers",
        ),
        (
            "settings-brokers",
            "Brokers",
            "Add or edit broker connections",
            "/settings/brokers",
        ),
        (
            "settings-identity",
            "Identity",
            "View on-chain identity & signing key",
            "/settings/identity",
        ),
    ];
    let now = chrono::Utc::now();
    for (id, title, summary, href) in ACTIONS {
        let entry = IndexEntry {
            artifact_id: (*id).into(),
            kind: SearchKind::Action,
            title: (*title).into(),
            summary: (*summary).into(),
            tags: vec![],
            updated_at: now,
            href: (*href).into(),
        };
        if let Err(e) = SearchIndex::upsert(&ctx.db, &entry).await {
            tracing::warn!(error = %e, action = id, "search index upsert (action) failed");
        }
    }
}

/// Cold-start walker: re-derive every index row from the authoritative
/// stores. Safe to call on a fresh DB and on a populated one — `upsert`
/// is idempotent.
///
/// Best-effort: a failure walking one store does NOT block the next one.
/// The dashboard logs and continues so users always get *some* search
/// surface even if (say) a single strategy file is corrupt.
pub async fn reindex_all(ctx: &ApiContext) {
    // 1. Strategies — walk the filesystem strategy store.
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    match store.list().await {
        Ok(ids) => {
            for id in ids {
                match store.load(&id).await {
                    Ok(strategy) => upsert_strategy(ctx, &strategy).await,
                    Err(e) => tracing::warn!(error = %e, agent_id = %id, "reindex: load strategy failed"),
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "reindex: list strategies failed"),
    }

    // 2. Runs (+ their findings) — paginate via RunStore::list with no
    // filter, then walk per-run findings so the palette can resolve a
    // finding row by keyword even though there's no incremental hook
    // wired into the (not-yet-orchestrated) extraction path.
    let run_store = RunStore::new(ctx.db.clone());
    match run_store.list(ListFilter::default()).await {
        Ok(runs) => {
            for run in runs {
                upsert_run(ctx, &run).await;
                match run_store.read_findings(&run.id).await {
                    Ok(findings) => {
                        for f in findings {
                            upsert_finding(ctx, &f).await;
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, run_id = %run.id, "reindex: read findings failed"),
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "reindex: list runs failed"),
    }

    // 3. Scenarios + actions — small static sets, just re-seed.
    upsert_scenarios(ctx).await;
    seed_actions(ctx).await;
}

fn strategy_entry(strategy: &Strategy) -> IndexEntry {
    let m = &strategy.manifest;
    let summary = if m.plain_summary.is_empty() {
        format!("{} · risk {}", m.template, m.risk_preset_or_config)
    } else {
        format!(
            "{} · risk {} · {}",
            m.template, m.risk_preset_or_config, m.plain_summary
        )
    };
    let mut tags = vec![m.template.clone(), m.risk_preset_or_config.clone()];
    tags.extend(m.asset_universe.iter().cloned());
    for r in &m.regime_fit {
        // RegimeFit serializes via serde rename_all = "snake_case"; reuse
        // its serde representation so search tokens match the on-chain
        // manifest exactly.
        if let Ok(s) = serde_json::to_value(r).and_then(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| serde::de::Error::custom("not a string"))
        }) {
            tags.push(s);
        }
    }
    IndexEntry {
        artifact_id: m.id.clone(),
        kind: SearchKind::Strategy,
        title: if m.display_name.is_empty() {
            m.id.clone()
        } else {
            m.display_name.clone()
        },
        summary,
        tags,
        updated_at: m.published_at.unwrap_or_else(chrono::Utc::now),
        href: format!("/authoring/{}", m.id),
    }
}

fn finding_entry(f: &Finding) -> IndexEntry {
    let id_prefix: String = f.id.chars().take(8).collect();
    let summary = format!(
        "{} · severity {} · run {}",
        f.kind,
        f.severity.as_str(),
        f.run_id.chars().take(8).collect::<String>()
    );
    let title = if f.summary.is_empty() {
        format!("Finding {} ({})", id_prefix, f.kind)
    } else {
        f.summary.clone()
    };
    IndexEntry {
        artifact_id: f.id.clone(),
        kind: SearchKind::Finding,
        title,
        summary,
        tags: vec![f.kind.clone(), f.severity.as_str().to_string()],
        updated_at: f.extracted_at,
        // Findings are surfaced inside their owning run's detail page;
        // a fragment id is the cheapest deep-link until the dashboard
        // grows a dedicated finding view.
        href: format!("/eval-runs/{}#finding-{}", f.run_id, f.id),
    }
}

fn run_entry(run: &Run) -> IndexEntry {
    let title_id = run.id.chars().take(8).collect::<String>();
    let title = format!("Run {} · {}", title_id, run.scenario_id);
    let metrics_str = match &run.metrics {
        Some(m) => format!(
            "sharpe {:.2} · return {:.2}% · max-dd {:.2}%",
            m.sharpe, m.total_return_pct, m.max_drawdown_pct
        ),
        None => "no metrics yet".into(),
    };
    let summary = format!(
        "{} · {} · {}",
        mode_label(run.mode),
        status_label(run.status),
        metrics_str
    );
    let tags = vec![
        run.scenario_id.clone(),
        mode_label(run.mode).to_string(),
        status_label(run.status).to_string(),
    ];
    IndexEntry {
        artifact_id: run.id.clone(),
        kind: SearchKind::Run,
        title,
        summary,
        tags,
        updated_at: run.completed_at.unwrap_or(run.started_at),
        href: format!("/eval-runs/{}", run.id),
    }
}

fn mode_label(m: RunMode) -> &'static str {
    match m {
        RunMode::Paper => "paper",
        RunMode::Backtest => "backtest",
    }
}

fn status_label(s: RunStatus) -> &'static str {
    s.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;

    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "test".into(),
            },
        )
        .await
        .unwrap();
        (ctx, dir)
    }

    #[tokio::test]
    async fn search_empty_returns_seeded_actions() {
        let (ctx, _dir) = fresh_ctx().await;
        seed_actions(&ctx).await;
        let hits = search(&ctx, "", &SearchQuery::default()).await.unwrap();
        assert!(hits.iter().all(|h| h.kind == SearchKind::Action));
        assert!(hits.iter().any(|h| h.artifact_id == "new-strategy"));
    }

    #[tokio::test]
    async fn search_finds_seeded_action_by_keyword() {
        let (ctx, _dir) = fresh_ctx().await;
        seed_actions(&ctx).await;
        let hits = search(&ctx, "broker", &SearchQuery::default())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].artifact_id, "settings-brokers");
    }

    #[tokio::test]
    async fn upsert_scenarios_indexes_canonical_set() {
        let (ctx, _dir) = fresh_ctx().await;
        upsert_scenarios(&ctx).await;
        let hits = search(
            &ctx,
            "",
            &SearchQuery {
                kind: Some(SearchKind::Scenario),
                limit: None,
            },
        )
        .await
        .unwrap();
        #[allow(deprecated)]
        let expected = canonical_scenarios().len();
        assert_eq!(hits.len(), expected);
    }

    #[tokio::test]
    async fn upsert_finding_indexes_by_summary_and_kind() {
        use crate::eval::findings::{Finding, Severity};

        let (ctx, _dir) = fresh_ctx().await;
        let f = Finding {
            id: "01F1NDING0000000000000000".into(),
            run_id: "01RUN0000000000000000000".into(),
            kind: "drawdown_concentration".into(),
            severity: Severity::Warning,
            summary: "Two of the worst three drawdowns happened in March 2025".into(),
            evidence: serde_json::json!({}),
            extracted_at: chrono::Utc::now(),
            schema_version: "v1".into(),
        };
        upsert_finding(&ctx, &f).await;

        // By summary token
        let by_summary = search(&ctx, "drawdowns", &SearchQuery::default())
            .await
            .unwrap();
        assert!(by_summary.iter().any(|h| h.kind == SearchKind::Finding));

        // By kind tag
        let by_kind = search(
            &ctx,
            "drawdown_concentration",
            &SearchQuery {
                kind: Some(SearchKind::Finding),
                limit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(by_kind.len(), 1);
        assert_eq!(by_kind[0].artifact_id, f.id);
    }

    #[tokio::test]
    async fn reindex_all_is_idempotent() {
        let (ctx, _dir) = fresh_ctx().await;
        reindex_all(&ctx).await;
        let count_after_first = search(&ctx, "", &SearchQuery::default())
            .await
            .unwrap()
            .len();
        reindex_all(&ctx).await;
        let count_after_second = search(&ctx, "", &SearchQuery::default())
            .await
            .unwrap()
            .len();
        assert_eq!(count_after_first, count_after_second);
    }
}
