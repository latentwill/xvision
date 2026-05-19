//! `xvn scenario` — scenario authoring: create / ls / show / clone / archive / rm / tree.
//!
//! Also exposes `xvn scenario select` — a **stateless** selector that filters
//! the scenario library by asset, timeframe, and decision-count proximity so
//! agents can pick a comparable set without hand-picking IDs.

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
    /// Select a comparable set of scenarios by asset, timeframe, and decision
    /// count. Purely a read-only query — nothing is created or mutated.
    ///
    /// Examples:
    ///   xvn scenario select --assets ETH/USD,BTC/USD --timeframe 4h --target-decisions 49 --count 4
    ///   xvn scenario select --same-decisions --max-decisions 200 --count 4 --json
    Select(SelectArgs),
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

/// Arguments for `xvn scenario select`.
///
/// Two mutually-exclusive selection modes:
///
/// **Mode A — `--target-decisions <N>`**: select scenarios whose decision count
/// is within ±10 % of N.  If more than `--count` match, one per asset is
/// preferred, then by closest to the target.
///
/// **Mode B — `--same-decisions` + `--max-decisions <N>`**: find the largest
/// common decision count ≤ N that appears in the candidate set and return only
/// those scenarios.  Auto-clone to normalise decision counts is **deferred**
/// (see code comment in `run_select` for deferral rationale).
#[derive(Args, Debug)]
pub struct SelectArgs {
    /// Comma-separated asset symbols to include (e.g. `ETH/USD,BTC/USD`).
    /// If omitted, all assets in the library are considered.
    #[arg(long, value_delimiter = ',')]
    pub assets: Vec<String>,

    /// Bar granularity / timeframe filter (e.g. `4h`, `1h`, `1d`).
    /// Maps to `decision_cadence_minutes`; see `parse_timeframe_minutes`.
    /// If omitted, all timeframes are considered.
    #[arg(long)]
    pub timeframe: Option<String>,

    /// [Mode A] Select scenarios within ±10 % of this decision count.
    /// Mutually exclusive with `--same-decisions`.
    #[arg(long, conflicts_with = "same_decisions")]
    pub target_decisions: Option<u64>,

    /// [Mode B] Return scenarios that share the largest common decision count
    /// that is ≤ `--max-decisions`.  Requires `--max-decisions`.
    #[arg(long, requires = "max_decisions", conflicts_with = "target_decisions")]
    pub same_decisions: bool,

    /// [Mode B] Maximum decision count for the common-count search.
    #[arg(long)]
    pub max_decisions: Option<u64>,

    /// Regime label filter (e.g. `bull,bear,range,crash`).
    ///
    /// NOTE: scenarios carry regime information via `regime:<label>` tags
    /// (e.g. `regime:trending_bull`).  This flag filters by those tags.
    /// The canonical tag values are: `trending_bull`, `trending_bear`,
    /// `range_bound`, `crash`.  Track #12 (`scenario-regime-labels`) will
    /// formalise a dedicated `regime_label` column; until then this flag
    /// performs a best-effort tag prefix match.
    #[arg(long, value_delimiter = ',')]
    pub regimes: Vec<String>,

    /// Maximum number of results to return.  When more candidates match than
    /// `--count`, preference is given to one-per-asset first, then by
    /// decision count closest to `--target-decisions` (or the common count
    /// in Mode B).
    #[arg(long, default_value_t = 4)]
    pub count: usize,

