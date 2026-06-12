//! `xvn bars` — manage the local Alpaca historical bars cache.
//!
//! Operations:
//! - `fetch`: warm the cache by fetching a window from Alpaca and storing
//!   it locally. Returns the `cache_key` so downstream callers can reuse
//!   the deterministic identifier.
//! - `ls`: list cached entries (key, asset, granularity, window, count).
//! - `rm <cache_key>`: drop one entry.
//! - `gc --older-than <Nd>`: evict entries whose `fetched_at` is older
//!   than the given duration. Only `Nd` (days) supported in v1.
//!
//! The list/rm/gc helpers SELECT/DELETE against the `bars_cache` table
//! directly via the public `ctx.db` pool. `fetch` goes through
//! `engine::eval::bars::load_bars` so the gzip + single-flight invariants
//! are preserved on the write path.

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Args, Subcommand};

use std::sync::Arc;

use xvision_core::config::AlpacaData;
use xvision_core::AssetSymbol;
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::settings::brokers;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::bars::{self, check_bar_coverage, compute_cache_key, BarCacheArgs, CoverageReport};

use crate::exit::{CliError, CliResult};

/// Canonical bar `data_source` tag for non-warmup historical loads. Mirrors the
/// constant the eval executor uses (`api::eval::load_bars_for_scenario`) so the
/// coverage view here matches what an eval run will actually read.
const HISTORICAL_DATA_SOURCE_TAG: &str = "alpaca-historical-v1";

#[derive(Args, Debug)]
pub struct BarsCmd {
    #[command(subcommand)]
    pub op: BarsOp,
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum BarsOp {
    /// Fetch bars from Alpaca and cache locally.
    Fetch(FetchArgs),
    /// List cached entries.
    #[command(visible_alias = "list")]
    Ls(LsArgs),
    /// Remove a cached entry by `cache_key`.
    Rm {
        /// The blake3 cache key (full hex string).
        cache_key: String,
    },
    /// Evict entries older than the given duration (e.g. `90d`).
    Gc {
        #[arg(long)]
        older_than: String,
    },
}

#[derive(Args, Debug, Default, Clone)]
pub struct LsArgs {
    /// Only show cached windows for this asset (accepts BTC or BTC/USD).
    #[arg(long)]
    pub asset: Option<String>,
    /// Only show cached windows for this bar granularity.
    #[arg(long, visible_alias = "timeframe")]
    pub granularity: Option<String>,
}

#[derive(Args, Debug)]
pub struct FetchArgs {
    /// Asset ticker (BTC, ETH, …).
    #[arg(long)]
    pub asset: String,
    /// Window start date (UTC, midnight).
    #[arg(long)]
    pub from: NaiveDate,
    /// Window end date (UTC, midnight).
    #[arg(long)]
    pub to: NaiveDate,
    /// Bar granularity. Supports Alpaca bars:
    /// 1-59m, 1-23h, 1d, 1w, 1/2/3/4/6/12mo.
    #[arg(long, default_value = "1h")]
    pub granularity: String,
}

pub async fn run(cmd: BarsCmd) -> CliResult<()> {
    let (ctx, rpm) = open_ctx(cmd.xvn_home.clone()).await.map_err(CliError::upstream)?;
    match cmd.op {
        BarsOp::Fetch(a) => run_fetch(&ctx, a, rpm).await,
        BarsOp::Ls(a) => run_ls(&ctx, a).await,
        BarsOp::Rm { cache_key } => run_rm(&ctx, cache_key).await,
        BarsOp::Gc { older_than } => run_gc(&ctx, older_than).await,
    }
}

/// Open the API context and resolve the Alpaca rate-limit (requests/minute).
/// The rpm is surfaced alongside the context so `run_fetch` can build a
/// credentialed fetcher (U16: app-store creds, not ENV) tuned to the same
/// limit the default fetcher would use.
async fn open_ctx(override_path: Option<PathBuf>) -> Result<(ApiContext, u32)> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))?;
    // Thread the rate-limit knob from `config/default.toml` if present.
    // Missing/unreadable config falls back to the default `AlpacaBarsFetcher`
    // constructed inside `ApiContext::open` (200 rpm) — `xvn bars` never
    // wants to fail boot on a missing config file.
    let mut rpm = AlpacaData::DEFAULT_RATE_LIMIT_RPM;
    let cfg_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config/default.toml");
    if cfg_path.is_file() {
        if let Ok(cfg) = xvision_core::config::load_runtime(&cfg_path) {
            rpm = cfg.data.alpaca.rate_limit_rpm;
            return Ok((ctx.with_alpaca_rate_limit_rpm(rpm), rpm));
        }
    }
    Ok((ctx, rpm))
}

