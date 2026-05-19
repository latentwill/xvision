//! `xvn example seed [--reset]` implementation.

use std::path::{Path, PathBuf};

use serde::Serialize;

use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::scenario_store;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::templates::{
    example_scenarios, example_strategies, is_example_scenario, is_example_strategy,
};

use crate::commands::example::{api_to_cli, CliResultUnit, SeedArgs};
use crate::exit::CliError;

/// Static markdown copied into `$XVN_HOME/examples/README.md` so the
/// tutorial artifact ships with the binary instead of having to be
/// looked up on disk at runtime.
const EXAMPLE_README: &str = include_str!("../../../../../data/examples/README.md");

#[derive(Debug, Default, Serialize)]
struct SeedSummary {
    reset: bool,
    strategies_created: Vec<String>,
    strategies_skipped: Vec<String>,
    strategies_removed: Vec<String>,
    scenarios_created: Vec<String>,
    scenarios_skipped: Vec<String>,
    scenarios_removed: Vec<String>,
    /// Example scenarios that could not be removed during `--reset`
    /// because at least one `eval_runs` row still references them. The
    /// existing row is preserved as-is so audit history stays intact.
    scenarios_preserved_referenced: Vec<String>,
    tutorial_path: String,
}

pub async fn run(xvn_home_override: Option<PathBuf>, args: SeedArgs) -> CliResultUnit {
    let xvn_home = crate::commands::home::resolve_xvn_home(xvn_home_override).map_err(CliError::usage)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| api_to_cli("open xvn_home", e))?;
    let strategy_store = FilesystemStore::new(strategy_store_dir(&xvn_home));

    let mut summary = SeedSummary {
        reset: args.reset,
        ..SeedSummary::default()
    };

    seed_strategies(&strategy_store, args.reset, &mut summary).await?;
    seed_scenarios(&ctx, args.reset, &mut summary).await?;
    write_tutorial(&xvn_home, &mut summary).await?;

    if args.json {
        let body = serde_json::to_string_pretty(&summary)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize summary: {e}")))?;
        println!("{body}");
    } else {
        print_human_summary(&summary);
    }

    Ok(())
}

async fn seed_strategies(store: &FilesystemStore, reset: bool, summary: &mut SeedSummary) -> CliResultUnit {
    if reset {
        let existing_ids = store
            .list()
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("list strategies: {e}")))?;
        for id in existing_ids {
            let strategy = match store.load(&id).await {
                Ok(s) => s,
                // A corrupt file blocking the reset path would be
                // surprising and unhelpful — skip and keep going.
                Err(_) => continue,
            };
            if is_example_strategy(&strategy) {
                store
                    .delete(&id)
                    .await
                    .map_err(|e| CliError::upstream(anyhow::anyhow!("delete {id}: {e}")))?;
                summary.strategies_removed.push(id);
            }
        }
    }

    for strategy in example_strategies() {
        let id = strategy.manifest.id.clone();
        let existing = store.load(&id).await.ok();
        match existing {
            Some(prior) if !reset && is_example_strategy(&prior) => {
                // Already labelled as ours and reset wasn't requested.
                summary.strategies_skipped.push(id);
                continue;
            }
            Some(prior) if !is_example_strategy(&prior) => {
                // Operator owns a strategy with the same id (extremely
                // unlikely given the `example-` prefix, but if so, never
                // overwrite — surface a clear error and stop).
                return Err(CliError::conflict(anyhow::anyhow!(
                    "strategy '{id}' exists and is not labelled as an example \
                     (creator='{}'). Refusing to overwrite operator data.",
                    prior.manifest.creator
                )));
            }
            _ => {}
        }
        store
            .save(&strategy)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("save {id}: {e}")))?;
        summary.strategies_created.push(id);
    }
    Ok(())
}

