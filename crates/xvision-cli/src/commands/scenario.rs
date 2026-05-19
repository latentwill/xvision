//! `xvn scenario` — scenario authoring: create / ls / show / clone / archive / rm / tree.

use std::path::PathBuf;
use std::str::FromStr;

use chrono::NaiveDate;
use clap::{Args, Subcommand};

use xvision_core::AssetSymbol;
use xvision_engine::api::eval as api_eval;
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, AssetRef, BarGranularity, CalendarRef, Capital, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, ReplayMode, Scenario, ScenarioSource,
    SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::eval::scenario_store;

use crate::exit::{CliError, CliResult, XvnExit};

/// Map an engine ApiError to a CliError with the appropriate exit code.
fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
    let exit = match &e {
        ApiError::NotFound(_) => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_) => XvnExit::Conflict,
        ApiError::Internal(_) | ApiError::Db(_) | ApiError::Other(_) => XvnExit::Upstream,
    };
    CliError {
        exit,
        source: anyhow::anyhow!("{prefix}: {e}"),
    }
}

#[derive(Args, Debug)]
pub struct ScenarioCmd {
    #[command(subcommand)]
    pub op: ScenarioOp,
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum ScenarioOp {
    /// Create a new scenario.
    Create(CreateArgs),
    /// List scenarios (newest first, archived excluded by default).
    Ls(LsArgs),
    /// Show a scenario by id. JSON shape matches the `scenario` slot
    /// inside `EvalRunExport` (q15 §3 / §6).
    #[command(visible_alias = "get")]
    Show(ShowArgs),
    /// Clone an existing scenario, optionally overriding fields.
    Clone(CloneArgs),
    /// Validate a full CreateScenarioRequest TOML file without creating it.
    Validate(ValidateArgs),
    /// Soft-delete (archive) a scenario by id.
    Archive {
        /// Scenario id.
        id: String,
    },
    /// Hard-delete a scenario by id (blocked when eval runs reference it).
    Rm {
        /// Scenario id.
        id: String,
    },
    /// Print the lineage tree for a scenario (ancestors + immediate children).
    Tree {
        /// Scenario id.
        id: String,
    },
    /// Print a compact plain-text summary card for an agent to read fast.
    /// Use --card (required) to request the card layout; other formats will be
    /// added later without breaking this layout commitment.
    Inspect(InspectArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Display name for the scenario.
    #[arg(long)]
    pub name: String,
    /// Asset ticker (BTC, ETH, SOL, …).
    #[arg(long)]
    pub asset: String,
    /// Window start date (YYYY-MM-DD, UTC midnight).
    #[arg(long)]
    pub from: NaiveDate,
    /// Window end date (YYYY-MM-DD, UTC midnight).
    #[arg(long)]
    pub to: NaiveDate,
    /// Bar granularity. Supports Alpaca bars:
    /// 1-59m, 1-23h, 1d, 1w, 1/2/3/4/6/12mo.
    #[arg(long, default_value = "1h")]
    pub granularity: String,
    /// Venue (only `alpaca` in v1).
    #[arg(long, default_value = "alpaca")]
    pub venue: String,
    /// Maker fee in basis points.
    #[arg(long, default_value_t = 10)]
    pub fees_maker: u32,
    /// Taker fee in basis points.
    #[arg(long, default_value_t = 25)]
    pub fees_taker: u32,
    /// Slippage model: `linear:<bps>` or `none`.
    #[arg(long, default_value = "linear:5")]
    pub slippage: String,
    /// Simulated fill latency in milliseconds.
    #[arg(long, default_value_t = 500)]
    pub latency_ms: u32,
    /// Tag (repeatable). e.g. `--tag regression --tag eth`.
    #[arg(long)]
    pub tag: Vec<String>,
    /// Optional notes.
    #[arg(long)]
    pub notes: Option<String>,
    /// Pre-window context bars. Defaults to 200. Backtest/paper executors
    /// pre-fetch this many bars before `--from` so indicators and the
    /// trader LLM see real history at bar 1 of the decision window.
    #[arg(long)]
    pub warmup_bars: Option<u32>,
    /// Emit the created Scenario as JSON.
    #[arg(long)]
    pub json: bool,
    /// Load the full `CreateScenarioRequest` from a TOML file (other flags
    /// are ignored when this is set).
    #[arg(long)]
    pub from_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Filter by source: `canonical`, `user`, `clone`, or `generated`.
    #[arg(long)]
    pub source: Option<String>,
    /// Filter by tag (repeatable, AND-composed).
    #[arg(long)]
    pub tag: Vec<String>,
    /// Include archived scenarios.
    #[arg(long)]
    pub archived: bool,
    /// Emit as JSON array instead of tab-separated rows.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Scenario id.
    pub id: String,
    /// Emit as TOML (CreateScenarioRequest shape, suitable for `--from-file`).
    /// Mutually exclusive with `--format`; when set, the format flag is
    /// ignored (kept for backward compat with scripts that used
    /// `xvn scenario show --toml`).
    #[arg(long)]
    pub toml: bool,
    /// Output format for JSON mode. `json` (default) is pretty-printed;
    /// `json-compact` is single-line for shell pipes. Ignored when
    /// `--toml` is also set.
    #[arg(long, value_enum, default_value_t = crate::json::ObjectFormat::Json)]
    pub format: crate::json::ObjectFormat,
}

#[derive(Args, Debug)]
pub struct CloneArgs {
    /// Source scenario id.
    pub id: String,
    /// Override the display name of the clone.
    #[arg(long)]
    pub name: Option<String>,
    /// Override the window start date.
    #[arg(long)]
    pub from: Option<NaiveDate>,
    /// Override the window end date.
    #[arg(long)]
    pub to: Option<NaiveDate>,
    /// Override the asset ticker.
    #[arg(long)]
    pub asset: Option<String>,
    /// Override the pre-window warmup bars. Scenarios are immutable
    /// post-insert; cloning with a different `--warmup-bars` is the
    /// supported mutation path.
    #[arg(long)]
    pub warmup_bars: Option<u32>,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Full `CreateScenarioRequest` TOML file.
    #[arg(long)]
    pub from_file: PathBuf,
    /// Emit a JSON validation report.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Scenario id.
    pub id: String,
    /// Print the compact plain-text summary card. Required (other formats will
    /// be added as `--json`, `--table` later without changing this layout).
    #[arg(long)]
    pub card: bool,
}

pub async fn run(cmd: ScenarioCmd) -> CliResult<()> {
    let ctx = open_ctx(cmd.xvn_home.clone()).await.map_err(CliError::upstream)?;
    match cmd.op {
        ScenarioOp::Create(a) => run_create(&ctx, a).await,
        ScenarioOp::Ls(a) => run_ls(&ctx, a).await,
        ScenarioOp::Show(a) => run_show(&ctx, a).await,
        ScenarioOp::Clone(a) => run_clone(&ctx, a).await,
        ScenarioOp::Validate(a) => run_validate(&ctx, a).await,
        ScenarioOp::Archive { id } => {
            api_scenario::archive(&ctx, &id)
                .await
                .map_err(|e| api_to_cli("scenario archive", e))?;
            println!("archived {id}");
            Ok(())
        }
        ScenarioOp::Rm { id } => {
            api_scenario::delete(&ctx, &id)
                .await
                .map_err(|e| api_to_cli("scenario rm", e))?;
            println!("removed {id}");
            Ok(())
        }
        ScenarioOp::Tree { id } => run_tree(&ctx, id).await,
        ScenarioOp::Inspect(a) => run_inspect(&ctx, a).await,
    }
}

// ---- helpers ----------------------------------------------------------------

async fn open_ctx(override_path: Option<PathBuf>) -> anyhow::Result<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))
}

