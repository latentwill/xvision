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
    load_warmup_window_with_fetcher(ctx, asset, granularity, now, warmup_bars, None).await
}

/// Same as [`load_warmup_window`] but uses `alpaca_fetcher` (when `Some`)
/// instead of the context's default fetcher.  Callers with live API
/// credentials pass them here so the warmup fetch uses the same keys as
/// the live poll — the context default fetcher is uncredentialed.
pub async fn load_warmup_window_with_fetcher(
    ctx: &ApiContext,
    asset: &str,
    granularity: BarGranularity,
    now: DateTime<Utc>,
    warmup_bars: u32,
    alpaca_fetcher: Option<&xvision_data::alpaca::AlpacaBarsFetcher>,
) -> ApiResult<Vec<xvision_core::market::Ohlcv>> {
    if warmup_bars == 0 {
        return Ok(Vec::new());
    }
    let secs = (granularity.seconds() as i64) * (warmup_bars as i64);
    let start = now - chrono::Duration::seconds(secs);
    let end = now;

    let market_bars = if let Some(fetcher) = alpaca_fetcher {
        fetcher
            .fetch_crypto_bars(asset, granularity, start, end)
            .await
            .map_err(|e| ApiError::Validation(format!("alpaca warmup fetch ({asset}): {e}")))?
    } else {
        let cache_key = compute_cache_key(asset, granularity, start, end, LIVE_WARMUP_DATA_SOURCE_TAG);
        load_bars(
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
        .await?
    };

    let got = market_bars.len() as u32;
    if got == 0 {
        tracing::warn!(
            target: "xvision_engine::live_source",
            asset, granularity = %granularity, requested = warmup_bars,
            start = %start, end = %end,
            "live warmup: Alpaca returned 0 bars. \
             Agent needs ~{warmup_bars} live bars before indicators have history. \
             Check APCA_API_KEY_ID / APCA_API_SECRET_KEY.",
        );
    } else if got < warmup_bars / 2 {
        tracing::warn!(
            target: "xvision_engine::live_source",
            asset, granularity = %granularity, got, requested = warmup_bars,
            "live warmup: only {got}/{warmup_bars} bars loaded",
        );
    }

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

// ---------------------------------------------------------------------------
// U16: bar-cache coverage preflight
// ---------------------------------------------------------------------------

/// One contiguous covered segment of the requested window, expressed as the
/// `[start, end)` it covers plus the originating cache row(s). Adjacent /
/// overlapping cache rows are merged into a single segment (see
/// [`merge_segments`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageSegment {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// blake3 cache keys of the rows that compose this merged segment, in
    /// chronological order. Surfaced so a CLI error can name the cache entries.
    pub cache_keys: Vec<String>,
}

/// A gap in coverage — a `[start, end)` sub-window of the request not covered
/// by any cache row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageGap {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Result of [`check_bar_coverage`]: whether the local cache fully covers the
/// requested `[start, end)` window as a UNION of (possibly multiple, possibly
/// adjacent) cache segments, plus the covered segments and any gaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    pub fully_covered: bool,
    pub covered: Vec<CoverageSegment>,
    pub gaps: Vec<CoverageGap>,
}