    /// Emit output as a JSON array of `{id, name, asset, timeframe, decision_count}`.
    /// Without this flag a plain-text table is printed.
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
        ScenarioOp::Inspect(a) => run_inspect(&ctx, a).await,
        ScenarioOp::Select(a) => run_select(&ctx, a).await,
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

// ---- scenario select --------------------------------------------------------

/// Compute the decision bar count for a scenario.
///
/// Uses the same formula as `format_inspect_card`: total bars in the time
/// window minus `warmup_bars`.  Returns 0 when bar granularity has no duration
/// (should never happen for valid scenarios).
pub fn scenario_decision_count(s: &Scenario) -> u64 {
    let window_secs = (s.time_window.end - s.time_window.start).num_seconds() as u64;
    let bar_secs = s.granularity.seconds();
    if bar_secs == 0 {
        return 0;
    }
    let total_bars = window_secs / bar_secs;
    total_bars.saturating_sub(s.warmup_bars as u64)
}

/// Extract the regime labels stored as `regime:<label>` tags.
pub fn scenario_regime_labels(s: &Scenario) -> Vec<String> {
    s.tags
        .iter()
        .filter_map(|t| t.strip_prefix("regime:").map(|r| r.to_string()))
        .collect()
}

/// One row in the `xvn scenario select` output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectRow {
    pub id: String,
    pub name: String,
    pub asset: String,
    pub timeframe: String,
    pub decision_count: u64,
}

