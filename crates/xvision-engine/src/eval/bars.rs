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

use xvision_core::market::Ohlcv;
use xvision_data::alpaca::{BarGranularity, MarketBar};

use crate::api::{ApiContext, ApiError, ApiResult};

/// Switch to gzip compression for windows with more than this many bars.
/// Below the threshold the JSON-lines blob is stored uncompressed —
/// gzip's fixed overhead is not worth it for tiny windows (a handful of
/// hourly bars).
const GZIP_THRESHOLD_BARS: usize = 1000;

/// Arguments for [`load_bars`]. `cache_key` should be derived
/// deterministically from the other fields (asset, granularity, window,
/// and source); callers compute it via a shared helper rather than passing
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

/// Deterministic cache key for a `(asset_pair, granularity, window,
/// data_source_tag)` tuple. blake3 hex (64 chars).
pub fn compute_cache_key(
    asset_pair: &str,
    granularity: BarGranularity,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    data_source_tag: &str,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(asset_pair.as_bytes());
    h.update(granularity.as_alpaca_str().as_bytes());
    h.update(start.to_rfc3339().as_bytes());
    h.update(end.to_rfc3339().as_bytes());
    h.update(data_source_tag.as_bytes());
    h.finalize().to_hex().to_string()
}

/// Cache key for a scenario's window+granularity, independent of asset.
/// Per-asset bar loads compute their own key via [`compute_cache_key`].
///
/// Scenarios are asset-free (the asset a run trades comes from the
/// strategy's `asset_universe`), so the scenario-level cache key omits the
/// asset component. Mirrors [`compute_cache_key`]'s blake3 hashing minus
/// the `asset_pair` input.
pub fn compute_scenario_cache_key(
    granularity: BarGranularity,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    source: &str,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(granularity.as_alpaca_str().as_bytes());
    h.update(start.to_rfc3339().as_bytes());
    h.update(end.to_rfc3339().as_bytes());
    h.update(source.as_bytes());
    h.finalize().to_hex().to_string()
}

/// Tag distinguishing warmup-window cache rows from the main scenario
/// window. Used as the `data_source_tag` argument to [`compute_cache_key`]
/// so warmup and main bars never collide on a shared key.
pub const WARMUP_DATA_SOURCE_TAG: &str = "alpaca-historical-v1-warmup";

/// Compute the `[start, end)` window for fetching `count` bars
/// immediately before `scenario_start`. End matches `scenario_start`
/// exactly so the warmup window is contiguous with the decision window.
pub fn warmup_window_for(
    granularity: BarGranularity,
    scenario_start: DateTime<Utc>,
    count: u32,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let secs = (granularity.seconds() as i64) * (count as i64);
    let warmup_start = scenario_start - chrono::Duration::seconds(secs);
    (warmup_start, scenario_start)
}

/// Load `count` bars immediately before `scenario_start` via the same
/// singleflight cache wrapper as [`load_bars`]. Returns an empty Vec
/// when `count == 0`. Cache key is derived from the warmup window plus
/// [`WARMUP_DATA_SOURCE_TAG`] so warmup rows live alongside (but never
/// collide with) the main scenario-window rows.
pub async fn load_warmup_bars(
    ctx: &ApiContext,
    asset_pair: &str,
    granularity: BarGranularity,
    scenario_start: DateTime<Utc>,
    count: u32,
) -> ApiResult<Vec<MarketBar>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let (start, end) = warmup_window_for(granularity, scenario_start, count);
    let cache_key = compute_cache_key(asset_pair, granularity, start, end, WARMUP_DATA_SOURCE_TAG);
    load_bars(
        ctx,
        &BarCacheArgs {
            cache_key,
            asset_pair: asset_pair.to_string(),
            granularity,
            start,
            end,
            data_source_tag: WARMUP_DATA_SOURCE_TAG.into(),
        },
    )
    .await
}

/// Tag for live-warmup cache rows. Used by [`load_warmup_window`] so
/// the now-anchored warmup window (loaded by `LiveStream::new_with_warmup`)
/// goes through the same cache table as scenario warmup but lives
/// under a distinct `data_source` column. Mirrors
/// [`WARMUP_DATA_SOURCE_TAG`].
pub const LIVE_WARMUP_DATA_SOURCE_TAG: &str = "alpaca-historical-v1-live-warmup";