async fn run_fetch(ctx: &ApiContext, a: FetchArgs, rpm: u32) -> CliResult<()> {
    let asset = AssetSymbol::from_str(&a.asset).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    let granularity =
        BarGranularity::from_str(&a.granularity).map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    let start = a
        .from
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --from date")))?
        .and_utc();
    let end =
        a.to.and_hms_opt(0, 0, 0)
            .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --to date")))?
            .and_utc();
    if end <= start {
        return Err(CliError::usage(anyhow::anyhow!(
            "--to must be strictly after --from"
        )));
    }
    let asset_pair = asset.as_alpaca_pair();
    let data_source_tag = HISTORICAL_DATA_SOURCE_TAG;
    let cache_key = compute_cache_key(&asset_pair, granularity, start, end, data_source_tag);

    // U16: preflight the cache before touching any credentials. In the common
    // (cached) case we must NOT resolve creds at all — only a real miss needs a
    // fetch. `check_bar_coverage` is cache-only.
    let coverage = check_bar_coverage(ctx, &asset_pair, granularity, start, end, data_source_tag)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;

    // U16(c): when a fetch is genuinely needed, resolve Alpaca credentials from
    // the app's broker store (NOT ENV) via `build_credentialed_fetcher`, and
    // attach the resulting fetcher to the context so `load_bars`' miss path uses
    // it. A truly-missing credential fails fast here with a dashboard-pointing
    // message rather than hanging. The 30s fetch timeout lives inside the
    // fetcher's HTTP client (agent C / `xvision-data`).
    let ctx_owned;
    let fetch_ctx: &ApiContext = if coverage.fully_covered {
        // Cache hit for the whole window — `load_bars` will never call upstream,
        // so leave the default fetcher in place and never resolve creds.
        ctx
    } else {
        let fetcher = brokers::build_credentialed_fetcher(&ctx.xvn_home, rpm)
            .await
            .map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
        ctx_owned = ctx.clone().with_alpaca_fetcher(Arc::new(fetcher));
        &ctx_owned
    };

    let args = BarCacheArgs {
        cache_key: cache_key.clone(),
        asset_pair,
        granularity,
        start,
        end,
        data_source_tag: data_source_tag.into(),
    };
    let out = bars::load_bars(fetch_ctx, &args)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
    println!("Fetched {} bars (cache_key={cache_key})", out.len());
    Ok(())
}

async fn run_ls(ctx: &ApiContext, args: LsArgs) -> CliResult<()> {
    let rows = filter_bars_cache_rows(list_bars_cache(ctx).await?, &args);
    if rows.is_empty() {
        println!("(no cached bar windows)");
        return Ok(());
    }
    // Per-entry lines (unchanged surface).
    for r in &rows {
        println!(
            "{}  {}  {}  {}..{}  {} bars",
            r.cache_key, r.asset, r.granularity, r.window_start, r.window_end, r.bar_count
        );
    }

    // U16: UNION coverage section. Group entries by (asset, granularity,
    // data_source) and show the contiguous covered windows + any internal gaps
    // for each group, so adjacent cache entries read as one continuous window
    // (the multi-segment case that silently hung evals). This is computed from
    // the rows already in hand — no extra DB round-trip and no creds.
    let report = render_coverage(&rows);
    if !report.is_empty() {
        println!();
        println!("Coverage (union of adjacent/overlapping entries):");
        print!("{report}");
    }
    Ok(())
}