/// Pure selection logic — takes a pre-fetched scenario list and applies
/// asset / timeframe / regime / decision-count filters plus the cap.
///
/// Exposed as `pub` so unit tests can call it directly without a live DB.
pub fn select_scenarios(
    scenarios: &[Scenario],
    assets: &[String],
    timeframe_minutes: Option<u32>,
    regimes: &[String],
    target_decisions: Option<u64>,
    same_decisions: bool,
    max_decisions: Option<u64>,
    count: usize,
) -> Result<Vec<SelectRow>, String> {
    // ── 1. Pre-filter by asset / timeframe / regime ───────────────────────

    let mut candidates: Vec<&Scenario> = scenarios
        .iter()
        .filter(|s| {
            // Asset filter: check if any of the scenario's AssetRefs match.
            if !assets.is_empty() {
                let sym = s.asset.first().map(|a| a.symbol.as_str()).unwrap_or("");
                // Accept "ETH/USD" or bare "ETH" matches.
                let matched = assets.iter().any(|want| {
                    let norm = want.split('/').next().unwrap_or(want);
                    sym.eq_ignore_ascii_case(norm) || want.eq_ignore_ascii_case(sym)
                });
                if !matched {
                    return false;
                }
            }

            // Timeframe filter: granularity.seconds() / 60 == timeframe_minutes.
            if let Some(tf_min) = timeframe_minutes {
                let bar_min = (s.granularity.seconds() / 60) as u32;
                if bar_min != tf_min {
                    return false;
                }
            }

            // Regime filter: match against `regime:<label>` tags.
            // A scenario qualifies when ANY of its regime tags is in the
            // requested list (OR semantics within a scenario).
            if !regimes.is_empty() {
                let labels = scenario_regime_labels(s);
                let matched = regimes.iter().any(|want| {
                    labels.iter().any(|l| {
                        // Accept "bull" matching "trending_bull" (prefix / substring),
                        // as well as exact matches.
                        l.eq_ignore_ascii_case(want) || l.contains(want.as_str())
                    })
                });
                if !matched {
                    return false;
                }
            }

            true
        })
        .collect();

    // ── 2. Decision-count mode ────────────────────────────────────────────

    let target_count: u64 = if same_decisions {
        // Mode B: find the largest common decision count ≤ max_decisions
        // across the candidate set.
        //
        // Auto-clone to normalise decision counts is DEFERRED.  The blast
        // radius of creating new scenario rows during a read-only `select`
        // call is too broad for this track (requires user confirmation and
        // a fresh display_name).  Track #12 (`scenario-regime-labels`) or a
        // dedicated `scenario normalise` verb should add this.  For now we
        // return only scenarios that already share the target count, and emit
        // an informational note when the set has to be further trimmed.
        let max = max_decisions.unwrap_or(u64::MAX);
        let counts: Vec<u64> = candidates
            .iter()
            .map(|s| scenario_decision_count(s))
            .filter(|&c| c <= max)
            .collect();

        // Find the largest count that appears at least `count` times, or the
        // largest count that appears at least once if no count meets the
        // threshold.
        let mut count_freq: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for c in &counts {
            *count_freq.entry(*c).or_insert(0) += 1;
        }

        // Prefer the largest count with freq ≥ requested count; fall back to
        // the global max.
        let best = count_freq
            .iter()
            .filter(|(_, &freq)| freq >= count)
            .map(|(&c, _)| c)
            .max()
            .or_else(|| count_freq.keys().copied().max());

        match best {
            Some(c) => c,
            None => {
                // No candidates at all — return empty.
                return Ok(vec![]);
            }
        }
    } else if let Some(t) = target_decisions {
        t
    } else {
        // No decision-count filter: keep all candidates (use 0 as sentinel,
        // filtered out below).
        0
    };

    // ── 3. Decision-count tolerance filter ───────────────────────────────

    if same_decisions {
        // Mode B: keep only exact matches to the common count.
        candidates.retain(|s| scenario_decision_count(s) == target_count);
    } else if let Some(t) = target_decisions {
        // Mode A: ±10 % tolerance.
        let lo = (t as f64 * 0.9).floor() as u64;
        let hi = (t as f64 * 1.1).ceil() as u64;
        candidates.retain(|s| {
            let dc = scenario_decision_count(s);
            dc >= lo && dc <= hi
        });
    }

    // ── 4. Cap at `count`, preferring one per asset then closest-to-target ──

    // Sort by closeness to target_count (0 when no target → stable order).
    candidates.sort_by_key(|s| {
        let dc = scenario_decision_count(s);
        if target_decisions.is_some() || same_decisions {
            let diff = (dc as i64 - target_count as i64).unsigned_abs();
            diff
        } else {
            0u64
        }
    });

    // One-per-asset preference: pick the closest-to-target scenario for each
    // distinct asset first, then fill up to `count` from the remaining.
    let mut seen_assets: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut selected: Vec<&Scenario> = Vec::with_capacity(count);

    // First pass: one per asset.
    for s in &candidates {
        if selected.len() >= count {
            break;
        }
        let sym = s.asset.first().map(|a| a.symbol.as_str()).unwrap_or("-").to_string();
        if seen_assets.insert(sym) {
            selected.push(s);
        }
    }

    // Second pass: fill remaining slots from any asset.
    for s in &candidates {
        if selected.len() >= count {
            break;
        }
        if !selected.iter().any(|r| r.id == s.id) {
            selected.push(s);
        }
    }

    // ── 5. Build output rows ──────────────────────────────────────────────

    let rows = selected
        .into_iter()
        .map(|s| {
            let asset = s.asset.first().map(|a| a.symbol.as_str()).unwrap_or("-").to_string();
            let timeframe = s.granularity.to_string();
            SelectRow {
                id: s.id.clone(),
                name: s.display_name.clone(),
                asset,
                timeframe,
                decision_count: scenario_decision_count(s),
            }
        })
        .collect();

    Ok(rows)
}