/// A single cache row's window, as read from `bars_cache`. Public so callers
/// that already have rows in hand (tests, `xvn bars ls`) can compose coverage
/// without re-querying.
#[derive(Debug, Clone)]
pub struct CachedWindow {
    pub cache_key: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Merge a set of cached windows into the minimal set of contiguous covered
/// segments. Windows are treated as half-open `[start, end)`. Two windows are
/// contiguous when the next window's start is `<=` the running segment's end —
/// crucially, granularity-aligned ADJACENCY (e.g. `Apr-01T00:00 == Apr-01T00:00`
/// where one row ends exactly where the next begins) is treated as contiguous,
/// NOT as a gap. This is the core fix for U16: a request spanning two adjacent
/// cache entries is "fully covered" even though no single entry contains it.
///
/// Pure and deterministic — unit-tested without any DB.
pub fn merge_segments(mut windows: Vec<CachedWindow>) -> Vec<CoverageSegment> {
    windows.sort_by_key(|w| (w.start, w.end));
    let mut out: Vec<CoverageSegment> = Vec::new();
    for w in windows {
        if w.end <= w.start {
            // Degenerate/empty window — ignore.
            continue;
        }
        match out.last_mut() {
            // Contiguous or overlapping: extend the running segment. `<=`
            // makes exact adjacency (end == next.start) merge instead of gap.
            Some(seg) if w.start <= seg.end => {
                if w.end > seg.end {
                    seg.end = w.end;
                }
                seg.cache_keys.push(w.cache_key);
            }
            _ => out.push(CoverageSegment {
                start: w.start,
                end: w.end,
                cache_keys: vec![w.cache_key],
            }),
        }
    }
    out
}

/// Given merged coverage segments and a requested `[req_start, req_end)`
/// window, compute the [`CoverageReport`]: the covered sub-segments that
/// intersect the request and the gaps within the request not covered by any
/// segment. Pure — separated from the DB read so it is directly unit-testable.
pub fn coverage_for(
    segments: &[CoverageSegment],
    req_start: DateTime<Utc>,
    req_end: DateTime<Utc>,
) -> CoverageReport {
    let mut covered: Vec<CoverageSegment> = Vec::new();
    let mut gaps: Vec<CoverageGap> = Vec::new();
    if req_end <= req_start {
        return CoverageReport {
            fully_covered: true,
            covered,
            gaps,
        };
    }
    // Walk the request from cursor → req_end, consuming intersecting segments.
    let mut cursor = req_start;
    for seg in segments {
        if seg.end <= cursor {
            continue; // entirely before the cursor
        }
        if seg.start >= req_end {
            break; // entirely after the request (segments are sorted)
        }
        let seg_lo = seg.start.max(req_start);
        if seg_lo > cursor {
            // Uncovered gap before this segment.
            gaps.push(CoverageGap {
                start: cursor,
                end: seg_lo,
            });
        }
        let seg_hi = seg.end.min(req_end);
        covered.push(CoverageSegment {
            start: seg_lo,
            end: seg_hi,
            cache_keys: seg.cache_keys.clone(),
        });
        if seg_hi > cursor {
            cursor = seg_hi;
        }
        if cursor >= req_end {
            break;
        }
    }
    if cursor < req_end {
        gaps.push(CoverageGap {
            start: cursor,
            end: req_end,
        });
    }
    CoverageReport {
        fully_covered: gaps.is_empty(),
        covered,
        gaps,
    }
}

/// U16 preflight: report whether the local `bars_cache` fully covers the
/// requested `[start, end)` window for `asset_pair` at `granularity`, treating
/// adjacent cache entries as contiguous (a multi-segment window is "covered").
///
/// Reads every `bars_cache` row matching the asset + granularity + the given
/// `data_source_tag`, merges their windows, and computes the report against the
/// request. Designed to be called by BOTH `xvn bars ls` (to show union
/// coverage) and the optimizer/eval preflight (to fail fast with an actionable
/// error BEFORE the cycle lock is acquired and BEFORE any backtest spawns).
pub async fn check_bar_coverage(
    ctx: &ApiContext,
    asset_pair: &str,
    granularity: BarGranularity,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    data_source_tag: &str,
) -> ApiResult<CoverageReport> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT cache_key, window_start, window_end FROM bars_cache \
         WHERE asset = ? AND granularity = ? AND data_source = ?",
    )
    .bind(asset_pair)
    .bind(granularity.as_alpaca_str())
    .bind(data_source_tag)
    .fetch_all(&ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("check_bar_coverage: {e}")))?;

    let mut windows = Vec::with_capacity(rows.len());
    for (cache_key, ws, we) in rows {
        let w_start = chrono::DateTime::parse_from_rfc3339(&ws)
            .map_err(|e| ApiError::Internal(format!("coverage: bad window_start: {e}")))?
            .with_timezone(&Utc);
        let w_end = chrono::DateTime::parse_from_rfc3339(&we)
            .map_err(|e| ApiError::Internal(format!("coverage: bad window_end: {e}")))?
            .with_timezone(&Utc);
        windows.push(CachedWindow {
            cache_key,
            start: w_start,
            end: w_end,
        });
    }
    let segments = merge_segments(windows);
    Ok(coverage_for(&segments, start, end))
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

    // 3. Cache miss — check whether the requested window is covered by
    //    OTHER cache entries (same asset, granularity, data_source) that
    //    overlap or abut the request. Fetch only the uncovered gaps instead
    //    of re-downloading bars we already have on disk.
    let coverage = check_bar_coverage(
        ctx,
        &args.asset_pair,
        args.granularity,
        args.start,
        args.end,
        &args.data_source_tag,
    )
    .await
    .map_err(|e| ApiError::Internal(format!("coverage check: {e}")))?;

    let bars = if coverage.fully_covered {
        // All bars already cached — merge from overlapping cache entries.
        load_and_merge_cached_bars(ctx, &coverage.covered, args.start, args.end).await?
    } else if !coverage.covered.is_empty() {
        // Partial coverage — read cached bars, fetch only gaps from Alpaca,
        // merge everything into one contiguous window.
        load_cached_and_fetch_gaps(ctx, args, &coverage.covered, &coverage.gaps).await?
    } else {
        // No coverage at all — fetch the full window from Alpaca (original path).
        ctx.alpaca_fetcher()
            .fetch_crypto_bars(&args.asset_pair, args.granularity, args.start, args.end)
            .await
            .map_err(|e| ApiError::Validation(format!("alpaca fetch: {e}")))?
    };

    // 4. Candle integrity pre-pass: hard-fail on structural corruption,
    //    warn on gaps.
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

    // 5. Persist — uncompressed for small windows, gzip above threshold.
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