/// Build the human-readable coverage section for `xvn bars ls` from the cache
/// rows. Pure (string in / string out) so it is directly unit-testable without
/// a DB. Returns an empty string when no rows have parseable windows.
fn render_coverage(rows: &[BarsCacheRow]) -> String {
    use std::collections::BTreeMap;

    // Group by (asset, granularity, data_source). BTreeMap keeps a stable,
    // deterministic ordering for the rendered output (and the tests).
    let mut groups: BTreeMap<(String, String, String), Vec<bars::CachedWindow>> = BTreeMap::new();
    for r in rows {
        let (Ok(start), Ok(end)) = (
            DateTime::parse_from_rfc3339(&r.window_start),
            DateTime::parse_from_rfc3339(&r.window_end),
        ) else {
            continue;
        };
        groups
            .entry((r.asset.clone(), r.granularity.clone(), r.data_source.clone()))
            .or_default()
            .push(bars::CachedWindow {
                cache_key: r.cache_key.clone(),
                start: start.with_timezone(&Utc),
                end: end.with_timezone(&Utc),
            });
    }

    let mut out = String::new();
    for ((asset, granularity, data_source), windows) in groups {
        let segments = bars::merge_segments(windows);
        if segments.is_empty() {
            continue;
        }
        // Report coverage over the full span [earliest start, latest end] so
        // operators see internal gaps between non-adjacent entries at a glance.
        let span_start = segments.first().map(|s| s.start).unwrap();
        let span_end = segments.last().map(|s| s.end).unwrap();
        let report: CoverageReport = bars::coverage_for(&segments, span_start, span_end);

        out.push_str(&format!("  {asset} {granularity} [{data_source}]\n"));
        for seg in &report.covered {
            out.push_str(&format!(
                "    covered: {}..{}  ({} {})\n",
                fmt_ts(seg.start),
                fmt_ts(seg.end),
                seg.cache_keys.len(),
                if seg.cache_keys.len() == 1 {
                    "entry"
                } else {
                    "entries"
                },
            ));
        }
        if report.gaps.is_empty() {
            out.push_str("    gaps:    none\n");
        } else {
            for gap in &report.gaps {
                out.push_str(&format!(
                    "    GAP:     {}..{}\n",
                    fmt_ts(gap.start),
                    fmt_ts(gap.end)
                ));
            }
        }
    }
    out
}

/// Compact UTC timestamp for the coverage section (RFC3339, no fractional secs).
fn fmt_ts(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

async fn run_rm(ctx: &ApiContext, cache_key: String) -> CliResult<()> {
    delete_bars_cache(ctx, &cache_key).await?;
    println!("removed {cache_key}");
    Ok(())
}

async fn run_gc(ctx: &ApiContext, older_than: String) -> CliResult<()> {
    let cutoff = parse_duration(&older_than)?;
    let cutoff_ts = Utc::now() - cutoff;
    let n = gc_bars_cache(ctx, cutoff_ts).await?;
    println!("evicted {n} entries older than {older_than}");
    Ok(())
}

fn parse_duration(s: &str) -> CliResult<chrono::Duration> {
    if let Some(d) = s.strip_suffix('d') {
        let n: i64 = d
            .parse()
            .map_err(|_| CliError::usage(anyhow::anyhow!("bad duration '{s}'")))?;
        if n < 0 {
            return Err(CliError::usage(anyhow::anyhow!(
                "duration must be non-negative: '{s}'"
            )));
        }
        Ok(chrono::Duration::days(n))
    } else {
        Err(CliError::usage(anyhow::anyhow!("only Nd supported (got '{s}')")))
    }
}

// ---- store helpers ----
// Inline rather than in `eval::bars` because they're CLI-shaped (list /
// delete-by-key / gc-by-cutoff) and don't carry the singleflight + gzip
// invariants that `load_bars` enforces. If a second caller (MCP) needs
// them, lift into engine then.

struct BarsCacheRow {
    cache_key: String,
    asset: String,
    granularity: String,
    window_start: String,
    window_end: String,
    bar_count: i64,
    data_source: String,
}

async fn list_bars_cache(ctx: &ApiContext) -> CliResult<Vec<BarsCacheRow>> {
    let rows: Vec<(String, String, String, String, String, i64, String)> = sqlx::query_as(
        "SELECT cache_key, asset, granularity, window_start, window_end, bar_count, data_source \
         FROM bars_cache ORDER BY fetched_at DESC",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("list_bars_cache: {e}")))?;
    Ok(rows
        .into_iter()
        .map(
            |(cache_key, asset, granularity, window_start, window_end, bar_count, data_source)| {
                BarsCacheRow {
                    cache_key,
                    asset,
                    granularity,
                    window_start,
                    window_end,
                    bar_count,
                    data_source,
                }
            },
        )
        .collect())
}

