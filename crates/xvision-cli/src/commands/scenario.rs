//! `xvn scenario` — scenario authoring: create / ls / show / clone / archive / rm / tree.

use std::path::PathBuf;
use std::str::FromStr;

use chrono::NaiveDate;
use clap::{Args, Subcommand};

use xvision_core::AssetSymbol;
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
    /// Show a scenario by id.
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
    #[arg(long)]
    pub toml: bool,
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
    } else {
        // Default: emit pretty JSON.
        println!(
            "{}",
            serde_json::to_string_pretty(&s)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize: {e}")))?
        );
    }
    Ok(())
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