/// Load bars from overlapping cache entries and slice to the requested window.
/// Used when `check_bar_coverage` reports full coverage — no Alpaca fetch needed.
async fn load_and_merge_cached_bars(
    ctx: &ApiContext,
    covered: &[CoverageSegment],
    req_start: DateTime<Utc>,
    req_end: DateTime<Utc>,
) -> ApiResult<Vec<MarketBar>> {
    let mut all_bars: Vec<MarketBar> = Vec::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for seg in covered {
        for ck in &seg.cache_keys {
            if !seen_keys.insert(ck.clone()) {
                continue; // same cache entry covers multiple gaps — read once
            }
            if let Some(bars) = read_bars_cache(ctx, ck).await? {
                all_bars.extend(bars);
            }
        }
    }

    // Sort by timestamp and deduplicate (overlapping entries may have
    // duplicate bars at the boundary).
    all_bars.sort_by_key(|b| b.timestamp);
    all_bars.dedup_by_key(|b| b.timestamp);

    // Slice to the requested window.
    let start_idx = all_bars
        .binary_search_by_key(&req_start, |b| b.timestamp)
        .unwrap_or_else(|i| i);
    let end_idx = all_bars
        .binary_search_by_key(&req_end, |b| b.timestamp)
        .unwrap_or_else(|i| i);

    Ok(all_bars[start_idx..end_idx].to_vec())
}

