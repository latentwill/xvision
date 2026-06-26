//! `xvn example seed [--reset]` implementation.

use std::path::{Path, PathBuf};

use serde::Serialize;

use xvision_engine::agents::{AgentSlot, AgentStore, Capability, InputsPolicy, NewAgent};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::scenario_store;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::templates::{
    example_scenarios, is_example_scenario, is_example_strategy, EXAMPLE_STRATEGY_CREATOR,
    EXAMPLE_STRATEGY_TREND_FOLLOWER_ID,
};
use xvision_engine::strategies::{ActivationMode, AgentRef, PipelineDef, Strategy};

use crate::commands::example::{api_to_cli, CliResultUnit, SeedArgs};
use crate::exit::CliError;

/// Static markdown copied into `$XVN_HOME/examples/README.md` so the
/// tutorial artifact ships with the binary instead of having to be
/// looked up on disk at runtime.
const EXAMPLE_README: &str = include_str!("../../../../../data/examples/README.md");

/// The registered, launchable provider/model the example seed binds everything
/// to — the seeded `autooptimizer.toml` mutator/judge AND the seeded example
/// trader agent.
///
/// F30 (2026-06-04): the seeded example trader was previously hardcoded to
/// `anthropic` / `claude-haiku-4-5` (and the manifest to `anthropic.claude-*`),
/// a default-injection that is NOT registered on an `openrouter`-only node — so a
/// freshly-seeded example was immediately unrunnable and tripped the optimizer's
/// F22 cross-provider guard. The file already pointed the *optimizer config* at
/// `openrouter`/`google/gemini-3.1-flash-lite`; binding the trader to the SAME
/// constants keeps the whole seed internally consistent and free of any literal
/// `anthropic.claude-*` production default. `openrouter` must be present in
/// `$XVN_HOME/config/default.toml` for either surface to dispatch.
const SEED_DEFAULT_PROVIDER: &str = "openrouter";
const SEED_DEFAULT_MODEL: &str = "google/gemini-3.1-flash-lite";

/// Default autooptimizer.toml seeded under `$XVN_HOME` so a first
/// `xvn optimizer run-cycle` points the experiment writer (mutator) and the
/// judge at a registered, launchable provider instead of the keyless
/// `test`/`anthropic` default. Derived from [`SEED_DEFAULT_PROVIDER`] /
/// [`SEED_DEFAULT_MODEL`] so the optimizer config and the seeded trader can never
/// drift apart. Only written when absent — never clobbers an operator-authored
/// config.
fn default_autooptimizer_toml() -> String {
    format!(
        "\
# Seeded by `xvn example seed`. Points the optimizer's experiment writer
# (mutator) and judge at a registered, launchable provider. `{provider}` must
# be present in $XVN_HOME/config/default.toml for this to dispatch.
min_improvement = 0.05
holdout_min_improvement = 0.005
min_trade_retention_ratio = 0.5


[baseline_untouched_window]
start = \"2025-09-01\"
end = \"2025-12-01\"

[day_window]
start = \"2025-01-01\"
end = \"2025-04-01\"

[mutator]
provider = \"{provider}\"
model = \"{model}\"
max_retries = 2
",
        provider = SEED_DEFAULT_PROVIDER,
        model = SEED_DEFAULT_MODEL,
    )
}

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
    /// Path to the seeded autooptimizer.toml, or empty when one already
    /// existed (left untouched).
    autooptimizer_config_path: String,
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

    seed_strategies(&ctx, &strategy_store, args.reset, &mut summary).await?;
    seed_scenarios(&ctx, args.reset, &mut summary).await?;
    write_tutorial(&xvn_home, &mut summary).await?;
    seed_autooptimizer_config(&xvn_home, &mut summary).await?;

    if args.json {
        let body = serde_json::to_string_pretty(&summary)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize summary: {e}")))?;
        println!("{body}");
    } else {
        print_human_summary(&summary);
    }

    Ok(())
}

