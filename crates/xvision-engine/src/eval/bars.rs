//! Bar-cache wrapper. Reads from the `bars_cache` SQLite table
//! (migration 005); on miss, calls the Alpaca fetcher and back-fills the
//! cache. Single-flight per `cache_key` so concurrent misses serialize
//! through one fetcher call.
//!
//! Public surface:
//! - [`BarCacheArgs`]: cache lookup key + window + upstream params.
//! - [`load_bars`]: the wrapper itself.
//!
//! Storage helpers (`read_bars_cache`/`write_bars_cache`) are private to
//! this module — the cache is an implementation detail of `load_bars`,
//! not a general-purpose store accessible via `ctx.db`. Callers must go
//! through `load_bars` so the singleflight + gzip-threshold invariants
//! are enforced.

use std::io::{Read, Write};

use chrono::{DateTime, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};

use xvision_data::alpaca::{BarGranularity, MarketBar};

use crate::api::{ApiContext, ApiError, ApiResult};

/// Switch to gzip compression for windows with more than this many bars.
/// Below the threshold the JSON-lines blob is stored uncompressed —
/// gzip's fixed overhead is not worth it for tiny windows (a handful of
/// hourly bars).
const GZIP_THRESHOLD_BARS: usize = 1000;

/// Arguments for [`load_bars`]. `cache_key` should be derived
/// deterministically from the other fields (asset + granularity + window
/// + source); callers compute it via a shared helper rather than passing
/// arbitrary strings.
#[derive(Debug, Clone)]
pub struct BarCacheArgs {
    pub cache_key: String,
    pub asset_pair: String,
    pub granularity: BarGranularity,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub data_source_tag: String,
}

/// Read bars for the window described by `args`, going through the
/// `bars_cache` table. On miss, calls the Alpaca fetcher on the context
/// and back-fills the cache before returning. Concurrent misses for the
/// same `cache_key` serialize on the per-key singleflight mutex.
pub async fn load_bars(ctx: &ApiContext, args: &BarCacheArgs) -> ApiResult<Vec<MarketBar>> {
    // 1. Single-flight per key — concurrent misses on the same key
    //    serialize on this mutex so only one upstream fetch fires.
    let lock = ctx.bars_singleflight_lock(&args.cache_key).await;
    let _guard = lock.lock().await;

    // 2. Cache lookup inside the guard. A previous caller that just
    //    finished a fetch has already written the blob, so a second
    //    concurrent caller hits the cache instead of re-fetching.
    if let Some(bars) = read_bars_cache(ctx, &args.cache_key).await? {
        return Ok(bars);
    }

    // 3. Fetch from upstream.
    let bars = ctx
        .alpaca_fetcher()
        .fetch_crypto_bars(&args.asset_pair, args.granularity, args.start, args.end)
        .await
        .map_err(|e| ApiError::Validation(format!("alpaca fetch: {e}")))?;

    // 4. Persist — uncompressed for small windows, gzip above threshold.
    let raw = serialise_bars(&bars);
    let (blob, compression) = if bars.len() > GZIP_THRESHOLD_BARS {
        (gzip(&raw), "gzip")
    } else {
        (raw, "none")
    };
    write_bars_cache(
        ctx,
        &args.cache_key,
        &args.asset_pair,
        args.granularity,
        args.start,
        args.end,
        &args.data_source_tag,
        bars.len(),
        &blob,
        compression,
    )
    .await?;

    Ok(bars)
}

async fn read_bars_cache(
    ctx: &ApiContext,
    cache_key: &str,
) -> ApiResult<Option<Vec<MarketBar>>> {
    let row: Option<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT bars_blob, compression FROM bars_cache WHERE cache_key = ?",
    )
    .bind(cache_key)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("read_bars_cache: {e}")))?;
    Ok(row.map(|(blob, compression)| deserialise_bars(&blob, &compression)))
}

#[allow(clippy::too_many_arguments)]
async fn write_bars_cache(
    ctx: &ApiContext,
    cache_key: &str,
    asset: &str,
    granularity: BarGranularity,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    data_source: &str,
    bar_count: usize,
    blob: &[u8],
    compression: &str,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO bars_cache \
         (cache_key, asset, granularity, window_start, window_end, \
          data_source, fetched_at, bar_count, bars_blob, compression) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(cache_key)
    .bind(asset)
    .bind(granularity.as_alpaca_str())
    .bind(window_start.to_rfc3339())
    .bind(window_end.to_rfc3339())
    .bind(data_source)
    .bind(Utc::now().to_rfc3339())
    .bind(bar_count as i64)
    .bind(blob)
    .bind(compression)
    .execute(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("write_bars_cache: {e}")))?;
    Ok(())
}

/// Serialise bars as newline-delimited JSON. ndjson keeps the
/// uncompressed-vs-gzip decision a flat blob-level switch (no schema
/// difference) and makes the cached payload easy to diff in tests.
fn serialise_bars(bars: &[MarketBar]) -> Vec<u8> {
    let mut out = Vec::new();
    for bar in bars {
        let line = serde_json::to_vec(&serde_json::json!({
            "t": bar.timestamp.to_rfc3339(),
            "o": bar.open,
            "h": bar.high,
            "l": bar.low,
            "c": bar.close,
            "v": bar.volume,
        }))
        .expect("MarketBar field set is JSON-safe");
        out.extend(line);
        out.push(b'\n');
    }
    out
}

fn gzip(input: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(input)
        .expect("gzip encoder cannot fail on Vec<u8>");
    enc.finish().expect("gzip finish cannot fail on Vec<u8>")
}

fn deserialise_bars(blob: &[u8], compression: &str) -> Vec<MarketBar> {
    let raw = if compression == "gzip" {
        let mut dec = GzDecoder::new(blob);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).expect("gzip blob decodes");
        out
    } else {
        blob.to_vec()
    };
    raw.split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_slice(l).expect("ndjson line");
            MarketBar {
                timestamp: chrono::DateTime::parse_from_rfc3339(v["t"].as_str().unwrap())
                    .unwrap()
                    .with_timezone(&Utc),
                open: v["o"].as_f64().unwrap(),
                high: v["h"].as_f64().unwrap(),
                low: v["l"].as_f64().unwrap(),
                close: v["c"].as_f64().unwrap(),
                volume: v["v"].as_f64().unwrap(),
            }
        })
        .collect()
}