async fn run_select(ctx: &ApiContext, a: SelectArgs) -> CliResult<()> {
    // Parse optional timeframe → minutes.
    let timeframe_minutes = a
        .timeframe
        .as_deref()
        .map(|tf| {
            crate::commands::strategy::parse_timeframe_minutes(tf)
                .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))
        })
        .transpose()?;

    // Validate that at least one selection mode is active.
    if a.target_decisions.is_none() && !a.same_decisions {
        return Err(CliError::usage(anyhow::anyhow!(
            "specify either --target-decisions <N> (Mode A) or --same-decisions --max-decisions <N> (Mode B)"
        )));
    }

    // Fetch all non-archived scenarios.
    let all = api_scenario::list(
        ctx,
        api_scenario::ListScenariosFilter {
            source: None,
            tags: vec![],
            include_archived: false,
            parent_scenario_id: None,
        },
    )
    .await
    .map_err(|e| api_to_cli("scenarios select", e))?;

    let rows = select_scenarios(
        &all,
        &a.assets,
        timeframe_minutes,
        &a.regimes,
        a.target_decisions,
        a.same_decisions,
        a.max_decisions,
        a.count,
    )
    .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;

    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows)
                .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize: {e}")))?
        );
        return Ok(());
    }

    // Plain-text table.
    if rows.is_empty() {
        println!("(no matching scenarios)");
        return Ok(());
    }

    println!("{:<30}  {:<40}  {:<10}  {:<8}  {}", "ID", "NAME", "ASSET", "TIMEFRAME", "DECISIONS");
    for r in &rows {
        println!("{:<30}  {:<40}  {:<10}  {:<8}  {}", r.id, r.name, r.asset, r.timeframe, r.decision_count);
    }

    Ok(())
}

// NOTE: `xvn scenario set create` (persisted scenario sets) is deliberately
// deferred — it requires a new DB table (`scenario_sets`, `scenario_set_members`)
// and a migration reservation.  See track #12 / a dedicated follow-up track.
// Downstream callers pass `--scenarios sc_a,sc_b,...` directly for now.

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

#[cfg(test)]
pub mod select {
    //! Unit tests for the pure `select_scenarios` filter + ranking logic.
    //! These tests operate entirely on in-memory `Scenario` values — no DB.

    use super::*;
    use chrono::{TimeZone, Utc};
    use xvision_core::Capital;
    use xvision_engine::eval::scenario::{
        AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, BarGranularity, CalendarRef, DataSource,
        Fees, FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy,
        ReplayMode, Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
    };

    use std::str::FromStr;

    /// Build a minimal `Scenario` for testing.  `window_secs` is the number of
    /// seconds covered by the time window (start is fixed; end = start + window_secs).
    fn make_scenario(
        id: &str,
        asset_sym: &str,
        granularity: &str,
        window_secs: i64,
        warmup_bars: u32,
        regime_tags: &[&str],
    ) -> Scenario {
        let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::seconds(window_secs);
        let gran = BarGranularity::from_str(granularity).expect("valid gran");
        Scenario {
            id: id.to_string(),
            parent_scenario_id: None,
            source: ScenarioSource::User,
            display_name: format!("test-{id}"),
            description: String::new(),
            tags: regime_tags.iter().map(|t| format!("regime:{t}")).collect(),
            notes: None,
            asset_class: AssetClass::Crypto,
            asset: vec![AssetRef {
                class: AssetClass::Crypto,
                symbol: asset_sym.to_string(),
                venue_symbol: format!("{asset_sym}/USD"),
            }],
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            granularity: gran,
            timezone: "UTC".to_string(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees { maker_bps: 10, taker_bps: 25 },
                slippage: SlippageModel::None,
                latency: LatencyModel { decision_to_fill_ms: 0 },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: id.to_string(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars,
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            created_by: "test".to_string(),
            archived_at: None,
        }
    }

    // ── scenario_decision_count ───────────────────────────────────────────

    #[test]
    fn decision_count_1h_gran_with_200_warmup() {
        // 1h = 3600 s.  Window = 300 hours = 1 080 000 s → 300 bars − 200 warmup = 100.
        let s = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &[]);
        assert_eq!(scenario_decision_count(&s), 100);
    }

    #[test]
    fn decision_count_4h_gran_with_0_warmup() {
        // 4h = 14400 s.  Window = 48 bars (8 days) − 0 warmup = 48.
        let s = make_scenario("sc2", "BTC", "4h", 48 * 4 * 3_600, 0, &[]);
        assert_eq!(scenario_decision_count(&s), 48);
    }

    #[test]
    fn decision_count_warmup_saturates_at_zero() {
        // Warmup > total bars: saturating_sub → 0, not underflow.
        let s = make_scenario("sc3", "SOL", "1h", 5 * 3_600, 200, &[]);
        assert_eq!(scenario_decision_count(&s), 0);
    }

    // ── scenario_regime_labels ────────────────────────────────────────────

    #[test]
    fn regime_labels_extracted_from_tags() {
        let s = make_scenario("sc_r", "ETH", "1h", 100 * 3_600, 0, &["trending_bull", "low_vol"]);
        let labels = scenario_regime_labels(&s);
        assert!(labels.contains(&"trending_bull".to_string()));
        assert!(labels.contains(&"low_vol".to_string()));
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn regime_labels_empty_when_no_regime_tags() {
        let s = make_scenario("sc_nr", "ETH", "1h", 100 * 3_600, 0, &[]);
        assert!(scenario_regime_labels(&s).is_empty());
    }

    // ── select_scenarios Mode A (target-decisions) ────────────────────────

    #[test]
    fn mode_a_returns_empty_when_no_match() {
        // 50-decision scenarios; target = 200 (±10 % → 180..220) → no match.
        let s1 = make_scenario("sc1", "ETH", "1h", 250 * 3_600, 200, &[]);
        // 250 total − 200 warmup = 50 decisions
        let rows = select_scenarios(&[s1], &[], None, &[], Some(200), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn mode_a_matches_within_ten_percent_tolerance() {
        // 1h window: 300 total bars − 200 warmup = 100 decisions.
        // Target = 100 → ±10 % = 90..110 → match.
        let s1 = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1], &[], None, &[], Some(100), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision_count, 100);
    }

    #[test]
    fn mode_a_asset_filter_excludes_wrong_asset() {
        let eth = make_scenario("sc_eth", "ETH", "1h", 300 * 3_600, 200, &[]);
        let btc = make_scenario("sc_btc", "BTC", "1h", 300 * 3_600, 200, &[]);
        let rows =
            select_scenarios(&[eth, btc], &["ETH".to_string()], None, &[], Some(100), false, None, 4)
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].asset, "ETH");
    }