/// System prompt for the example trend-follower agent.
///
/// Must be ≥ 200 characters and must not start with the default placeholder
/// text (see `validate_agent_for_save`). Kept here rather than in a data file
/// so the seeder is self-contained and the binary needs no extra asset.
const EXAMPLE_TRADER_PROMPT: &str = "\
You are a momentum trend-follower trading BTC/USD on hourly bars. \
Read the supplied OHLCV price history and emit exactly one JSON decision: \
{\"action\":\"long_open|short_open|flat|hold\",\"conviction\":0.0..1.0,\"justification\":\"<one sentence>\"}. \
Reasoning guide: \
(1) Identify the dominant short-term trend from recent highs/lows and EMA slope direction. \
(2) Only open positions that align with the trend; prefer flat or hold when bars are range-bound or conflicting. \
(3) Conviction above 0.7 requires clear momentum evidence (e.g. higher highs, bullish engulfing). \
(4) Do not omit the action field. Do not wrap in markdown fences.\
";

async fn seed_strategies(
    ctx: &ApiContext,
    store: &FilesystemStore,
    reset: bool,
    summary: &mut SeedSummary,
) -> CliResultUnit {
    let agent_store = AgentStore::new(ctx.db.clone());

    // ── 1. Remove legacy seed-owned strategies (all ids) ───────────────────
    let existing_ids = store
        .list()
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("list strategies: {e}")))?;
    for id in existing_ids {
        let strategy = match store.load(&id).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        if is_example_strategy(&strategy) {
            // On reset (or always for legacy rows): also clean up any
            // scoped agents that were attached to this strategy so the
            // DB stays consistent.
            if reset || id != EXAMPLE_STRATEGY_TREND_FOLLOWER_ID {
                let _ = agent_store.delete_scoped_to(&id).await;
                store
                    .delete(&id)
                    .await
                    .map_err(|e| CliError::upstream(anyhow::anyhow!("delete {id}: {e}")))?;
                summary.strategies_removed.push(id);
            }
        }
    }

    // ── 2. Idempotency check — skip if already seeded and not reset ─────────
    if !reset {
        if let Ok(existing) = store.load(EXAMPLE_STRATEGY_TREND_FOLLOWER_ID).await {
            if is_example_strategy(&existing) && !existing.agents.is_empty() {
                summary
                    .strategies_skipped
                    .push(EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.into());
                return Ok(());
            }
        }
    }

    // ── 3. Create the scoped trader agent ───────────────────────────────────
    let agent_id = agent_store
        .create(NewAgent {
            name: "Example trend-follower trader".into(),
            description: "Default trader agent seeded with `xvn example seed`. \
                          Swap the model and prompt to match your strategy."
                .into(),
            tags: vec!["source:example".into()],
            slots: vec![AgentSlot {
                name: "trader".into(),
                // F30: bind to the registered seed default (openrouter/gemini),
                // NOT a hardcoded `anthropic` literal that an openrouter-only node
                // can't dispatch. Same provider/model the seeded optimizer config
                // uses — see SEED_DEFAULT_PROVIDER/MODEL.
                provider: SEED_DEFAULT_PROVIDER.into(),
                model: SEED_DEFAULT_MODEL.into(),
                system_prompt: EXAMPLE_TRADER_PROMPT.into(),
                skill_ids: vec![],
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                prompt_version: AgentSlot::compute_prompt_version(EXAMPLE_TRADER_PROMPT),
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: Default::default(),
                noop_skip: None,
                allowed_tools: vec!["ohlcv".into(), "submit_decision".into()],
                delta_briefing: None,
            }],
            scope_strategy_id: Some(EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.into()),
        })
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("create example agent: {e}")))?;

    // ── 4. Create the example strategy with the agent attached ──────────────
    let strategy = Strategy {
        manifest: PublicManifest {
            id: EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.into(),
            display_name: "Example — BTC/USD Trend Follower".into(),
            plain_summary: "Seeded example strategy. Follows hourly BTC/USD momentum. \
                            Edit or clone this to build your own strategy."
                .into(),
            creator: EXAMPLE_STRATEGY_CREATOR.into(),
            template: "trend_follower".into(),
            regime_fit: vec![RegimeFit::TrendingBull],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            // F30: provenance follows the seeded trader's actual binding
            // (openrouter/gemini), not a stale `anthropic.claude-*` literal.
            attested_with: vec![format!("{SEED_DEFAULT_PROVIDER}.{SEED_DEFAULT_MODEL}")],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id,
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: PipelineDef::single(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        decision_mode: Default::default(),
        mechanistic_config: None,
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        // Acknowledge every-bar dispatch for the out-of-the-box example so
        // `eval validate` passes without requiring the operator to add a filter
        // first. Operators who clone this strategy should unset this and add a
        // filter once they understand the cost model.
        acknowledge_no_filter: true,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    };

    store
        .save(&strategy)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("save example strategy: {e}")))?;

    summary
        .strategies_created
        .push(EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.into());
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