fn parse_granularity(s: &str) -> CliResult<BarGranularity> {
    BarGranularity::from_str(s).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))
}

fn parse_slippage(s: &str) -> CliResult<SlippageModel> {
    if let Some(bps_str) = s.strip_prefix("linear:") {
        let bps: u32 = bps_str
            .parse()
            .map_err(|_| CliError::usage(anyhow::anyhow!("bad slippage '{s}' — expected linear:<bps>")))?;
        Ok(SlippageModel::Linear { bps })
    } else if s == "none" {
        Ok(SlippageModel::None)
    } else {
        Err(CliError::usage(anyhow::anyhow!(
            "unknown slippage '{s}' — try linear:5 or none"
        )))
    }
}

fn parse_source(s: &str) -> CliResult<ScenarioSource> {
    match s.to_lowercase().as_str() {
        "canonical" => Ok(ScenarioSource::Canonical),
        "user" => Ok(ScenarioSource::User),
        "clone" => Ok(ScenarioSource::Clone),
        "generated" => Ok(ScenarioSource::Generated),
        other => Err(CliError::usage(anyhow::anyhow!(
            "unknown source '{other}'; expected one of: canonical | user | clone | generated"
        ))),
    }
}

fn parse_venue(s: &str) -> CliResult<Venue> {
    match s.to_lowercase().as_str() {
        "alpaca" => Ok(Venue::Alpaca),
        other => Err(CliError::usage(anyhow::anyhow!(
            "unknown venue '{other}'; only 'alpaca' is supported in v1"
        ))),
    }
}