async fn seed_scenarios(ctx: &ApiContext, reset: bool, summary: &mut SeedSummary) -> CliResultUnit {
    use xvision_engine::api::ApiError;

    // Scenarios are immutable post-insert (migration 006's
    // `scenarios_no_update` trigger blocks `UPDATE` on every column
    // except `archived_at`). Refresh shape:
    //
    // * Default — insert if missing, skip if present. Never overwrite an
    //   operator-authored scenario.
    // * `--reset` — try `scenario_store::delete_scenario` on each
    //   example row first, then insert the curated set. The delete is
    //   blocked when at least one `eval_runs` row still references the
    //   scenario; in that case the existing row is preserved (the
    //   audit trail is more valuable than refreshing the body), and the
    //   row is recorded in `scenarios_preserved_referenced` so the
    //   operator can see why their reset did not rewrite everything.
    if reset {
        for scenario in example_scenarios() {
            let existing = scenario_store::get_scenario(ctx, &scenario.id)
                .await
                .map_err(|e| api_to_cli("seed lookup", e))?;
            let Some(prior) = existing else {
                continue;
            };
            if !is_example_scenario(&prior) {
                return Err(CliError::conflict(anyhow::anyhow!(
                    "scenario '{}' exists and is not labelled as an example. \
                     Refusing to overwrite operator data.",
                    scenario.id
                )));
            }
            match scenario_store::delete_scenario(ctx, &scenario.id).await {
                Ok(()) => {
                    summary.scenarios_removed.push(scenario.id.clone());
                }
                Err(ApiError::Validation(_)) => {
                    // delete_scenario surfaces a Validation error when
                    // `eval_runs` references the scenario. Keep the row.
                    summary.scenarios_preserved_referenced.push(scenario.id.clone());
                }
                Err(e) => return Err(api_to_cli("seed delete", e)),
            }
        }
    }

    for scenario in example_scenarios() {
        scenario
            .validate_v1()
            .map_err(|e| CliError::usage(anyhow::anyhow!("seed scenario validate: {e}")))?;
        let existing = scenario_store::get_scenario(ctx, &scenario.id)
            .await
            .map_err(|e| api_to_cli("seed lookup", e))?;
        match existing {
            Some(prior) if is_example_scenario(&prior) => {
                summary.scenarios_skipped.push(scenario.id.clone());
                continue;
            }
            Some(_) => {
                return Err(CliError::conflict(anyhow::anyhow!(
                    "scenario '{}' exists and is not labelled as an example. \
                     Refusing to overwrite operator data.",
                    scenario.id
                )));
            }
            None => {}
        }
        scenario_store::insert_scenario(ctx, &scenario)
            .await
            .map_err(|e| api_to_cli("seed insert", e))?;
        summary.scenarios_created.push(scenario.id);
    }
    Ok(())
}

async fn write_tutorial(xvn_home: &Path, summary: &mut SeedSummary) -> CliResultUnit {
    let examples_dir = xvn_home.join("examples");
    tokio::fs::create_dir_all(&examples_dir)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("create {}: {e}", examples_dir.display())))?;
    let readme = examples_dir.join("README.md");
    tokio::fs::write(&readme, EXAMPLE_README)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write {}: {e}", readme.display())))?;
    summary.tutorial_path = readme.display().to_string();
    Ok(())
}