    #[test]
    fn mode_a_asset_filter_accepts_slash_form() {
        // `--assets ETH/USD` should match a scenario with symbol "ETH".
        let eth = make_scenario("sc_eth", "ETH", "1h", 300 * 3_600, 200, &[]);
        let rows =
            select_scenarios(&[eth], &["ETH/USD".to_string()], None, &[], Some(100), false, None, 4)
                .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn mode_a_timeframe_filter_excludes_wrong_granularity() {
        // 4h scenario; filter by 1h → excluded.
        let s = make_scenario("sc1", "ETH", "4h", 200 * 4 * 3_600, 0, &[]);
        // 60 min = 1h filter
        let rows = select_scenarios(&[s], &[], Some(60), &[], Some(100), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn mode_a_timeframe_filter_includes_matching_granularity() {
        // 4h scenario; filter by 4h (240 min) → included.
        // 200 total 4h bars − 0 warmup = 200 decisions.  target=200 → within ±10 %.
        let s = make_scenario("sc1", "ETH", "4h", 200 * 4 * 3_600, 0, &[]);
        let rows = select_scenarios(&[s], &[], Some(240), &[], Some(200), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn mode_a_regime_filter_excludes_non_matching() {
        let s = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &["trending_bear"]);
        let rows =
            select_scenarios(&[s], &[], None, &["bull".to_string()], Some(100), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn mode_a_regime_filter_includes_partial_match() {
        // "bull" is a substring of "trending_bull" → should match.
        let s = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &["trending_bull"]);
        let rows =
            select_scenarios(&[s], &[], None, &["bull".to_string()], Some(100), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn mode_a_count_cap_respected() {
        // 4 scenarios all matching; count=2 → only 2 returned.
        let s1 = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 300 * 3_600, 200, &[]);
        let s3 = make_scenario("sc3", "SOL", "1h", 300 * 3_600, 200, &[]);
        let s4 = make_scenario("sc4", "DOGE", "1h", 300 * 3_600, 200, &[]);
        let rows =
            select_scenarios(&[s1, s2, s3, s4], &[], None, &[], Some(100), false, None, 2).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn mode_a_one_per_asset_preferred_before_filling() {
        // Two ETH scenarios + one BTC; count=2 → expect one ETH + one BTC.
        let eth1 = make_scenario("sc_eth1", "ETH", "1h", 300 * 3_600, 200, &[]);
        let eth2 = make_scenario("sc_eth2", "ETH", "1h", 305 * 3_600, 200, &[]);
        let btc = make_scenario("sc_btc", "BTC", "1h", 300 * 3_600, 200, &[]);
        let rows =
            select_scenarios(&[eth1, eth2, btc], &[], None, &[], Some(100), false, None, 2).unwrap();
        assert_eq!(rows.len(), 2);
        let assets: Vec<&str> = rows.iter().map(|r| r.asset.as_str()).collect();
        assert!(assets.contains(&"ETH"), "expected ETH in result");
        assert!(assets.contains(&"BTC"), "expected BTC in result");
    }

    // ── select_scenarios Mode B (same-decisions) ──────────────────────────

    #[test]
    fn mode_b_finds_common_count() {
        // Two scenarios with 100 decisions, one with 50 — common count = 100.
        let s1 = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 300 * 3_600, 200, &[]);
        let s3 = make_scenario("sc3", "SOL", "1h", 250 * 3_600, 200, &[]);
        // s1 and s2 → 100 decisions; s3 → 50 decisions.
        let rows =
            select_scenarios(&[s1, s2, s3], &[], None, &[], None, true, Some(200), 2).unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(r.decision_count, 100, "all rows must have 100 decisions in mode B");
        }
    }

    #[test]
    fn mode_b_max_decisions_cap_observed() {
        // s1 → 200 decisions, s2 → 100 decisions, s3 → 100 decisions.
        // max_decisions = 150 → s1 excluded, common count among s2/s3 = 100.
        let s1 = make_scenario("sc1", "ETH", "1h", 400 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 300 * 3_600, 200, &[]);
        let s3 = make_scenario("sc3", "SOL", "1h", 300 * 3_600, 200, &[]);
        let rows =
            select_scenarios(&[s1, s2, s3], &[], None, &[], None, true, Some(150), 4).unwrap();
        assert!(!rows.iter().any(|r| r.id == "sc1"), "sc1 should be excluded");
        for r in &rows {
            assert_eq!(r.decision_count, 100);
        }
    }

    #[test]
    fn mode_b_returns_empty_when_no_candidates_under_max() {
        let s1 = make_scenario("sc1", "ETH", "1h", 400 * 3_600, 200, &[]);
        // 200 decisions; max_decisions = 50 → excluded → empty.
        let rows =
            select_scenarios(&[s1], &[], None, &[], None, true, Some(50), 4).unwrap();
        assert!(rows.is_empty());
    }

    // ── select_scenarios no-filter mode ──────────────────────────────────

    #[test]
    fn no_filter_mode_returns_all_without_decision_filter() {
        // The guard lives in run_select (async), not in select_scenarios itself.
        // We test that passing neither mode produces rows unfiltered by decision
        // count (the guard is tested at the clap / run_select level separately).
        let s1 = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 400 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1, s2], &[], None, &[], None, false, None, 4).unwrap();
        // Both returned; no decision-count filter applied.
        assert_eq!(rows.len(), 2);
    }

    // ── select subcommand is registered ──────────────────────────────────

    #[test]
    fn scenario_select_subcommand_registered() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        let select = scenario.find_subcommand("select");
        assert!(
            select.is_some(),
            "`xvn scenario select` subcommand must be registered"
        );
    }
}