fn asset_ref_from_sym(sym: &AssetSymbol) -> AssetRef {
    AssetRef {
        class: AssetClass::Crypto,
        symbol: sym.as_short().into(),
        venue_symbol: sym.as_alpaca_pair(),
    }
}

fn scenario_to_create_request(s: &Scenario) -> api_scenario::CreateScenarioRequest {
    api_scenario::CreateScenarioRequest {
        display_name: s.display_name.clone(),
        description: s.description.clone(),
        asset_class: s.asset_class,
        asset: s.asset.clone(),
        quote_currency: s.quote_currency,
        time_window: s.time_window.clone(),
        capital: s.capital.clone(),
        granularity: s.granularity,
        timezone: s.timezone.clone(),
        calendar: s.calendar.clone(),
        venue: s.venue.clone(),
        data_source: s.data_source.clone(),
        replay_mode: s.replay_mode,
        tags: s.tags.clone(),
        notes: s.notes.clone(),
        parent_scenario_id: s.parent_scenario_id.clone(),
        source: s.source,
        warmup_bars: Some(s.warmup_bars),
    }
}

// ---- handlers ---------------------------------------------------------------

async fn run_create(ctx: &ApiContext, a: CreateArgs) -> CliResult<()> {
    // --from-file path: load a TOML file containing a CreateScenarioRequest.
    if let Some(path) = a.from_file {
        let body = std::fs::read_to_string(&path)
            .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", path.display())))?;
        let req: api_scenario::CreateScenarioRequest =
            toml::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse TOML: {e}")))?;
        let s = api_scenario::create(ctx, req)
            .await
            .map_err(|e| api_to_cli("scenario create", e))?;
        if a.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&s)
                    .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize: {e}")))?
            );
            return Ok(());
        }
        println!("created {} ({})", s.id, s.display_name);
        return Ok(());
    }

    let asset_sym = AssetSymbol::from_str(&a.asset).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    let granularity = parse_granularity(&a.granularity)?;
    let slippage = parse_slippage(&a.slippage)?;
    let venue = parse_venue(&a.venue)?;

    let req = api_scenario::CreateScenarioRequest {
        display_name: a.name,
        description: String::new(),
        asset_class: AssetClass::Crypto,
        asset: vec![asset_ref_from_sym(&asset_sym)],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: a
                .from
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --from date")))?
                .and_utc(),
            end: a
                .to
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --to date")))?
                .and_utc(),
        },
        capital: Capital::default(),
        granularity,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        venue: VenueSettings {
            venue,
            fees: Fees {
                maker_bps: a.fees_maker,
                taker_bps: a.fees_taker,
            },
            slippage,
            latency: LatencyModel {
                decision_to_fill_ms: a.latency_ms,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
        },
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        replay_mode: ReplayMode::Continuous,
        tags: a.tag,
        notes: a.notes,
        parent_scenario_id: None,
        source: ScenarioSource::User,
        warmup_bars: a.warmup_bars,
    };

    let s = api_scenario::create(ctx, req)
        .await
        .map_err(|e| api_to_cli("scenario create", e))?;
    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&s)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize: {e}")))?
        );
        return Ok(());
    }
    println!("created {} ({})", s.id, s.display_name);
    Ok(())
}