fn filter_bars_cache_rows(rows: Vec<BarsCacheRow>, args: &LsArgs) -> Vec<BarsCacheRow> {
    rows.into_iter()
        .filter(|row| {
            args.asset
                .as_deref()
                .map(|asset| asset_filter_matches(&row.asset, asset))
                .unwrap_or(true)
        })
        .filter(|row| {
            args.granularity
                .as_deref()
                .map(|granularity| granularity_filter_matches(&row.granularity, granularity))
                .unwrap_or(true)
        })
        .collect()
}

fn asset_filter_matches(row_asset: &str, filter: &str) -> bool {
    if row_asset.eq_ignore_ascii_case(filter) {
        return true;
    }
    AssetSymbol::from_str(filter)
        .map(|asset| row_asset.eq_ignore_ascii_case(&asset.as_alpaca_pair()))
        .unwrap_or(false)
}

fn granularity_filter_matches(row_granularity: &str, filter: &str) -> bool {
    if row_granularity.eq_ignore_ascii_case(filter) {
        return true;
    }
    BarGranularity::from_str(filter)
        .map(|granularity| {
            row_granularity.eq_ignore_ascii_case(&granularity.as_alpaca_str())
                || row_granularity.eq_ignore_ascii_case(&granularity.canonical())
        })
        .unwrap_or(false)
}

async fn delete_bars_cache(ctx: &ApiContext, cache_key: &str) -> CliResult<()> {
    sqlx::query("DELETE FROM bars_cache WHERE cache_key = ?")
        .bind(cache_key)
        .execute(&ctx.db)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("delete_bars_cache: {e}")))?;
    Ok(())
}

