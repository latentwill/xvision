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

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Args, Subcommand};

use xvision_core::AssetSymbol;
use xvision_data::alpaca::BarGranularity;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::bars::{self, compute_cache_key, BarCacheArgs};

use crate::exit::{CliError, CliResult};

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
    Ls,
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
    /// Bar granularity. v1 supports `1h` or `1d`.
    #[arg(long, default_value = "1h")]
    pub granularity: String,
}

pub async fn run(cmd: BarsCmd) -> CliResult<()> {
    let ctx = open_ctx(cmd.xvn_home.clone())
        .await
        .map_err(CliError::upstream)?;
    match cmd.op {
        BarsOp::Fetch(a) => run_fetch(&ctx, a).await,
        BarsOp::Ls => run_ls(&ctx).await,
        BarsOp::Rm { cache_key } => run_rm(&ctx, cache_key).await,
        BarsOp::Gc { older_than } => run_gc(&ctx, older_than).await,
    }
}

fn resolve_xvn_home(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    if let Ok(p) = std::env::var("XVN_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("HOME not set; pass --xvn-home")?;
    Ok(home.join(".xvn"))
}

async fn open_ctx(override_path: Option<PathBuf>) -> Result<ApiContext> {
    let xvn_home = resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))
}

async fn run_fetch(ctx: &ApiContext, a: FetchArgs) -> CliResult<()> {
    let asset = AssetSymbol::from_str(&a.asset)
        .map_err(|e| CliError::usage(anyhow::anyhow!("{e}")))?;
    let granularity = match a.granularity.as_str() {
        "1h" => BarGranularity::Hour1,
        "1d" => BarGranularity::Day1,
        other => {
            return Err(CliError::usage(anyhow::anyhow!(
                "granularity '{other}' not in v1 set {{1h,1d}}"
            )));
        }
    };
    let start = a
        .from
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --from date")))?
        .and_utc();
    let end = a
        .to
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| CliError::usage(anyhow::anyhow!("invalid --to date")))?
        .and_utc();
    if end <= start {
        return Err(CliError::usage(anyhow::anyhow!(
            "--to must be strictly after --from"
        )));
    }
    let asset_pair = asset.as_alpaca_pair();
    let data_source_tag = "alpaca-historical-v1";
    let cache_key = compute_cache_key(&asset_pair, granularity, start, end, data_source_tag);
    let args = BarCacheArgs {
        cache_key: cache_key.clone(),
        asset_pair,
        granularity,
        start,
        end,
        data_source_tag: data_source_tag.into(),
    };
    let out = bars::load_bars(ctx, &args)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("{e}")))?;
    println!("Fetched {} bars (cache_key={cache_key})", out.len());
    Ok(())
}

async fn run_ls(ctx: &ApiContext) -> CliResult<()> {
    let rows = list_bars_cache(ctx).await?;
    if rows.is_empty() {
        println!("(no cached bar windows)");
        return Ok(());
    }
    for r in rows {
        println!(
            "{}  {}  {}  {}..{}  {} bars",
            r.cache_key, r.asset, r.granularity, r.window_start, r.window_end, r.bar_count
        );
    }
    Ok(())
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
        Err(CliError::usage(anyhow::anyhow!(
            "only Nd supported (got '{s}')"
        )))
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
}

async fn list_bars_cache(ctx: &ApiContext) -> CliResult<Vec<BarsCacheRow>> {
    let rows: Vec<(String, String, String, String, String, i64)> = sqlx::query_as(
        "SELECT cache_key, asset, granularity, window_start, window_end, bar_count \
         FROM bars_cache ORDER BY fetched_at DESC",
    )
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| CliError::upstream(anyhow::anyhow!("list_bars_cache: {e}")))?;
    Ok(rows
        .into_iter()
        .map(
            |(cache_key, asset, granularity, window_start, window_end, bar_count)| BarsCacheRow {
                cache_key,
                asset,
                granularity,
                window_start,
                window_end,
                bar_count,
            },
        )
        .collect())
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
}