async fn run_ls(ctx: &ApiContext, a: LsArgs) -> CliResult<()> {
    let source = a.source.as_deref().map(parse_source).transpose()?;
    let filter = api_scenario::ListScenariosFilter {
        source,
        tags: a.tag,
        include_archived: a.archived,
        parent_scenario_id: None,
    };
    let rows = api_scenario::list(ctx, filter)
        .await
        .map_err(|e| api_to_cli("scenario ls", e))?;

    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize: {e}")))?
        );
        return Ok(());
    }

    if rows.is_empty() {
        println!("(no scenarios)");
        return Ok(());
    }

    println!("ID\tDISPLAY_NAME\tASSET\tWINDOW\tSOURCE");
    for s in &rows {
        let asset = s.asset.first().map(|a| a.symbol.as_str()).unwrap_or("-");
        println!(
            "{}\t{}\t{}\t{}..{}\t{:?}",
            s.id,
            s.display_name,
            asset,
            s.time_window.start.format("%Y-%m-%d"),
            s.time_window.end.format("%Y-%m-%d"),
            s.source,
        );
    }
    Ok(())
}

async fn run_show(ctx: &ApiContext, a: ShowArgs) -> CliResult<()> {
    let s = api_scenario::get(ctx, &a.id)
        .await
        .map_err(|e| api_to_cli("scenario show", e))?;

    if a.toml {
        let req = scenario_to_create_request(&s);
        let out = toml::to_string_pretty(&req)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize TOML: {e}")))?;
        println!("{out}");
        return Ok(());
    }
    // Default: emit JSON in the shared shape (matches the
    // `scenario` slot inside `EvalRunExport`).
    crate::json::emit_object(&s, a.format)
}

async fn run_clone(ctx: &ApiContext, a: CloneArgs) -> CliResult<()> {
    // Resolve time_window override (need both --from and --to if either is set).
    let time_window = match (a.from, a.to) {
        (Some(f), Some(t)) => Some(TimeWindow {
            start: f
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --from date")))?
                .and_utc(),
            end: t
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --to date")))?
                .and_utc(),
        }),
        (None, None) => None,
        _ => {
            return Err(CliError::usage(anyhow::anyhow!(
                "clone: --from and --to must both be set, or neither"
            )));
        }
    };

    let asset = a
        .asset
        .as_deref()
        .map(|sym_str| {
            let sym = AssetSymbol::from_str(sym_str).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
            Ok::<_, CliError>(vec![asset_ref_from_sym(&sym)])
        })
        .transpose()?;

    let mutations = api_scenario::ScenarioMutations {
        display_name: a.name,
        time_window,
        asset,
        description: None,
        granularity: None,
        venue: None,
        tags: None,
        notes: None,
        warmup_bars: a.warmup_bars,
    };

    let s = api_scenario::clone(ctx, &a.id, mutations)
        .await
        .map_err(|e| api_to_cli("scenario clone", e))?;
    println!("cloned to {} (parent: {})", s.id, a.id);
    Ok(())
}

