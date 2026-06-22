//! `xvn scenario` — scenario authoring: create / ls / show / clone / archive / rm / tree.
//!
//! Also exposes `xvn scenario select` — a **stateless** selector that filters
//! the scenario library by strategy timeframe, decision-count proximity, and
//! regime. Scenarios are asset-free date ranges; asset-universe and timeframe
//! selection live at the run/strategy layer.

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Subcommand, ValueEnum};

use xvision_engine::api::eval as api_eval;
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::regime::derive_regime_labels;
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, CalendarRef, Capital, DataSource, Fees, FillModel, LatencyModel, LimitOrderFill,
    MarketOrderFill, QuoteCurrency, ReplayMode, Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue,
    VenueSettings,
};
use xvision_engine::eval::scenario_store;

use crate::exit::{CliError, CliResult, XvnExit};
use crate::io::{print_json, print_json_compact};

/// Output format for list / collection subcommands (`ls`, `select`).
/// Mirrors the convention from `xvn agent ls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ListFormat {
    /// Human-readable table (default).
    Table,
    /// Pretty-printed JSON array.
    Json,
    /// Compact single-line JSON array, suitable for piping.
    #[clap(name = "json-compact")]
    JsonCompact,
}

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
    #[command(visible_alias = "list")]
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
        /// Validate and preview the would-be change without writing anything.
        /// Exits 0 when valid; exits non-zero on validation failure.
        #[arg(long)]
        dry_run: bool,
    },
    /// Hard-delete a scenario by id (blocked when eval runs reference it).
    Rm {
        /// Scenario id.
        id: String,
        /// Validate and preview the would-be delete without writing anything.
        /// Exits 0 when valid; exits non-zero when the id is not found.
        #[arg(long)]
        dry_run: bool,
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
    /// Select a comparable set of scenarios by decision count and regime.
    /// Purely a read-only query — nothing is created or mutated. `--timeframe`
    /// is the strategy timeframe used for decision-count math.
    ///
    /// Examples:
    ///   xvn scenario select --timeframe 4h --target-decisions 49 --count 4
    ///   xvn scenario select --same-decisions --max-decisions 200 --count 4 --json
    Select(SelectArgs),
    /// Auto-derive regime labels for one or all scenarios from their bar window.
    ///
    /// Writes regime_label, volatility_label, trend_direction, and sets
    /// regime_derived = true.  Skips scenarios that already have operator-set
    /// labels (regime_derived = false) unless --force is given.
    ///
    /// Examples:
    ///   xvn scenario classify sc_01JR3PPWB1WE5XKYGEP7NYWRT9
    ///   xvn scenario classify --all
    ///   xvn scenario classify sc_01JR3PPWB1WE5XKYGEP7NYWRT9 --force
    Classify(ClassifyArgs),
    /// Set operator-authored regime labels on a scenario (regime_derived = false).
    ///
    /// All three label flags are optional; omitting one leaves the existing value
    /// unchanged.  Use --regime, --volatility, --direction to set one or more.
    ///
    /// Examples:
    ///   xvn scenario set-regime sc_01JR3PPWB1WE5XKYGEP7NYWRT9 --regime expansion --volatility high --direction up
    ///   xvn scenario set-regime sc_01JR3PPWB1WE5XKYGEP7NYWRT9 --regime crash
    #[command(name = "set-regime")]
    SetRegime(SetRegimeArgs),
    /// Apply a scenario file (JSON/TOML) — create if not found; diff-only if already exists (scenarios are immutable post-insert).
    Apply(ScenarioApplyArgs),
    /// Show the diff between a scenario file and the saved workspace version. Read-only.
    Diff(ScenarioDiffArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Display name for the scenario.
    #[arg(long)]
    pub name: String,
    /// Window start date (YYYY-MM-DD, UTC midnight).
    #[arg(long)]
    pub from: NaiveDate,
    /// Window end date (YYYY-MM-DD, UTC midnight).
    #[arg(long)]
    pub to: NaiveDate,
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
    /// Validate args and resolve references without persisting the scenario.
    /// Prints a preview of the would-be create. Exits 0 on valid input;
    /// exits non-zero on validation failure.
    #[arg(long)]
    pub dry_run: bool,
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
    /// Override the pre-window warmup bars. Scenarios are immutable
    /// post-insert; cloning with a different `--warmup-bars` is the
    /// supported mutation path.
    #[arg(long)]
    pub warmup_bars: Option<u32>,
    /// Validate and preview the would-be clone without writing anything.
    /// Resolves the source scenario (read-only) and prints a preview.
    /// Exits 0 on valid input; exits non-zero on validation failure.
    #[arg(long)]
    pub dry_run: bool,
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
/// is within ±10 % of N.  If more than `--count` match, results are ordered by
/// closest to the target.
///
/// **Mode B — `--same-decisions` + `--max-decisions <N>`**: find the largest
/// common decision count ≤ N that appears in the candidate set and return only
/// those scenarios.  Auto-clone to normalise decision counts is **deferred**
/// (see code comment in `run_select` for deferral rationale).
#[derive(Args, Debug)]
pub struct SelectArgs {
    /// Strategy timeframe used to compute decision counts (e.g. `4h`, `1h`, `1d`).
    /// If omitted, decision counts are computed at 1h.
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
    /// `--count`, results are ordered by decision count closest to
    /// `--target-decisions` (or the common count in Mode B).
    #[arg(long, default_value_t = 4)]
    pub count: usize,

    /// Emit output as a JSON array of `{id, name, decision_count}`.
    /// Alias for `--format json-compact`. Explicit `--format` wins.
    #[arg(long)]
    pub json: bool,
    /// Output format: `table` (default), `json`, or `json-compact`.
    /// When `--json` is also set and `--format` is not explicitly supplied,
    /// `--json` is treated as `--format json-compact`.
    /// Leave unset (None) to use the default (`table`, or `json-compact` when `--json`).
    #[arg(long, value_enum)]
    pub format: Option<ListFormat>,
}

/// Arguments for `xvn scenario classify`.
#[derive(Args, Debug)]
pub struct ClassifyArgs {
    /// Scenario id to classify. Mutually exclusive with `--all`.
    #[arg(conflicts_with = "all")]
    pub id: Option<String>,

    /// Classify every scenario with a NULL regime_label (no operator override).
    /// Mutually exclusive with positional `id`.
    #[arg(long, conflicts_with = "id")]
    pub all: bool,

    /// Overwrite even operator-set labels (regime_derived = false).
    #[arg(long)]
    pub force: bool,

    /// Compute labels and print the preview without writing to the DB.
    /// Exits 0 when bars are available and classification succeeds;
    /// exits non-zero when bars are missing or classification fails.
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments for `xvn scenario set-regime`.
#[derive(Args, Debug)]
pub struct SetRegimeArgs {
    /// Scenario id.
    pub id: String,

    /// Broad regime label. One of: trend | chop | crash | expansion | recovery.
    #[arg(long)]
    pub regime: Option<String>,

    /// Volatility label. One of: low | normal | high | extreme.
    #[arg(long)]
    pub volatility: Option<String>,

    /// Trend direction. One of: up | down | sideways.
    #[arg(long)]
    pub direction: Option<String>,

    /// Validate and preview the would-be label update without writing to the DB.
    /// Exits 0 on valid input; exits non-zero on validation failure.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct ScenarioApplyArgs {
    /// Path to a Scenario JSON or TOML file.
    pub file: std::path::PathBuf,
    /// Preview without creating (dry-run).
    #[arg(long)]
    pub dry_run: bool,
    /// Emit result as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ScenarioDiffArgs {
    /// Path to a Scenario JSON or TOML file.
    pub file: std::path::PathBuf,
    /// Emit diff as JSON.
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
        ScenarioOp::Archive { id, dry_run } => run_archive(&ctx, id, dry_run).await,
        ScenarioOp::Rm { id, dry_run } => run_rm(&ctx, id, dry_run).await,
        ScenarioOp::Tree { id } => run_tree(&ctx, id).await,
        ScenarioOp::Inspect(a) => run_inspect(&ctx, a).await,
        ScenarioOp::Select(a) => run_select(&ctx, a).await,
        ScenarioOp::Classify(a) => run_classify(&ctx, a).await,
        ScenarioOp::SetRegime(a) => run_set_regime(&ctx, a).await,
        ScenarioOp::Apply(a) => run_scenario_apply(&ctx, a).await,
        ScenarioOp::Diff(a) => run_scenario_diff(&ctx, a).await,
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
        "frozen" => Ok(ScenarioSource::Frozen),
        other => Err(CliError::usage(anyhow::anyhow!(
            "unknown source '{other}'; expected one of: canonical | user | clone | generated | frozen"
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

fn scenario_to_create_request(s: &Scenario) -> api_scenario::CreateScenarioRequest {
    api_scenario::CreateScenarioRequest {
        display_name: s.display_name.clone(),
        description: s.description.clone(),
        asset_class: s.asset_class,
        quote_currency: s.quote_currency,
        time_window: s.time_window.clone(),
        capital: s.capital.clone(),
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
    let dry_run = a.dry_run;

    // --from-file path: load a TOML file containing a CreateScenarioRequest.
    if let Some(path) = a.from_file {
        let body = std::fs::read_to_string(&path)
            .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", path.display())))?;
        let req: api_scenario::CreateScenarioRequest =
            toml::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse TOML: {e}")))?;
        if dry_run {
            // Validate the request without persisting.
            api_scenario::validate_request(&req, ctx)
                .await
                .map_err(|e| api_to_cli("scenario create --dry-run", e))?;
            if a.json {
                print_json(&serde_json::json!({
                    "dry_run": true,
                    "action": "create",
                    "display_name": req.display_name,
                }))?;
            } else {
                eprintln!("DRY RUN — would create scenario '{}'", req.display_name);
            }
            return Ok(());
        }
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

    let slippage = parse_slippage(&a.slippage)?;
    let venue = parse_venue(&a.venue)?;

    let req = api_scenario::CreateScenarioRequest {
        display_name: a.name,
        description: String::new(),
        asset_class: AssetClass::Crypto,
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
            borrow_bps_per_day: 5.0,
            overrides: Vec::new(),
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

    if dry_run {
        // Validate the request without persisting.
        api_scenario::validate_request(&req, ctx)
            .await
            .map_err(|e| api_to_cli("scenario create --dry-run", e))?;
        if a.json {
            print_json(&serde_json::json!({
                "dry_run": true,
                "action": "create",
                "display_name": req.display_name,
            }))?;
        } else {
            eprintln!("DRY RUN — would create scenario '{}'", req.display_name);
        }
        return Ok(());
    }

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
        ..Default::default()
    };
    let rows = api_scenario::list(ctx, filter)
        .await
        .map_err(|e| api_to_cli("scenario ls", e))?;

    if a.json {
        return print_json(&rows);
    }

    if rows.is_empty() {
        println!("(no scenarios)");
        return Ok(());
    }

    println!("ID\tDISPLAY_NAME\tWINDOW\tSOURCE");
    for s in &rows {
        println!(
            "{}\t{}\t{}..{}\t{:?}",
            s.id,
            s.display_name,
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

    if a.dry_run {
        // Read-only: resolve the source scenario to confirm it exists.
        let source = api_scenario::get(ctx, &a.id)
            .await
            .map_err(|e| api_to_cli("scenario clone --dry-run", e))?;
        let new_name = a.name.as_deref().unwrap_or(&source.display_name);
        eprintln!(
            "DRY RUN — would clone '{}' ({}) to new scenario '{}'",
            source.display_name, a.id, new_name
        );
        return Ok(());
    }

    let mutations = api_scenario::ScenarioMutations {
        display_name: a.name,
        time_window,
        description: None,
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

async fn run_archive(ctx: &ApiContext, id: String, dry_run: bool) -> CliResult<()> {
    if dry_run {
        // Read-only: resolve the scenario to confirm it exists.
        let s = api_scenario::get(ctx, &id)
            .await
            .map_err(|e| api_to_cli("scenario archive --dry-run", e))?;
        eprintln!("DRY RUN — would archive '{}' ({})", s.display_name, id);
        return Ok(());
    }
    api_scenario::archive(ctx, &id)
        .await
        .map_err(|e| api_to_cli("scenario archive", e))?;
    println!("archived {id}");
    Ok(())
}

async fn run_rm(ctx: &ApiContext, id: String, dry_run: bool) -> CliResult<()> {
    if dry_run {
        // Read-only: resolve the scenario to confirm it exists.
        let s = api_scenario::get(ctx, &id)
            .await
            .map_err(|e| api_to_cli("scenario rm --dry-run", e))?;
        eprintln!("DRY RUN — would remove '{}' ({})", s.display_name, id);
        return Ok(());
    }
    api_scenario::delete(ctx, &id)
        .await
        .map_err(|e| api_to_cli("scenario rm", e))?;
    println!("removed {id}");
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

    let quote = format!("{:?}", s.quote_currency).to_uppercase();

    out.push_str(&format!("id: {}\n", s.id));
    out.push_str(&format!("name: {}\n", s.display_name));
    out.push_str(&format!("quote_currency: {}\n", quote));
    out.push_str(&format!(
        "date_window: {}..{}\n",
        s.time_window.start.format("%Y-%m-%d"),
        s.time_window.end.format("%Y-%m-%d"),
    ));
    out.push_str(&format!("warmup_bars: {}\n", s.warmup_bars));

    if let Some(parent_id) = &s.parent_scenario_id {
        out.push_str(&format!("source: cloned_from {}\n", parent_id));
    }

    // Regime labels (migration 021 / track #12).
    if s.regime_label.is_some() || s.volatility_label.is_some() || s.trend_direction.is_some() {
        out.push_str("regime:\n");
        if let Some(ref label) = s.regime_label {
            let derived = if s.regime_derived {
                " (auto)"
            } else {
                " (operator)"
            };
            out.push_str(&format!("  label: {}{}\n", label, derived));
        }
        if let Some(ref vol) = s.volatility_label {
            out.push_str(&format!("  volatility: {}\n", vol));
        }
        if let Some(ref dir) = s.trend_direction {
            out.push_str(&format!("  direction: {}\n", dir));
        }
    }

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
            ..Default::default()
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

/// Compute the decision bar count for a scenario at a caller-supplied timeframe.
///
/// `warmup_bars` are pre-window context and do not reduce decision
/// opportunities.
pub fn scenario_decision_count(s: &Scenario, timeframe_minutes: u32) -> u64 {
    let window_secs = (s.time_window.end - s.time_window.start).num_seconds() as u64;
    let bar_secs = u64::from(timeframe_minutes) * 60;
    if bar_secs == 0 {
        return 0;
    }
    window_secs / bar_secs
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
    pub decision_count: u64,
}

/// Pure selection logic — takes a pre-fetched scenario list and applies
/// regime / decision-count filters plus the cap at the caller-supplied
/// strategy timeframe.
///
/// Exposed as `pub` so unit tests can call it directly without a live DB.
pub fn select_scenarios(
    scenarios: &[Scenario],
    timeframe_minutes: u32,
    regimes: &[String],
    target_decisions: Option<u64>,
    same_decisions: bool,
    max_decisions: Option<u64>,
    count: usize,
) -> Result<Vec<SelectRow>, String> {
    let mut candidates: Vec<&Scenario> = scenarios
        .iter()
        .filter(|s| {
            if !regimes.is_empty() {
                let matched = if let Some(ref col_label) = s.regime_label {
                    regimes
                        .iter()
                        .any(|want| col_label.eq_ignore_ascii_case(want) || col_label.contains(want.as_str()))
                } else {
                    let labels = scenario_regime_labels(s);
                    regimes.iter().any(|want| {
                        labels
                            .iter()
                            .any(|l| l.eq_ignore_ascii_case(want) || l.contains(want.as_str()))
                    })
                };
                if !matched {
                    return false;
                }
            }
            true
        })
        .collect();

    let target_count: u64 = if same_decisions {
        let max = max_decisions.unwrap_or(u64::MAX);
        let counts: Vec<u64> = candidates
            .iter()
            .map(|s| scenario_decision_count(s, timeframe_minutes))
            .filter(|&c| c <= max)
            .collect();

        let mut count_freq: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for c in &counts {
            *count_freq.entry(*c).or_insert(0) += 1;
        }

        let best = count_freq
            .iter()
            .filter(|(_, &freq)| freq >= count)
            .map(|(&c, _)| c)
            .max()
            .or_else(|| count_freq.keys().copied().max());

        match best {
            Some(c) => c,
            None => return Ok(vec![]),
        }
    } else if let Some(t) = target_decisions {
        t
    } else {
        0
    };

    if same_decisions {
        candidates.retain(|s| scenario_decision_count(s, timeframe_minutes) == target_count);
    } else if let Some(t) = target_decisions {
        let lo = (t as f64 * 0.9).floor() as u64;
        let hi = (t as f64 * 1.1).ceil() as u64;
        candidates.retain(|s| {
            let dc = scenario_decision_count(s, timeframe_minutes);
            dc >= lo && dc <= hi
        });
    }

    candidates.sort_by_key(|s| {
        let dc = scenario_decision_count(s, timeframe_minutes);
        if target_decisions.is_some() || same_decisions {
            (dc as i64 - target_count as i64).unsigned_abs()
        } else {
            0u64
        }
    });

    let rows = candidates
        .into_iter()
        .take(count)
        .map(|s| SelectRow {
            id: s.id.clone(),
            name: s.display_name.clone(),
            decision_count: scenario_decision_count(s, timeframe_minutes),
        })
        .collect();

    Ok(rows)
}

async fn run_select(ctx: &ApiContext, a: SelectArgs) -> CliResult<()> {
    let timeframe_minutes = a
        .timeframe
        .as_deref()
        .map(|tf| {
            crate::commands::strategy::parse_timeframe_minutes(tf)
                .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))
        })
        .transpose()?
        .unwrap_or(60);

    if a.target_decisions.is_none() && !a.same_decisions {
        return Err(CliError::usage(anyhow::anyhow!(
            "specify either --target-decisions <N> (Mode A) or --same-decisions --max-decisions <N> (Mode B)"
        )));
    }

    let all = api_scenario::list(
        ctx,
        api_scenario::ListScenariosFilter {
            source: None,
            tags: vec![],
            include_archived: false,
            parent_scenario_id: None,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| api_to_cli("scenarios select", e))?;

    let rows = select_scenarios(
        &all,
        timeframe_minutes,
        &a.regimes,
        a.target_decisions,
        a.same_decisions,
        a.max_decisions,
        a.count,
    )
    .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;

    let effective_format = match a.format {
        Some(fmt) => fmt,
        None if a.json => ListFormat::JsonCompact,
        None => ListFormat::Table,
    };

    match effective_format {
        ListFormat::Json => return print_json(&rows),
        ListFormat::JsonCompact => return print_json_compact(&rows),
        ListFormat::Table => {}
    }

    if rows.is_empty() {
        println!("(no matching scenarios)");
        return Ok(());
    }

    println!("{:<30}  {:<40}  {}", "ID", "NAME", "DECISIONS");
    for r in &rows {
        println!("{:<30}  {:<40}  {}", r.id, r.name, r.decision_count);
    }

    Ok(())
}

// ---- scenario classify ------------------------------------------------------

/// Validate and canonicalise a regime label string against the documented set.
fn validate_regime_label(v: &str) -> Result<(), String> {
    match v {
        "trend" | "chop" | "crash" | "expansion" | "recovery" => Ok(()),
        other => Err(format!(
            "unknown regime_label '{other}'; expected one of: trend | chop | crash | expansion | recovery"
        )),
    }
}

fn validate_volatility_label(v: &str) -> Result<(), String> {
    match v {
        "low" | "normal" | "high" | "extreme" => Ok(()),
        other => Err(format!(
            "unknown volatility_label '{other}'; expected one of: low | normal | high | extreme"
        )),
    }
}

fn validate_trend_direction(v: &str) -> Result<(), String> {
    match v {
        "up" | "down" | "sideways" => Ok(()),
        other => Err(format!(
            "unknown trend_direction '{other}'; expected one of: up | down | sideways"
        )),
    }
}

/// Classify a single scenario by id.  Loads bars from the cache (the cache
/// must have been warmed via `xvn bars fetch`; we don't fetch live here).
/// Returns `Ok(true)` when labels were written (or previewed in dry-run),
/// `Ok(false)` when skipped.
async fn classify_one(ctx: &ApiContext, id: &str, force: bool, dry_run: bool) -> CliResult<bool> {
    let s = api_scenario::get(ctx, id)
        .await
        .map_err(|e| api_to_cli("scenario classify", e))?;

    // Skip operator-set labels unless --force.
    if !force && !s.regime_derived && s.regime_label.is_some() {
        println!("skipped {id} (operator-set labels; use --force to overwrite)");
        return Ok(false);
    }

    // Scenario-only classification uses the default 60m cadence. Eval paths
    // derive timeframe from the strategy.
    let asset_pair = "BTC/USD";
    let granularity = xvision_engine::strategies::bar_granularity_for_cadence(60);
    let cache_key = xvision_engine::eval::bars::compute_cache_key(
        asset_pair,
        granularity,
        s.time_window.start,
        s.time_window.end,
        "alpaca-historical-v1",
    );

    let bars = xvision_engine::eval::bars::load_bars(
        ctx,
        &xvision_engine::eval::bars::BarCacheArgs {
            cache_key,
            asset_pair: asset_pair.to_string(),
            granularity,
            start: s.time_window.start,
            end: s.time_window.end,
            data_source_tag: "alpaca-historical-v1".to_string(),
        },
    )
    .await;

    let bars = match bars {
        Ok(b) => b,
        Err(e) => {
            println!("skipped {id}: bars not available ({e}); run `xvn bars fetch` first");
            return Ok(false);
        }
    };

    if bars.len() < 2 {
        println!("skipped {id}: fewer than 2 bars in cache (window too short for classification)");
        return Ok(false);
    }

    let labels = derive_regime_labels(&bars);

    // regime_labels returns all-None for < 2 bars, but we checked above.
    let regime_label = labels.regime_label.as_deref();
    let volatility_label = labels.volatility_label.as_deref();
    let trend_direction = labels.trend_direction.as_deref();

    if dry_run {
        eprintln!(
            "DRY RUN — would classify {id}: regime={} vol={} direction={}",
            regime_label.unwrap_or("null"),
            volatility_label.unwrap_or("null"),
            trend_direction.unwrap_or("null"),
        );
        return Ok(true);
    }

    xvision_engine::eval::scenario_store::update_regime_labels(
        ctx,
        id,
        regime_label,
        volatility_label,
        trend_direction,
        true, // regime_derived = true
    )
    .await
    .map_err(|e| api_to_cli("scenario classify (write)", e))?;

    println!(
        "classified {id}: regime={} vol={} direction={}",
        regime_label.unwrap_or("null"),
        volatility_label.unwrap_or("null"),
        trend_direction.unwrap_or("null"),
    );
    Ok(true)
}

async fn run_classify(ctx: &ApiContext, a: ClassifyArgs) -> CliResult<()> {
    let dry_run = a.dry_run;
    if a.all {
        // Classify every scenario that either has no regime_label (NULL) or
        // has auto-derived labels and force is set.
        let all = api_scenario::list(
            ctx,
            api_scenario::ListScenariosFilter {
                source: None,
                tags: vec![],
                include_archived: false,
                parent_scenario_id: None,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| api_to_cli("scenario classify --all", e))?;

        let mut classified = 0usize;
        let mut skipped = 0usize;
        for s in &all {
            // Without --force: only classify scenarios with no labels yet.
            if !a.force && !s.regime_derived && s.regime_label.is_some() {
                skipped += 1;
                continue;
            }
            match classify_one(ctx, &s.id, a.force, dry_run).await {
                Ok(true) => classified += 1,
                Ok(false) => skipped += 1,
                Err(e) => {
                    eprintln!("error classifying {}: {}", s.id, e);
                    skipped += 1;
                }
            }
        }
        println!("done: {classified} classified, {skipped} skipped");
        return Ok(());
    }

    // Single id mode.
    let id =
        a.id.ok_or_else(|| CliError::usage(anyhow::anyhow!("specify a scenario id or --all")))?;
    classify_one(ctx, &id, a.force, dry_run).await?;
    Ok(())
}

// ---- scenario set-regime ----------------------------------------------------

async fn run_set_regime(ctx: &ApiContext, a: SetRegimeArgs) -> CliResult<()> {
    // Validate provided values before touching the DB.
    if let Some(ref v) = a.regime {
        validate_regime_label(v).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    }
    if let Some(ref v) = a.volatility {
        validate_volatility_label(v).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    }
    if let Some(ref v) = a.direction {
        validate_trend_direction(v).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    }

    if a.regime.is_none() && a.volatility.is_none() && a.direction.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "specify at least one of --regime, --volatility, --direction"
        )));
    }

    // Fetch current state to merge unset flags with existing values.
    let current = api_scenario::get(ctx, &a.id)
        .await
        .map_err(|e| api_to_cli("scenario set-regime", e))?;

    let regime_label = a.regime.as_deref().or(current.regime_label.as_deref());
    let volatility_label = a.volatility.as_deref().or(current.volatility_label.as_deref());
    let trend_direction = a.direction.as_deref().or(current.trend_direction.as_deref());

    if a.dry_run {
        eprintln!(
            "DRY RUN — would set-regime for {}: regime={} vol={} direction={}",
            a.id,
            regime_label.unwrap_or("null"),
            volatility_label.unwrap_or("null"),
            trend_direction.unwrap_or("null"),
        );
        return Ok(());
    }

    xvision_engine::eval::scenario_store::update_regime_labels(
        ctx,
        &a.id,
        regime_label,
        volatility_label,
        trend_direction,
        false, // regime_derived = false → operator-set
    )
    .await
    .map_err(|e| api_to_cli("scenario set-regime (write)", e))?;

    println!(
        "set regime labels for {}: regime={} vol={} direction={}",
        a.id,
        regime_label.unwrap_or("null"),
        volatility_label.unwrap_or("null"),
        trend_direction.unwrap_or("null"),
    );
    Ok(())
}

// ---- scenario apply / diff --------------------------------------------------

fn load_scenario_file(path: &std::path::Path) -> CliResult<Scenario> {
    let body = std::fs::read_to_string(path)
        .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", path.display())))?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("toml") => {
            toml::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse TOML: {e}")))
        }
        _ => serde_json::from_str(&body).map_err(|e| CliError::usage(anyhow::anyhow!("parse JSON: {e}"))),
    }
}

#[derive(Debug, serde::Serialize)]
struct ScenarioFieldChange {
    field: String,
    old: String,
    new: String,
}

fn scenario_diff(old: &Scenario, new_s: &Scenario) -> Vec<ScenarioFieldChange> {
    let old_v = serde_json::to_value(old).unwrap_or_default();
    let new_v = serde_json::to_value(new_s).unwrap_or_default();
    let mut changes = Vec::new();
    if let (serde_json::Value::Object(om), serde_json::Value::Object(nm)) = (&old_v, &new_v) {
        let keys: std::collections::BTreeSet<_> = om.keys().chain(nm.keys()).collect();
        for key in keys {
            let ov = om.get(key).unwrap_or(&serde_json::Value::Null);
            let nv = nm.get(key).unwrap_or(&serde_json::Value::Null);
            if ov != nv {
                let compact = |v: &serde_json::Value| -> String {
                    let s = serde_json::to_string(v).unwrap_or_default();
                    if s.len() > 120 {
                        format!("{}…", &s[..117])
                    } else {
                        s
                    }
                };
                changes.push(ScenarioFieldChange {
                    field: key.clone(),
                    old: compact(ov),
                    new: compact(nv),
                });
            }
        }
    }
    changes
}

async fn run_scenario_apply(ctx: &ApiContext, a: ScenarioApplyArgs) -> CliResult<()> {
    let new_s = load_scenario_file(&a.file)?;
    let id = new_s.id.clone();

    match api_scenario::get(ctx, &id).await {
        Ok(existing) => {
            let changes = scenario_diff(&existing, &new_s);
            if a.json {
                crate::io::print_json(&serde_json::json!({
                    "action": "exists",
                    "scenario_id": id,
                    "note": "scenarios are immutable post-insert",
                    "drift": changes,
                }))?;
            } else {
                println!("scenario {id} already exists (immutable — no update applied)");
                if changes.is_empty() {
                    println!("  (file matches stored version)");
                } else {
                    println!("  drift detected ({} field(s)):", changes.len());
                    for c in &changes {
                        println!("  {}: {} → {}", c.field, c.old, c.new);
                    }
                }
            }
        }
        Err(_) => {
            let req = scenario_to_create_request(&new_s);
            if a.dry_run {
                if a.json {
                    crate::io::print_json(
                        &serde_json::json!({ "dry_run": true, "action": "create", "scenario_id": id, "display_name": new_s.display_name }),
                    )?;
                } else {
                    eprintln!(
                        "DRY RUN — would create scenario '{}' ({})",
                        new_s.display_name, id
                    );
                }
                return Ok(());
            }
            let created = api_scenario::create(ctx, req)
                .await
                .map_err(|e| api_to_cli("scenario apply (create)", e))?;
            if a.json {
                crate::io::print_json(
                    &serde_json::json!({ "action": "created", "scenario_id": created.id }),
                )?;
            } else {
                println!("created {} ({})", created.id, created.display_name);
            }
        }
    }
    Ok(())
}

async fn run_scenario_diff(ctx: &ApiContext, a: ScenarioDiffArgs) -> CliResult<()> {
    let new_s = load_scenario_file(&a.file)?;
    let id = new_s.id.clone();

    match api_scenario::get(ctx, &id).await {
        Ok(existing) => {
            let changes = scenario_diff(&existing, &new_s);
            if a.json {
                crate::io::print_json(&serde_json::json!({ "scenario_id": id, "changes": changes }))?;
            } else {
                println!("diff for scenario {id}");
                if changes.is_empty() {
                    println!("  (no changes)");
                } else {
                    for c in &changes {
                        println!("  {}: {} → {}", c.field, c.old, c.new);
                    }
                }
            }
        }
        Err(_) => {
            if a.json {
                crate::io::print_json(
                    &serde_json::json!({ "scenario_id": id, "changes": [], "note": "not found — would create" }),
                )?;
            } else {
                println!("scenario {id} not found — would create");
            }
        }
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
        AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, DataSource, Fees, FillModel,
        LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
        ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
    };

    use std::str::FromStr;

    /// Build a minimal `Scenario` for testing.  `window_secs` is the number of
    /// seconds covered by the time window (start is fixed; end = start + window_secs).
    fn make_scenario(
        id: &str,
        _asset_sym: &str,
        granularity: &str,
        window_secs: i64,
        warmup_bars: u32,
        regime_tags: &[&str],
    ) -> Scenario {
        let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::seconds(window_secs);
                Scenario {
            id: id.to_string(),
            parent_scenario_id: None,
            source: ScenarioSource::User,
            display_name: format!("test-{id}"),
            description: String::new(),
            tags: regime_tags.iter().map(|t| format!("regime:{t}")).collect(),
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            timezone: "UTC".to_string(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps: 25,
                },
                slippage: SlippageModel::None,
                latency: LatencyModel {
                    decision_to_fill_ms: 0,
                },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
                overrides: Vec::new(),
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: id.to_string(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            created_by: "test".to_string(),
            archived_at: None,
            // Pre-existing baseline fix: `venue_label`/`safety_limits`
            // were added upstream but this helper wasn't updated.
            // Unblocks workspace-test verification for the
            // strategy-template-registry-removal contract.
            venue_label: xvision_engine::safety::VenueLabel::Paper,
            safety_limits: None,
        }
    }

    // ── scenario_decision_count ───────────────────────────────────────────

    #[test]
    fn decision_count_1h_gran_with_200_warmup() {
        // 1h = 3600 s. Window = 300 hours = 300 decision bars.
        // Warmup bars are pre-window context, so they do not reduce the
        // scenario's decision count.
        let s = make_scenario("sc1", "ETH", "1h", 300 * 3_600, 200, &[]);
        assert_eq!(scenario_decision_count(&s, 60), 300);
    }

    #[test]
    fn decision_count_4h_gran_with_0_warmup() {
        // 4h = 14400 s. Window = 48 bars (8 days).
        let s = make_scenario("sc2", "BTC", "4h", 48 * 4 * 3_600, 0, &[]);
        assert_eq!(scenario_decision_count(&s, 60), 48);
    }

    #[test]
    fn decision_count_ignores_warmup_larger_than_window() {
        // Warmup > total bars is valid because warmup is pre-window context;
        // the scenario still has 5 decision bars in its own window.
        let s = make_scenario("sc3", "SOL", "1h", 5 * 3_600, 200, &[]);
        assert_eq!(scenario_decision_count(&s, 60), 5);
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
        // 50-decision scenario; target = 200 (±10 % → 180..220) → no match.
        let s1 = make_scenario("sc1", "ETH", "1h", 50 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1], 60, &[], Some(200), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn mode_a_matches_within_ten_percent_tolerance() {
        // 1h window: 100 decision bars. Target = 100 → ±10 % = 90..110 → match.
        let s1 = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1], 60, &[], Some(100), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision_count, 100);
    }

    #[test]
    fn mode_a_timeframe_filter_excludes_wrong_granularity() {
        // 4h scenario; filter by 1h → excluded.
        let s = make_scenario("sc1", "ETH", "4h", 200 * 4 * 3_600, 0, &[]);
        // 60 min = 1h filter
        let rows = select_scenarios(&[s], 60, &[], Some(100), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn mode_a_timeframe_filter_includes_matching_granularity() {
        // 4h scenario; filter by 4h (240 min) → included.
        // 200 total 4h bars. target=200 → within ±10 %.
        let s = make_scenario("sc1", "ETH", "4h", 200 * 4 * 3_600, 0, &[]);
        let rows = select_scenarios(&[s], 240, &[], Some(200), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn mode_a_regime_filter_excludes_non_matching() {
        let s = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &["trending_bear"]);
        let rows = select_scenarios(&[s], 60, &["bull".to_string()], Some(100), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn mode_a_regime_filter_includes_partial_match() {
        // "bull" is a substring of "trending_bull" → should match.
        let s = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &["trending_bull"]);
        let rows = select_scenarios(&[s], 60, &["bull".to_string()], Some(100), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn mode_a_count_cap_respected() {
        // 4 scenarios all matching; count=2 → only 2 returned.
        let s1 = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 100 * 3_600, 200, &[]);
        let s3 = make_scenario("sc3", "SOL", "1h", 100 * 3_600, 200, &[]);
        let s4 = make_scenario("sc4", "DOGE", "1h", 100 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1, s2, s3, s4], 60, &[], Some(100), false, None, 2).unwrap();
        assert_eq!(rows.len(), 2);
    }

    // ── select_scenarios Mode B (same-decisions) ──────────────────────────

    #[test]
    fn mode_b_finds_common_count() {
        // Two scenarios with 100 decisions, one with 50 — common count = 100.
        let s1 = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 100 * 3_600, 200, &[]);
        let s3 = make_scenario("sc3", "SOL", "1h", 50 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1, s2, s3], 60, &[], None, true, Some(200), 2).unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(
                r.decision_count, 100,
                "all rows must have 100 decisions in mode B"
            );
        }
    }

    #[test]
    fn mode_b_max_decisions_cap_observed() {
        // s1 → 200 decisions, s2 → 100 decisions, s3 → 100 decisions.
        // max_decisions = 150 → s1 excluded, common count among s2/s3 = 100.
        let s1 = make_scenario("sc1", "ETH", "1h", 200 * 3_600, 200, &[]);
        let s2 = make_scenario("sc2", "BTC", "1h", 100 * 3_600, 200, &[]);
        let s3 = make_scenario("sc3", "SOL", "1h", 100 * 3_600, 200, &[]);
        let rows = select_scenarios(&[s1, s2, s3], 60, &[], None, true, Some(150), 4).unwrap();
        assert!(!rows.iter().any(|r| r.id == "sc1"), "sc1 should be excluded");
        for r in &rows {
            assert_eq!(r.decision_count, 100);
        }
    }

    #[test]
    fn mode_b_returns_empty_when_no_candidates_under_max() {
        let s1 = make_scenario("sc1", "ETH", "1h", 200 * 3_600, 200, &[]);
        // 200 decisions; max_decisions = 50 → excluded → empty.
        let rows = select_scenarios(&[s1], 60, &[], None, true, Some(50), 4).unwrap();
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
        let rows = select_scenarios(&[s1, s2], 60, &[], None, false, None, 4).unwrap();
        // Both returned; no decision-count filter applied.
        assert_eq!(rows.len(), 2);
    }

    // ── regime_label column matching (migration 021) ─────────────────────

    #[test]
    fn regime_column_match_takes_priority_over_tag_match() {
        // Scenario has regime_label = "expansion" in the column, but its tag
        // says "bear" — column wins and it should match "expansion", not "bear".
        let mut s = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &["bear"]);
        s.regime_label = Some("expansion".to_string());

        let rows_expansion = select_scenarios(
            &[s.clone()],
            60,
            &["expansion".to_string()],
            Some(100),
            false,
            None,
            4,
        )
        .unwrap();
        assert_eq!(rows_expansion.len(), 1, "column 'expansion' should match");

        // Bear tag should NOT match since column overrides.
        let rows_bear =
            select_scenarios(&[s], 60, &["bear".to_string()], Some(100), false, None, 4).unwrap();
        assert!(
            rows_bear.is_empty(),
            "tag 'bear' should not match when column says 'expansion'"
        );
    }

    #[test]
    fn tag_fallback_when_column_is_null() {
        // Scenario has no regime_label column (None), but has regime tag → tag fallback.
        let s = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &["trending_bull"]);
        assert!(s.regime_label.is_none());

        let rows = select_scenarios(&[s], 60, &["bull".to_string()], Some(100), false, None, 4).unwrap();
        assert_eq!(
            rows.len(),
            1,
            "tag fallback should match 'bull' substring in 'trending_bull'"
        );
    }

    #[test]
    fn scenario_without_regime_excluded_when_regime_filter_set() {
        // No regime in column AND no regime tag → excluded when filter is active.
        let s = make_scenario("sc1", "ETH", "1h", 100 * 3_600, 200, &[]);
        let rows =
            select_scenarios(&[s], 60, &["expansion".to_string()], Some(100), false, None, 4).unwrap();
        assert!(rows.is_empty());
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

    // ── classify and set-regime subcommands are registered ───────────────

    #[test]
    fn scenario_classify_subcommand_registered() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        let classify = scenario.find_subcommand("classify");
        assert!(
            classify.is_some(),
            "`xvn scenario classify` subcommand must be registered"
        );
    }

    #[test]
    fn scenario_set_regime_subcommand_registered() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        let set_regime = scenario.find_subcommand("set-regime");
        assert!(
            set_regime.is_some(),
            "`xvn scenario set-regime` subcommand must be registered"
        );
    }

    // ── validate_regime_label ──────────────────────────────────────────────

    #[test]
    fn valid_regime_labels_accepted() {
        for label in &["trend", "chop", "crash", "expansion", "recovery"] {
            assert!(
                super::validate_regime_label(label).is_ok(),
                "expected '{label}' to be valid"
            );
        }
    }

    #[test]
    fn invalid_regime_label_rejected() {
        assert!(super::validate_regime_label("bull").is_err());
        assert!(super::validate_regime_label("bear").is_err());
        assert!(super::validate_regime_label("").is_err());
    }

    #[test]
    fn valid_volatility_labels_accepted() {
        for label in &["low", "normal", "high", "extreme"] {
            assert!(super::validate_volatility_label(label).is_ok());
        }
    }

    #[test]
    fn valid_trend_directions_accepted() {
        for dir in &["up", "down", "sideways"] {
            assert!(super::validate_trend_direction(dir).is_ok());
        }
    }
}

#[cfg(test)]
pub mod apply_diff {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::str::FromStr;
    use xvision_core::Capital;
    use xvision_engine::eval::scenario::{
        AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, DataSource, Fees, FillModel,
        LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
        ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
    };

    fn make_test_scenario(id: &str) -> Scenario {
        let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::hours(100);
        Scenario {
            id: id.to_string(),
            parent_scenario_id: None,
            source: ScenarioSource::User,
            display_name: format!("test-{id}"),
            description: String::new(),
            tags: vec![],
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            timezone: "UTC".to_string(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps: 25,
                },
                slippage: SlippageModel::None,
                latency: LatencyModel {
                    decision_to_fill_ms: 0,
                },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
                overrides: Vec::new(),
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: id.to_string(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars: 0,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            created_by: "test".to_string(),
            archived_at: None,
            venue_label: xvision_engine::safety::VenueLabel::Paper,
            safety_limits: None,
        }
    }

    // ── scenario_diff pure-fn tests ──────────────────────────────────────────

    #[test]
    fn scenario_diff_identical_returns_empty() {
        let s = make_test_scenario("sc1");
        let changes = scenario_diff(&s, &s);
        assert!(changes.is_empty(), "identical scenarios must produce no diff");
    }

    #[test]
    fn scenario_diff_detects_display_name_change() {
        let old = make_test_scenario("sc1");
        let mut new_s = make_test_scenario("sc1");
        new_s.display_name = "renamed".to_string();
        let changes = scenario_diff(&old, &new_s);
        let c = changes
            .iter()
            .find(|c| c.field == "display_name")
            .expect("display_name change must appear in diff");
        assert!(c.old.contains("test-sc1"), "old must reflect original name");
        assert!(c.new.contains("renamed"), "new must reflect updated name");
    }

    #[test]
    fn scenario_diff_long_value_truncated_with_ellipsis() {
        let mut old = make_test_scenario("sc1");
        let mut new_s = make_test_scenario("sc1");
        old.description = "short".to_string();
        new_s.description = "x".repeat(200);
        let changes = scenario_diff(&old, &new_s);
        let c = changes
            .iter()
            .find(|c| c.field == "description")
            .expect("description change must appear in diff");
        assert!(
            c.new.ends_with('…'),
            "value longer than 120 chars must be truncated with ellipsis"
        );
    }

    #[test]
    fn scenario_diff_only_changed_fields_reported() {
        let old = make_test_scenario("sc1");
        let mut new_s = make_test_scenario("sc1");
        new_s.warmup_bars = 42;
        let changes = scenario_diff(&old, &new_s);
        assert_eq!(changes.len(), 1, "only the mutated field should appear");
        assert_eq!(changes[0].field, "warmup_bars");
    }

    // ── load_scenario_file tests ─────────────────────────────────────────────

    #[test]
    fn load_scenario_file_missing_returns_error() {
        let result = load_scenario_file(std::path::Path::new("/nonexistent/path/sc.json"));
        assert!(result.is_err(), "missing file must return an error");
    }

    #[test]
    fn load_scenario_file_invalid_json_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, b"not-valid-json{{").unwrap();
        let result = load_scenario_file(&path);
        assert!(result.is_err(), "invalid JSON must return a parse error");
    }

    #[test]
    fn load_scenario_file_toml_extension_routes_to_toml_parser() {
        // Valid JSON is not valid TOML for this struct — the TOML parser rejects it.
        // The assertion is that the `.toml` extension triggers the TOML code path (returns Err).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sc.toml");
        std::fs::write(&path, b"{ \"id\": 1 }").unwrap();
        let result = load_scenario_file(&path);
        assert!(result.is_err(), ".toml extension must use the TOML parser");
    }

    #[test]
    fn load_scenario_file_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sc.json");
        let original = make_test_scenario("roundtrip-id");
        let json = serde_json::to_string(&original).unwrap();
        std::fs::write(&path, json.as_bytes()).unwrap();
        let loaded = load_scenario_file(&path).expect("valid JSON must load successfully");
        assert_eq!(loaded.id, "roundtrip-id");
        assert_eq!(loaded.display_name, original.display_name);
        assert_eq!(loaded.warmup_bars, original.warmup_bars);
    }

    #[test]
    fn load_scenario_file_unknown_extension_falls_back_to_json() {
        // No extension → falls through to the JSON branch.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sc");
        let original = make_test_scenario("noext-id");
        let json = serde_json::to_string(&original).unwrap();
        std::fs::write(&path, json.as_bytes()).unwrap();
        let loaded = load_scenario_file(&path).expect("no-extension file must try JSON");
        assert_eq!(loaded.id, "noext-id");
    }

    // ── CLI subcommand registration ──────────────────────────────────────────

    #[test]
    fn scenario_apply_subcommand_registered() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        assert!(
            scenario.find_subcommand("apply").is_some(),
            "`xvn scenario apply` must be registered"
        );
    }

    #[test]
    fn scenario_diff_subcommand_registered() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        assert!(
            scenario.find_subcommand("diff").is_some(),
            "`xvn scenario diff` must be registered"
        );
    }

    #[test]
    fn scenario_apply_has_dry_run_and_json_flags() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        let apply = scenario.find_subcommand("apply").expect("apply subcommand");
        let longs: Vec<_> = apply.get_arguments().filter_map(|a| a.get_long()).collect();
        assert!(longs.contains(&"dry-run"), "`apply` must have --dry-run flag");
        assert!(longs.contains(&"json"), "`apply` must have --json flag");
    }

    #[test]
    fn scenario_diff_has_json_flag() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let scenario = cmd.find_subcommand("scenario").expect("scenario subcommand");
        let diff = scenario.find_subcommand("diff").expect("diff subcommand");
        let has_json = diff.get_arguments().any(|a| a.get_long() == Some("json"));
        assert!(has_json, "`diff` must have --json flag");
    }
}