/// Load cached bars from covered segments, fetch only the uncovered gaps from
/// Alpaca, merge everything, and sort/dedup. Used when coverage is partial.
async fn load_cached_and_fetch_gaps(
    ctx: &ApiContext,
    args: &BarCacheArgs,
    covered: &[CoverageSegment],
    gaps: &[CoverageGap],
) -> ApiResult<Vec<MarketBar>> {
    // 1. Load cached bars from covered segments.
    let mut all_bars: Vec<MarketBar> = Vec::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for seg in covered {
        for ck in &seg.cache_keys {
            if !seen_keys.insert(ck.clone()) {
                continue;
            }
            if let Some(bars) = read_bars_cache(ctx, ck).await? {
                all_bars.extend(bars);
            }
        }
    }

    // 2. Fetch each gap from Alpaca.
    for gap in gaps {
        let gap_bars = ctx
            .alpaca_fetcher()
            .fetch_crypto_bars(&args.asset_pair, args.granularity, gap.start, gap.end)
            .await
            .map_err(|e| ApiError::Validation(format!("alpaca fetch (gap): {e}")))?;
        all_bars.extend(gap_bars);
    }

    // 3. Sort + dedup + slice to the requested window.
    all_bars.sort_by_key(|b| b.timestamp);
    all_bars.dedup_by_key(|b| b.timestamp);

    let start_idx = all_bars
        .binary_search_by_key(&args.start, |b| b.timestamp)
        .unwrap_or_else(|i| i);
    let end_idx = all_bars
        .binary_search_by_key(&args.end, |b| b.timestamp)
        .unwrap_or_else(|i| i);

    Ok(all_bars[start_idx..end_idx].to_vec())
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

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn win(key: &str, start: &str, end: &str) -> CachedWindow {
        CachedWindow {
            cache_key: key.into(),
            start: ts(start),
            end: ts(end),
        }
    }

    /// Two adjacent cache rows (Apr-01-end == Apr-01-start) merge into ONE
    /// segment — exact adjacency is NOT a gap. This is the U16 core bug.
    #[test]
    fn test_adjacent_windows_merge_no_gap() {
        let windows = vec![
            win("a", "2025-01-01T00:00:00Z", "2025-04-01T00:00:00Z"),
            win("b", "2025-04-01T00:00:00Z", "2025-06-01T00:00:00Z"),
        ];
        let segs = merge_segments(windows);
        assert_eq!(segs.len(), 1, "adjacent rows must merge into one segment");
        assert_eq!(segs[0].start, ts("2025-01-01T00:00:00Z"));
        assert_eq!(segs[0].end, ts("2025-06-01T00:00:00Z"));
        assert_eq!(segs[0].cache_keys, vec!["a".to_string(), "b".to_string()]);

        // A request straddling the boundary is fully covered.
        let report = coverage_for(&segs, ts("2025-03-15T00:00:00Z"), ts("2025-05-01T00:00:00Z"));
        assert!(report.fully_covered, "straddling request must be covered");
        assert!(report.gaps.is_empty());
    }

    /// Overlapping windows also merge.
    #[test]
    fn test_overlapping_windows_merge() {
        let windows = vec![
            win("a", "2025-01-01T00:00:00Z", "2025-03-15T00:00:00Z"),
            win("b", "2025-03-01T00:00:00Z", "2025-06-01T00:00:00Z"),
        ];
        let segs = merge_segments(windows);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].end, ts("2025-06-01T00:00:00Z"));
    }

    /// A real gap between two non-adjacent windows is reported.
    #[test]
    fn test_real_gap_reported() {
        let windows = vec![
            win("a", "2025-01-01T00:00:00Z", "2025-02-01T00:00:00Z"),
            win("b", "2025-04-01T00:00:00Z", "2025-06-01T00:00:00Z"),
        ];
        let segs = merge_segments(windows);
        assert_eq!(segs.len(), 2, "non-adjacent rows stay separate");
        let report = coverage_for(&segs, ts("2025-01-15T00:00:00Z"), ts("2025-05-01T00:00:00Z"));
        assert!(!report.fully_covered);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].start, ts("2025-02-01T00:00:00Z"));
        assert_eq!(report.gaps[0].end, ts("2025-04-01T00:00:00Z"));
    }

    /// Request entirely outside any cache → one gap == the whole request.
    #[test]
    fn test_no_coverage_at_all() {
        let segs: Vec<CoverageSegment> = Vec::new();
        let report = coverage_for(&segs, ts("2025-01-01T00:00:00Z"), ts("2025-02-01T00:00:00Z"));
        assert!(!report.fully_covered);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].start, ts("2025-01-01T00:00:00Z"));
        assert_eq!(report.gaps[0].end, ts("2025-02-01T00:00:00Z"));
    }

    /// Leading and trailing gaps around a single mid-window segment.
    #[test]
    fn test_leading_and_trailing_gaps() {
        let segs = merge_segments(vec![win("a", "2025-02-01T00:00:00Z", "2025-03-01T00:00:00Z")]);
        let report = coverage_for(&segs, ts("2025-01-01T00:00:00Z"), ts("2025-04-01T00:00:00Z"));
        assert!(!report.fully_covered);
        assert_eq!(report.gaps.len(), 2, "leading + trailing gaps");
        assert_eq!(report.gaps[0].start, ts("2025-01-01T00:00:00Z"));
        assert_eq!(report.gaps[0].end, ts("2025-02-01T00:00:00Z"));
        assert_eq!(report.gaps[1].start, ts("2025-03-01T00:00:00Z"));
        assert_eq!(report.gaps[1].end, ts("2025-04-01T00:00:00Z"));
        assert_eq!(report.covered.len(), 1);
    }

    /// Empty request window is trivially covered.
    #[test]
    fn test_empty_request_covered() {
        let segs: Vec<CoverageSegment> = Vec::new();
        let report = coverage_for(&segs, ts("2025-01-01T00:00:00Z"), ts("2025-01-01T00:00:00Z"));
        assert!(report.fully_covered);
        assert!(report.gaps.is_empty());
    }
}