async fn run_validate(ctx: &ApiContext, a: ValidateArgs) -> CliResult<()> {
    let body = std::fs::read_to_string(&a.from_file)
        .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", a.from_file.display())))?;
    let req: api_scenario::CreateScenarioRequest =
        toml::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse TOML: {e}")))?;
    api_scenario::validate_request(&req, ctx)
        .await
        .map_err(|e| api_to_cli("scenario validate", e))?;
    if a.json {
        println!("{}", serde_json::json!({ "ok": true }));
    } else {
        println!("ok");
    }
    Ok(())
}

async fn run_tree(ctx: &ApiContext, id: String) -> CliResult<()> {
    let s = api_scenario::get(ctx, &id)
        .await
        .map_err(|e| api_to_cli("scenario tree", e))?;

    // Walk up to the root, collecting the ancestor chain.
    let mut chain: Vec<Scenario> = vec![s.clone()];
    let mut cur = s.parent_scenario_id.clone();
    while let Some(pid) = cur {
        let p = api_scenario::get(ctx, &pid)
            .await
            .map_err(|e| api_to_cli("scenario tree (parent lookup)", e))?;
        cur = p.parent_scenario_id.clone();
        chain.push(p);
    }
    chain.reverse(); // root first

    // Print the ancestor chain (indented by depth).
    for (i, node) in chain.iter().enumerate() {
        let marker = if node.id == id { " ← (this)" } else { "" };
        let archived = if node.archived_at.is_some() {
            " (archived)"
        } else {
            ""
        };
        println!(
            "{}{} ({}){}{}",
            "  ".repeat(i),
            node.id,
            node.display_name,
            archived,
            marker
        );
    }

    // Print immediate children one level down.
    let children = scenario_store::list_children(ctx, &id)
        .await
        .map_err(|e| api_to_cli("scenario tree (children)", e))?;
    let child_indent = "  ".repeat(chain.len());
    for child in &children {
        let archived = if child.archived_at.is_some() {
            " (archived)"
        } else {
            ""
        };
        println!(
            "{}{} ({}){}",
            child_indent, child.id, child.display_name, archived
        );
    }

    if chain.len() == 1 && children.is_empty() {
        println!("  (no parent, no children)");
    }

    Ok(())
}

/// Build the compact plain-text summary card string for a scenario.
///
/// `run_count` and `best_return_pct` come from the caller after aggregating
/// the eval runs list; pass `None` for both when the runs list is unavailable.
pub fn format_inspect_card(s: &Scenario, run_count: Option<usize>, best_return_pct: Option<f64>) -> String {
    let mut out = String::new();

    let asset = s.asset.first().map(|a| a.symbol.as_str()).unwrap_or("-");
    let quote = format!("{:?}", s.quote_currency).to_uppercase();
    let asset_pair = format!("{}/{}", asset, quote);

    out.push_str(&format!("id: {}\n", s.id));
    out.push_str(&format!("name: {}\n", s.display_name));
    out.push_str(&format!("asset: {}\n", asset_pair));
    out.push_str(&format!("timeframe: {}\n", s.granularity));
    out.push_str(&format!(
        "date_window: {}..{}\n",
        s.time_window.start.format("%Y-%m-%d"),
        s.time_window.end.format("%Y-%m-%d"),
    ));
    out.push_str(&format!("warmup_bars: {}\n", s.warmup_bars));

    // decision_bars: derive from the window duration ÷ granularity bar seconds,
    // then subtract warmup_bars. We use the granularity's seconds_per_bar to
    // compute how many decision bars fit in the window.
    let window_secs = (s.time_window.end - s.time_window.start).num_seconds() as u64;
    let bar_secs = s.granularity.seconds();
    let decision_bars = if bar_secs > 0 {
        let total_bars = window_secs / bar_secs;
        total_bars.saturating_sub(s.warmup_bars as u64)
    } else {
        0
    };
    out.push_str(&format!("decision_bars: {}\n", decision_bars));

    if let Some(parent_id) = &s.parent_scenario_id {
        out.push_str(&format!("source: cloned_from {}\n", parent_id));
    }

    // TODO: regime/volatility labels — see track #12

    match (run_count, best_return_pct) {
        (Some(count), best) => {
            out.push_str("previous_runs:\n");
            out.push_str(&format!("  count: {}\n", count));
            if let Some(ret) = best {
                out.push_str(&format!("  best_return_pct: {:.2}\n", ret));
            } else {
                out.push_str("  best_return_pct: (none)\n");
            }
        }
        _ => {
            out.push_str("previous_runs: (unavailable)\n");
        }
    }

    // Trim trailing newline for clean output.
    if out.ends_with('\n') {
        out.truncate(out.len() - 1);
    }
    out
}