/// Seed a default `$XVN_HOME/autooptimizer.toml` pointing the optimizer's
/// mutator + judge at a registered, launchable provider. Idempotent and
/// non-destructive: if the file already exists it is left untouched (operator
/// edits win) and `autooptimizer_config_path` stays empty in the summary.
async fn seed_autooptimizer_config(xvn_home: &Path, summary: &mut SeedSummary) -> CliResultUnit {
    let config_path = xvn_home.join("autooptimizer.toml");
    if config_path.exists() {
        // Never clobber an existing config.
        return Ok(());
    }
    tokio::fs::create_dir_all(xvn_home)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("create {}: {e}", xvn_home.display())))?;
    tokio::fs::write(&config_path, default_autooptimizer_toml())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("write {}: {e}", config_path.display())))?;
    summary.autooptimizer_config_path = config_path.display().to_string();
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
    if !s.autooptimizer_config_path.is_empty() {
        println!("optimizer config: {}", s.autooptimizer_config_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
    use xvision_engine::strategies::risk::RiskPreset;
    use xvision_engine::strategies::slot::LLMSlot;
    use xvision_engine::strategies::templates::{
        EXAMPLE_SCENARIO_QUICKSTART_BULL, EXAMPLE_SCENARIO_QUICKSTART_FLASH, EXAMPLE_STRATEGY_CREATOR,
        EXAMPLE_STRATEGY_TREND_FOLLOWER_ID,
    };
    use xvision_engine::strategies::{ActivationMode, PipelineDef, Strategy};

    fn strategy_fixture(id: &str, creator: &str) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: id.into(),
                display_name: "Strategy fixture".into(),
                plain_summary: "Test strategy fixture".into(),
                creator: creator.into(),
                template: "trend_follower".into(),
                regime_fit: vec![RegimeFit::TrendingBull],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
                timeframe_requirements: Default::default(),
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                attested_with: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Balanced.expand(),
            decision_mode: Default::default(),
            mechanistic_config: None,
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        }
    }

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
        seed_strategies(&ctx, &store, reset, &mut summary).await.unwrap();
        seed_scenarios(&ctx, reset, &mut summary).await.unwrap();
        write_tutorial(xvn_home, &mut summary).await.unwrap();
        seed_autooptimizer_config(xvn_home, &mut summary).await.unwrap();
        summary
    }

    #[tokio::test]
    async fn first_seed_creates_strategy_with_trader_agent() {
        // Regression test for T7: `example-trend-follower` must be seeded with
        // at least one AgentRef whose role is "trader". Before this fix the
        // strategy was not seeded at all, so `eval run`/`eval validate` failed
        // with "strategy has no agent attached".
        let dir = tempdir().unwrap();
        let summary = seed_fresh(dir.path(), false).await;

        // Strategy is created.
        assert!(
            summary
                .strategies_created
                .contains(&EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.to_string()),
            "expected example-trend-follower in strategies_created, got: {:?}",
            summary.strategies_created,
        );
        assert!(summary.strategies_skipped.is_empty());
        assert!(summary.strategies_removed.is_empty());

        // The strategy on disk has >= 1 agent with role "trader".
        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let strategy = store
            .load(EXAMPLE_STRATEGY_TREND_FOLLOWER_ID)
            .await
            .expect("example-trend-follower must exist after seed");
        let has_trader = strategy.agents.iter().any(|a| a.canonical_role() == "trader");
        assert!(
            has_trader,
            "seeded example strategy must have >= 1 agent with role 'trader', agents: {:?}",
            strategy.agents,
        );
    }

    #[tokio::test]
    async fn seeded_example_trader_binds_to_registered_provider_not_anthropic() {
        // F30 regression: the seeded example trader must NOT be hardcoded to a
        // literal `anthropic.claude-*` default-injection. On an openrouter-only
        // node that binding is unrunnable and trips the optimizer's F22
        // cross-provider guard. The trader (and its provenance) must resolve to
        // the SAME registered provider the seeded optimizer config uses.
        let dir = tempdir().unwrap();
        seed_fresh(dir.path(), false).await;

        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let strategy = store
            .load(EXAMPLE_STRATEGY_TREND_FOLLOWER_ID)
            .await
            .expect("example-trend-follower must exist after seed");

        // Manifest provenance carries no stale `anthropic.claude-*` literal.
        for att in &strategy.manifest.attested_with {
            assert!(
                !att.to_ascii_lowercase().contains("anthropic"),
                "seeded manifest attested_with must not inject anthropic, got: {att}"
            );
        }
        assert!(
            strategy
                .manifest
                .attested_with
                .iter()
                .any(|a| a.contains(SEED_DEFAULT_PROVIDER)),
            "seeded manifest attested_with must reference the registered seed provider '{SEED_DEFAULT_PROVIDER}', got: {:?}",
            strategy.manifest.attested_with,
        );

        // The scoped trader agent's slot resolves to the registered seed default,
        // not anthropic.
        let agent_ref = strategy
            .agents
            .iter()
            .find(|a| a.canonical_role() == "trader")
            .expect("seeded strategy must have a trader agent");
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "test-user".into(),
            },
        )
        .await
        .expect("open ctx");
        let agent = AgentStore::new(ctx.db.clone())
            .get(&agent_ref.agent_id)
            .await
            .expect("load seeded agent")
            .expect("seeded trader agent must exist");
        let slot = agent
            .slots
            .iter()
            .find(|s| s.allowed_tools.iter().any(|tool| tool == "submit_decision"))
            .expect("seeded agent must have a trader slot");
        assert_eq!(
            slot.provider, SEED_DEFAULT_PROVIDER,
            "seeded trader slot must bind to the registered seed provider, not a hardcoded default"
        );
        assert_eq!(slot.model, SEED_DEFAULT_MODEL);
        assert_ne!(
            slot.provider, "anthropic",
            "F30: seeded trader must not inject an anthropic default"
        );
    }

    #[tokio::test]
    async fn first_seed_creates_scenarios_and_tutorial() {
        let dir = tempdir().unwrap();
        let summary = seed_fresh(dir.path(), false).await;

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
        // Strategy is skipped, not re-created.
        assert!(second.strategies_created.is_empty());
        assert_eq!(
            second.strategies_skipped,
            vec![EXAMPLE_STRATEGY_TREND_FOLLOWER_ID]
        );
        assert!(second.scenarios_created.is_empty());
        assert_eq!(second.scenarios_skipped.len(), 2);
    }

    #[tokio::test]
    async fn reset_recreates_strategy_and_scenarios() {
        let dir = tempdir().unwrap();
        seed_fresh(dir.path(), false).await;
        let second = seed_fresh(dir.path(), true).await;
        // On reset the strategy is removed and re-created.
        assert!(
            second
                .strategies_removed
                .contains(&EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.to_string()),
            "reset must remove the example strategy, removed: {:?}",
            second.strategies_removed,
        );
        assert!(
            second
                .strategies_created
                .contains(&EXAMPLE_STRATEGY_TREND_FOLLOWER_ID.to_string()),
            "reset must recreate the example strategy, created: {:?}",
            second.strategies_created,
        );
        // No eval_runs exist yet, so example scenarios delete cleanly and
        // get re-inserted from the curated set.
        assert_eq!(second.scenarios_removed.len(), 2);
        assert_eq!(second.scenarios_created.len(), 2);
        assert!(second.scenarios_skipped.is_empty());
        assert!(second.scenarios_preserved_referenced.is_empty());
    }

    #[tokio::test]
    async fn seed_prunes_legacy_agentless_example_strategy() {
        // A legacy strategy file (agentless, creator = @xvision-examples) is
        // pruned and replaced with the properly-wired version on the next seed.
        let dir = tempdir().unwrap();
        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let legacy = strategy_fixture("example-trend-follower", EXAMPLE_STRATEGY_CREATOR);
        // The fixture has `agents: Vec::new()` — this is the broken shape.
        assert!(
            legacy.agents.is_empty(),
            "fixture must represent the old agentless shape"
        );
        store.save(&legacy).await.unwrap();

        let summary = seed_fresh(dir.path(), false).await;

        // Legacy row is pruned (agents.is_empty → treated as stale) and
        // re-created with a real agent.
        assert!(
            summary
                .strategies_removed
                .contains(&"example-trend-follower".to_string())
                || summary
                    .strategies_created
                    .contains(&"example-trend-follower".to_string()),
            "expected prune+recreate of the agentless legacy row, \
             removed={:?} created={:?}",
            summary.strategies_removed,
            summary.strategies_created,
        );
        let loaded = store.load("example-trend-follower").await.unwrap();
        assert!(
            !loaded.agents.is_empty(),
            "after seed, example-trend-follower must have >= 1 agent"
        );
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
    async fn seed_writes_autooptimizer_config_at_registered_provider() {
        use xvision_engine::autooptimizer::config::AutoOptimizerConfig;

        let dir = tempdir().unwrap();
        let summary = seed_fresh(dir.path(), false).await;

        let config_path = dir.path().join("autooptimizer.toml");
        assert!(config_path.exists(), "seed must write autooptimizer.toml");
        assert_eq!(
            summary.autooptimizer_config_path,
            config_path.display().to_string()
        );

        // The seeded file must parse and point mutator/judge at a registered,
        // launchable provider — NOT the keyless test/anthropic default.
        let cfg = AutoOptimizerConfig::load(&config_path).expect("seeded config parses");
        cfg.validate().expect("seeded config validates");
        assert_eq!(cfg.mutator.provider, "openrouter");
        assert_eq!(cfg.mutator.model, "google/gemini-3.1-flash-lite");
        assert_ne!(cfg.mutator.provider, "test");
        assert_ne!(cfg.mutator.provider, "anthropic");
    }

    #[tokio::test]
    async fn seed_does_not_clobber_existing_autooptimizer_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("autooptimizer.toml");
        let operator_body = "# operator-authored\nmin_improvement = 0.10\n\
            [baseline_untouched_window]\nstart = \"2025-01-01\"\nend = \"2025-02-01\"\n\
            [day_window]\nstart = \"2024-01-01\"\nend = \"2024-06-01\"\n\
            [mutator]\nprovider = \"deepseek\"\nmodel = \"deepseek-v4-pro\"\nmax_retries = 1\n";
        tokio::fs::write(&config_path, operator_body).await.unwrap();

        let summary = seed_fresh(dir.path(), false).await;

        // Existing config is left byte-for-byte intact and not reported.
        let after = tokio::fs::read_to_string(&config_path).await.unwrap();
        assert_eq!(after, operator_body, "seed must not clobber operator config");
        assert!(
            summary.autooptimizer_config_path.is_empty(),
            "summary must not claim to have written a config when one existed"
        );
    }

    #[tokio::test]
    async fn seed_does_not_touch_operator_owned_strategies() {
        let dir = tempdir().unwrap();
        let store = FilesystemStore::new(strategy_store_dir(dir.path()));
        let operator = strategy_fixture("operator-trend", "@operator");
        store.save(&operator).await.unwrap();

        // Seed, then reset, then confirm operator row still alive.
        seed_fresh(dir.path(), false).await;
        seed_fresh(dir.path(), true).await;

        let loaded = store.load("operator-trend").await.unwrap();
        assert_eq!(loaded.manifest.creator, "@operator");
    }
}