async fn gc_bars_cache(ctx: &ApiContext, cutoff: DateTime<Utc>) -> CliResult<u64> {
    let res = sqlx::query("DELETE FROM bars_cache WHERE fetched_at < ?")
        .bind(cutoff.to_rfc3339())
        .execute(&ctx.db)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("gc_bars_cache: {e}")))?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_cache_key_is_deterministic() {
        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let a = compute_cache_key("BTC/USD", BarGranularity::Hour1, start, end, "src-v1");
        let b = compute_cache_key("BTC/USD", BarGranularity::Hour1, start, end, "src-v1");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // blake3 hex = 32 bytes -> 64 hex chars
    }

    #[test]
    fn compute_cache_key_varies_with_inputs() {
        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let base = compute_cache_key("BTC/USD", BarGranularity::Hour1, start, end, "src-v1");
        let alt_asset = compute_cache_key("ETH/USD", BarGranularity::Hour1, start, end, "src-v1");
        let alt_gran = compute_cache_key("BTC/USD", BarGranularity::Day1, start, end, "src-v1");
        let alt_src = compute_cache_key("BTC/USD", BarGranularity::Hour1, start, end, "src-v2");
        assert_ne!(base, alt_asset);
        assert_ne!(base, alt_gran);
        assert_ne!(base, alt_src);
    }

    #[test]
    fn parse_duration_accepts_days() {
        let d = parse_duration("90d").unwrap();
        assert_eq!(d, chrono::Duration::days(90));
    }

    #[test]
    fn parse_duration_rejects_other_units() {
        assert!(parse_duration("90h").is_err());
        assert!(parse_duration("90").is_err());
        assert!(parse_duration("90dx").is_err());
    }

    #[test]
    fn parse_duration_rejects_negative() {
        assert!(parse_duration("-1d").is_err());
    }

    fn row(asset: &str, gran: &str, start: &str, end: &str, key: &str) -> BarsCacheRow {
        BarsCacheRow {
            cache_key: key.into(),
            asset: asset.into(),
            granularity: gran.into(),
            window_start: start.into(),
            window_end: end.into(),
            bar_count: 1,
            data_source: HISTORICAL_DATA_SOURCE_TAG.into(),
        }
    }

    #[test]
    fn bars_ls_accepts_asset_and_granularity_filters() {
        use clap::Parser;

        let cli = crate::Cli::try_parse_from([
            "xvn",
            "bars",
            "ls",
            "--asset",
            "BTC/USD",
            "--granularity",
            "1Hour",
        ])
        .expect("bars ls filters must parse");
        let crate::Command::Bars(cmd) = cli.command else {
            panic!("expected bars command");
        };
        let BarsOp::Ls(args) = cmd.op else {
            panic!("expected bars ls op");
        };
        assert_eq!(args.asset.as_deref(), Some("BTC/USD"));
        assert_eq!(args.granularity.as_deref(), Some("1Hour"));
    }

    #[test]
    fn bars_ls_filters_rows_by_asset_and_granularity() {
        let rows = vec![
            row(
                "BTC/USD",
                "1Hour",
                "2025-01-01T00:00:00Z",
                "2025-01-02T00:00:00Z",
                "btc-hour",
            ),
            row(
                "ETH/USD",
                "1Hour",
                "2025-01-01T00:00:00Z",
                "2025-01-02T00:00:00Z",
                "eth-hour",
            ),
            row(
                "BTC/USD",
                "1Day",
                "2025-01-01T00:00:00Z",
                "2025-01-02T00:00:00Z",
                "btc-day",
            ),
        ];
        let args = LsArgs {
            asset: Some("BTC/USD".into()),
            granularity: Some("1Hour".into()),
        };
        let filtered = filter_bars_cache_rows(rows, &args);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].cache_key, "btc-hour");
    }

    #[test]
    fn render_coverage_merges_adjacent_entries_into_one_window() {
        // U16: two adjacent entries (one ends exactly where the next begins)
        // must read as a single continuous covered window with no gap.
        let rows = vec![
            row(
                "BTC/USD",
                "1Hour",
                "2025-01-01T00:00:00Z",
                "2025-04-01T00:00:00Z",
                "aaa",
            ),
            row(
                "BTC/USD",
                "1Hour",
                "2025-04-01T00:00:00Z",
                "2025-06-01T00:00:00Z",
                "bbb",
            ),
        ];
        let out = render_coverage(&rows);
        assert!(out.contains("BTC/USD 1Hour"), "group header missing: {out}");
        // Single merged segment spanning both entries.
        assert!(
            out.contains("covered: 2025-01-01T00:00:00Z..2025-06-01T00:00:00Z  (2 entries)"),
            "expected one merged 2-entry window, got: {out}"
        );
        assert!(out.contains("gaps:    none"), "expected no gaps, got: {out}");
    }

    #[test]
    fn render_coverage_reports_internal_gap_between_non_adjacent_entries() {
        let rows = vec![
            row(
                "ETH/USD",
                "1Hour",
                "2025-01-01T00:00:00Z",
                "2025-02-01T00:00:00Z",
                "aaa",
            ),
            row(
                "ETH/USD",
                "1Hour",
                "2025-03-01T00:00:00Z",
                "2025-04-01T00:00:00Z",
                "bbb",
            ),
        ];
        let out = render_coverage(&rows);
        assert!(
            out.contains("GAP:     2025-02-01T00:00:00Z..2025-03-01T00:00:00Z"),
            "expected a gap between the two windows, got: {out}"
        );
    }

    #[test]
    fn render_coverage_skips_unparseable_rows() {
        let rows = vec![row("BTC/USD", "1Hour", "not-a-date", "also-bad", "aaa")];
        assert!(render_coverage(&rows).is_empty());
    }
}