async fn run_inspect(ctx: &ApiContext, a: InspectArgs) -> CliResult<()> {
    if !a.card {
        return Err(CliError::usage(anyhow::anyhow!(
            "specify --card (other formats will be added later: --json, --table)"
        )));
    }

    let s = api_scenario::get(ctx, &a.id)
        .await
        .map_err(|e| api_to_cli("scenario inspect", e))?;

    // Aggregate previous runs: count + best total_return_pct.
    let runs_result = api_eval::list(
        ctx,
        api_eval::ListRunsRequest {
            scenario_id: Some(a.id.clone()),
            agent_id: None,
            status: None,
        },
    )
    .await;

    let (run_count, best_return) = match runs_result {
        Ok(runs) => {
            let count = runs.len();
            let best = runs
                .iter()
                .filter_map(|r| r.metrics.as_ref().map(|m| m.total_return_pct))
                .reduce(f64::max);
            (Some(count), best)
        }
        Err(_) => (None, None),
    };

    println!("{}", format_inspect_card(&s, run_count, best_return));
    Ok(())
}

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-cli scenario::get::json` (per the
    //! q15-object-json-output contract verification block).
    //!
    //! Parity guard: the `xvn scenario get` CLI emits the same
    //! `Scenario` struct that `EvalRunExport.scenario` carries.

    pub mod json {
        use xvision_engine::api::scenario as api_scenario;
        use xvision_engine::api::{Actor, ApiContext};
        use xvision_engine::eval::export as eval_export;
        use xvision_engine::eval::run::{Run, RunMode, RunStatus};
        use xvision_engine::eval::store::RunStore;

        #[tokio::test]
        async fn scenario_get_shape_matches_eval_export_scenario_slot() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = ApiContext::open(
                dir.path(),
                Actor::Cli {
                    user: "object-json-test".into(),
                },
            )
            .await
            .expect("open ApiContext");

            // Canonical scenarios land via the migrate-on-first-open
            // hook; pick the always-present one.
            let scenario_id = "crypto-bull-q1-2025";

            // Seed a completed run so `EvalRunExport.scenario` is
            // populated for the parity compare.
            let store = RunStore::new(ctx.db.clone());
            let mut run = Run::new_queued("agent-fixture".into(), scenario_id.into(), RunMode::Backtest);
            run.status = RunStatus::Completed;
            store.create(&run).await.expect("seed run");
            store
                .update_status(&run.id, RunStatus::Completed, None)
                .await
                .expect("transition");

            let direct = api_scenario::get(&ctx, scenario_id).await.expect("scenario get");
            let export = eval_export::build_export(&ctx, &run.id)
                .await
                .expect("build_export");

            let direct_json = serde_json::to_value(&direct).expect("scenario->json");
            let from_export = export
                .scenario
                .as_ref()
                .map(serde_json::to_value)
                .expect("export.scenario present")
                .expect("export.scenario->json");
            assert_eq!(
                direct_json, from_export,
                "scenario shape from `xvn scenario get` must equal `EvalRunExport.scenario`",
            );
        }

        #[test]
        fn scenario_show_has_get_alias() {
            use clap::CommandFactory;
            let cmd = crate::Cli::command();
            let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
            let show = scenario.find_subcommand("show").expect("show subcommand");
            let aliases: Vec<&str> = show.get_visible_aliases().collect();
            assert!(
                aliases.contains(&"get"),
                "expected `get` visible alias on `xvn scenario show`; aliases: {aliases:?}",
            );
        }
    }
}