fn print_human_summary(s: &SeedSummary) {
    if s.reset {
        println!("xvn example seed --reset");
    } else {
        println!("xvn example seed");
    }
    if !s.strategies_removed.is_empty() {
        println!("removed {} example strategies:", s.strategies_removed.len());
        for id in &s.strategies_removed {
            println!("  - {id}");
        }
    }
    if !s.strategies_created.is_empty() {
        println!("created {} strategies:", s.strategies_created.len());
        for id in &s.strategies_created {
            println!("  + {id}");
        }
    }
    if !s.strategies_skipped.is_empty() {
        println!(
            "skipped {} strategies (already seeded):",
            s.strategies_skipped.len()
        );
        for id in &s.strategies_skipped {
            println!("  · {id}");
        }
    }
    if !s.scenarios_removed.is_empty() {
        println!("removed {} example scenarios:", s.scenarios_removed.len());
        for id in &s.scenarios_removed {
            println!("  - {id}");
        }
    }
    if !s.scenarios_preserved_referenced.is_empty() {
        println!(
            "preserved {} example scenarios (referenced by existing eval runs — \
             body cannot be refreshed without removing those runs first):",
            s.scenarios_preserved_referenced.len()
        );
        for id in &s.scenarios_preserved_referenced {
            println!("  · {id}");
        }
    }
    if !s.scenarios_created.is_empty() {
        println!("created {} scenarios:", s.scenarios_created.len());
        for id in &s.scenarios_created {
            println!("  + {id}");
        }
    }
    if !s.scenarios_skipped.is_empty() {
        println!(
            "skipped {} scenarios (already seeded):",
            s.scenarios_skipped.len()
        );
        for id in &s.scenarios_skipped {
            println!("  · {id}");
        }
    }
    println!("tutorial: {}", s.tutorial_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use xvision_engine::strategies::templates::{
        EXAMPLE_SCENARIO_QUICKSTART_BULL, EXAMPLE_SCENARIO_QUICKSTART_FLASH, EXAMPLE_STRATEGY_BREAKOUT,
        EXAMPLE_STRATEGY_MEAN_REVERSION, EXAMPLE_STRATEGY_TREND_FOLLOWER,
    };

    async fn seed_fresh(xvn_home: &Path, reset: bool) -> SeedSummary {
        let user = "test-user".to_string();
        let ctx = ApiContext::open(xvn_home, Actor::Cli { user })
            .await
            .expect("open ctx");
        let store = FilesystemStore::new(strategy_store_dir(xvn_home));
        let mut summary = SeedSummary {
            reset,
            ..SeedSummary::default()
        };
        seed_strategies(&store, reset, &mut summary).await.unwrap();
        seed_scenarios(&ctx, reset, &mut summary).await.unwrap();
        write_tutorial(xvn_home, &mut summary).await.unwrap();
        summary
    }

    #[tokio::test]
    async fn first_seed_creates_strategies_scenarios_and_tutorial() {
        let dir = tempdir().unwrap();
        let summary = seed_fresh(dir.path(), false).await;

        assert_eq!(summary.strategies_created.len(), 3);
        assert!(summary
            .strategies_created
            .iter()
            .any(|id| id == EXAMPLE_STRATEGY_TREND_FOLLOWER));
        assert!(summary
            .strategies_created
            .iter()
            .any(|id| id == EXAMPLE_STRATEGY_MEAN_REVERSION));
        assert!(summary
            .strategies_created
            .iter()
            .any(|id| id == EXAMPLE_STRATEGY_BREAKOUT));
        assert!(summary.strategies_skipped.is_empty());
        assert!(summary.strategies_removed.is_empty());

        assert_eq!(summary.scenarios_created.len(), 2);
        assert!(summary
            .scenarios_created
            .iter()
            .any(|id| id == EXAMPLE_SCENARIO_QUICKSTART_BULL));
        assert!(summary
            .scenarios_created
            .iter()
            .any(|id| id == EXAMPLE_SCENARIO_QUICKSTART_FLASH));

        let readme = dir.path().join("examples/README.md");
        let body = tokio::fs::read_to_string(&readme).await.unwrap();
        assert!(body.contains("xvision example workspace"));
    }

    #[tokio::test]
    async fn second_seed_is_idempotent() {
        let dir = tempdir().unwrap();
        seed_fresh(dir.path(), false).await;
        let second = seed_fresh(dir.path(), false).await;
        assert!(second.strategies_created.is_empty());
        assert_eq!(second.strategies_skipped.len(), 3);
        assert!(second.scenarios_created.is_empty());
        assert_eq!(second.scenarios_skipped.len(), 2);
    }

    #[tokio::test]
    async fn reset_removes_and_recreates_strategies_and_unreferenced_scenarios() {
        let dir = tempdir().unwrap();
        seed_fresh(dir.path(), false).await;
        let second = seed_fresh(dir.path(), true).await;
        assert_eq!(second.strategies_removed.len(), 3);
        assert_eq!(second.strategies_created.len(), 3);
        // No eval_runs exist yet, so example scenarios delete cleanly and
        // get re-inserted from the curated set.
        assert_eq!(second.scenarios_removed.len(), 2);
        assert_eq!(second.scenarios_created.len(), 2);
        assert!(second.scenarios_skipped.is_empty());
        assert!(second.scenarios_preserved_referenced.is_empty());
    }

    #[tokio::test]
    async fn reset_preserves_scenarios_referenced_by_eval_runs() {
        use xvision_engine::strategies::templates::EXAMPLE_SCENARIO_QUICKSTART_BULL;

        let dir = tempdir().unwrap();
        seed_fresh(dir.path(), false).await;

        // Insert an eval_runs row that references one of the example
        // scenarios. The `delete_scenario` validation should kick in on
        // reset and the row should be preserved, not removed.
        let user = "test-user".to_string();
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user })
            .await
            .expect("open ctx");
        sqlx::query(
            "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
             VALUES ('run-pinning', 'strategy-x', ?1, 'backtest', 'completed', \
                     '2026-05-17T00:00:00Z')",
        )
        .bind(EXAMPLE_SCENARIO_QUICKSTART_BULL)
        .execute(&ctx.db)
        .await
        .expect("insert eval_runs row pinning the scenario");
        drop(ctx);

        let summary = seed_fresh(dir.path(), true).await;

        assert!(summary
            .scenarios_preserved_referenced
            .iter()
            .any(|id| id == EXAMPLE_SCENARIO_QUICKSTART_BULL));
        assert!(!summary
            .scenarios_removed
            .iter()
            .any(|id| id == EXAMPLE_SCENARIO_QUICKSTART_BULL));
        // The unreferenced flash-crash scenario still gets removed and
        // recreated on the same reset call.
        assert!(summary
            .scenarios_removed
            .iter()
            .any(|id| id == xvision_engine::strategies::templates::EXAMPLE_SCENARIO_QUICKSTART_FLASH));
    }

    #[tokio::test]
    async fn seed_does_not_touch_operator_owned_strategies() {
        use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
        use xvision_engine::strategies::risk::RiskPreset;
        use xvision_engine::strategies::slot::LLMSlot;
        use xvision_engine::strategies::{PipelineDef, Strategy};

        let dir = tempdir().unwrap();
        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        // Pre-seed an operator strategy with a non-example id and creator.
        let operator = Strategy {
            manifest: PublicManifest {
                id: "operator-trend".into(),
                display_name: "Operator's trend".into(),
                plain_summary: "Mine, not the examples".into(),
                creator: "@operator".into(),
                template: "trend_follower".into(),
                regime_fit: vec![RegimeFit::TrendingBull],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: "act on the briefing".into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({"ema_fast": 12}),
        };
        store.save(&operator).await.unwrap();

        // Seed, then reset, then confirm operator row still alive.
        seed_fresh(dir.path(), false).await;
        seed_fresh(dir.path(), true).await;

        let loaded = store.load("operator-trend").await.unwrap();
        assert_eq!(loaded.manifest.creator, "@operator");
    }
}