/// Synchronously load the most recent `warmup_bars` bars for `asset`
/// at `granularity`, ending at `now`. The returned [`Ohlcv`] vector is
/// ordered oldest-first and ready to be drained by the executor's
/// per-bar loop before live bars start arriving.
///
/// Goes through the same cache + singleflight path as backtest
/// scenarios (see [`load_bars`] and [`load_warmup_bars`]) so the
/// `LiveStream` warmup never duplicates a fetch for a window the
/// scenario layer already cached. Tagged with
/// [`LIVE_WARMUP_DATA_SOURCE_TAG`] in the cache so the now-anchored
/// rows don't collide with scenario-anchored warmup rows.
///
/// Added by the Alpaca-Live executor refactor (sub-track 3); the
/// `LiveStream::new_with_warmup` constructor is the primary caller.
pub async fn load_warmup_window(
    ctx: &ApiContext,
    asset: &str,
    granularity: BarGranularity,
    now: DateTime<Utc>,
    warmup_bars: u32,
) -> ApiResult<Vec<xvision_core::market::Ohlcv>> {
    if warmup_bars == 0 {
        return Ok(Vec::new());
    }
    let secs = (granularity.seconds() as i64) * (warmup_bars as i64);
    let start = now - chrono::Duration::seconds(secs);
    let end = now;
    let cache_key = compute_cache_key(asset, granularity, start, end, LIVE_WARMUP_DATA_SOURCE_TAG);
    let market_bars = load_bars(
        ctx,
        &BarCacheArgs {
            cache_key,
            asset_pair: asset.to_string(),
            granularity,
            start,
            end,
            data_source_tag: LIVE_WARMUP_DATA_SOURCE_TAG.into(),
        },
    )
    .await?;
    Ok(market_bars.into_iter().map(market_bar_to_ohlcv).collect())
}

fn market_bar_to_ohlcv(b: MarketBar) -> xvision_core::market::Ohlcv {
    xvision_core::market::Ohlcv {
        timestamp: b.timestamp,
        open: b.open,
        high: b.high,
        low: b.low,
        close: b.close,
        volume: b.volume,
    }
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

    // 3a. Candle integrity pre-pass: hard-fail on structural corruption,
    //     warn on gaps. Runs on cache-miss so every uncached window is
    //     validated before being committed to the bar cache.
    {
        let ohlcv_check: Vec<Ohlcv> = bars.iter().map(|b| market_bar_to_ohlcv(b.clone())).collect();
        let gap_findings = crate::eval::candle_integrity::validate_bar_series(
            &ohlcv_check,
            Some(args.granularity.seconds()),
        )
        .map_err(|e| ApiError::Validation(format!("candle integrity: {e}")))?;
        for gap in &gap_findings {
            tracing::warn!(
                asset = %args.asset_pair,
                gap_start = %gap.gap_start_ts,
                gap_end = %gap.gap_end_ts,
                missing = gap.expected_bars,
                "bar series gap detected",
            );
        }
    }

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

async fn read_bars_cache(ctx: &ApiContext, cache_key: &str) -> ApiResult<Option<Vec<MarketBar>>> {
    let row: Option<(Vec<u8>, String)> =
        sqlx::query_as("SELECT bars_blob, compression FROM bars_cache WHERE cache_key = ?")
            .bind(cache_key)
            .fetch_optional(&ctx.db)
            .await
            .map_err(|e| ApiError::Internal(format!("read_bars_cache: {e}")))?;
    let Some((blob, compression)) = row else {
        return Ok(None);
    };
    match deserialise_bars(&blob, &compression) {
        Ok(bars) => Ok(Some(bars)),
        Err(e) => {
            tracing::warn!(
                cache_key = %cache_key,
                error = %e,
                "evicting corrupted bars_cache row"
            );
            // Best-effort eviction; if the DELETE fails, the next call
            // will hit the same row and try again.
            let _ = sqlx::query("DELETE FROM bars_cache WHERE cache_key = ?")
                .bind(cache_key)
                .execute(&ctx.db)
                .await;
            Ok(None)
        }
    }
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
    enc.write_all(input).expect("gzip encoder cannot fail on Vec<u8>");
    enc.finish().expect("gzip finish cannot fail on Vec<u8>")
}

fn deserialise_bars(blob: &[u8], compression: &str) -> Result<Vec<MarketBar>, ApiError> {
    let raw = if compression == "gzip" {
        let mut dec = GzDecoder::new(blob);
        let mut out = Vec::new();
        dec.read_to_end(&mut out)
            .map_err(|e| ApiError::Internal(format!("deserialise: gzip decode: {e}")))?;
        out
    } else {
        blob.to_vec()
    };
    let mut bars = Vec::new();
    for line in raw.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let v: serde_json::Value = serde_json::from_slice(line)
            .map_err(|e| ApiError::Internal(format!("deserialise: ndjson parse: {e}")))?;
        let ts_str = v
            .get("t")
            .and_then(|x| x.as_str())
            .ok_or_else(|| ApiError::Internal("deserialise: missing 't' field".into()))?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(ts_str)
            .map_err(|e| ApiError::Internal(format!("deserialise: bad timestamp: {e}")))?
            .with_timezone(&Utc);
        let open = v
            .get("o")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| ApiError::Internal("deserialise: missing 'o'".into()))?;
        let high = v
            .get("h")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| ApiError::Internal("deserialise: missing 'h'".into()))?;
        let low = v
            .get("l")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| ApiError::Internal("deserialise: missing 'l'".into()))?;
        let close = v
            .get("c")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| ApiError::Internal("deserialise: missing 'c'".into()))?;
        let volume = v
            .get("v")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| ApiError::Internal("deserialise: missing 'v'".into()))?;
        bars.push(MarketBar {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        });
    }
    Ok(bars)
}
