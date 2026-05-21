//! Test-fixture loader and generator for OHLCV parquet files.
//!
//! Fixtures live at `data/probes/<fixture>.parquet` under the workspace root
//! for local runs, or under `$XVN_DATA_DIR/probes` / `$XVN_PROBES_DIR` in
//! deployments.
//!
//! # Validator hook
//!
//! `load_ohlcv_fixture_with_hash` is the recommended entry point for scenario
//! runners. It reads the Parquet bytes, computes `bars_content_hash` via
//! `manifest::bars_content_hash`, parses the bars, and returns both together.
//! Callers can then run `validate::validate_ohlcv` over the bars before
//! feeding them into the backtest loop.

use std::path::PathBuf;
use std::sync::Arc;

use arrow_array::{Array, Float64Array, LargeStringArray, RecordBatch, StringArray, StringViewArray};
use arrow_schema::{DataType, Field, Schema};
use chrono::{DateTime, TimeZone, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use xvision_core::market::Ohlcv;

use crate::manifest::bars_content_hash;

/// Absolute path to the probe fixture directory.
///
/// Runtime deployments mount writable state under `XVN_DATA_DIR`, while local
/// tests still expect the workspace-root `data/probes` directory. Prefer the
/// deployed data path when configured, then fall back to the workspace copy.
fn probe_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XVN_PROBES_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XVN_DATA_DIR") {
        return PathBuf::from(path).join("probes");
    }

    // CARGO_MANIFEST_DIR = <workspace>/crates/xvision-data  (two levels deep)
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..") // crates/
        .join("..") // workspace root
        .join("data")
        .join("probes")
}

/// Absolute path to a named fixture parquet file.
pub fn fixture_path(fixture: &str) -> PathBuf {
    probe_dir().join(format!("{fixture}.parquet"))
}

/// Load a parquet OHLCV fixture. Returns the LAST `lookback_bars` bars in
/// chronological order. `asset` is currently informational — fixtures are
/// per-asset so the caller picks the right file.
pub fn load_ohlcv_fixture(fixture: &str, _asset: &str, lookback_bars: usize) -> anyhow::Result<Vec<Ohlcv>> {
    let (bars, _hash) = load_ohlcv_fixture_with_hash(fixture, _asset, lookback_bars)?;
    Ok(bars)
}

/// Load a parquet OHLCV fixture and compute the content hash of the raw
/// Parquet bytes.
///
/// Returns `(bars, bars_content_hash)` where `bars_content_hash` is the
/// sha256 hex digest of the Parquet file bytes. The hash is computed before
/// any trimming so it reflects the on-disk file, not the requested slice.
/// Persist `bars_content_hash` on the `Run` record (see migration 027).
///
/// This is the **recommended entry point** for scenario runners that need
/// to participate in the determinism receipt. After calling this function,
/// run `validate::validate_ohlcv` over the returned bars.
pub fn load_ohlcv_fixture_with_hash(
    fixture: &str,
    _asset: &str,
    lookback_bars: usize,
) -> anyhow::Result<(Vec<Ohlcv>, String)> {
    let path = fixture_path(fixture);
    let raw_bytes = std::fs::read(&path).map_err(|e| anyhow::anyhow!("reading {}: {}", path.display(), e))?;

    // Compute hash over the raw Parquet bytes before parsing.
    let content_hash = bars_content_hash(&raw_bytes);

    // Convert to bytes::Bytes which implements parquet's ChunkReader trait.
    let parquet_bytes = bytes::Bytes::from(raw_bytes);
    let reader = ParquetRecordBatchReaderBuilder::try_new(parquet_bytes)?.build()?;

    let mut bars = Vec::new();
    for batch in reader {
        let batch = batch?;
        let schema = batch.schema();
        let ts_idx = schema.index_of("timestamp")?;
        let open_idx = schema.index_of("open")?;
        let high_idx = schema.index_of("high")?;
        let low_idx = schema.index_of("low")?;
        let close_idx = schema.index_of("close")?;
        let volume_idx = schema.index_of("volume")?;

        for row in 0..batch.num_rows() {
            let ts: DateTime<Utc> = string_value(batch.column(ts_idx).as_ref(), row, "timestamp")?.parse()?;
            bars.push(Ohlcv {
                timestamp: ts,
                open: f64_value(batch.column(open_idx).as_ref(), row, "open")?,
                high: f64_value(batch.column(high_idx).as_ref(), row, "high")?,
                low: f64_value(batch.column(low_idx).as_ref(), row, "low")?,
                close: f64_value(batch.column(close_idx).as_ref(), row, "close")?,
                volume: f64_value(batch.column(volume_idx).as_ref(), row, "volume")?,
            });
        }
    }

    let start = bars.len().saturating_sub(lookback_bars);
    Ok((bars.split_off(start), content_hash))
}

fn string_value<'a>(array: &'a dyn Array, row: usize, column: &str) -> anyhow::Result<&'a str> {
    if array.is_null(row) {
        anyhow::bail!("null {column} at row {row}");
    }
    if let Some(values) = array.as_any().downcast_ref::<StringArray>() {
        return Ok(values.value(row));
    }
    if let Some(values) = array.as_any().downcast_ref::<LargeStringArray>() {
        return Ok(values.value(row));
    }
    if let Some(values) = array.as_any().downcast_ref::<StringViewArray>() {
        return Ok(values.value(row));
    }
    anyhow::bail!(
        "column {column} has unsupported parquet type {:?}",
        array.data_type()
    )
}

fn f64_value(array: &dyn Array, row: usize, column: &str) -> anyhow::Result<f64> {
    if array.is_null(row) {
        return Ok(0.0);
    }
    if let Some(values) = array.as_any().downcast_ref::<Float64Array>() {
        return Ok(values.value(row));
    }
    anyhow::bail!(
        "column {column} has unsupported parquet type {:?}",
        array.data_type()
    )
}

/// Generate a deterministic synthetic fixture (300 hourly bars, mean-reverting
/// walk around 42 000 USD). Idempotent: writes only if the file doesn't already
/// exist.
pub fn ensure_test_fixture(fixture: &str) -> anyhow::Result<PathBuf> {
    let path = fixture_path(fixture);
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(probe_dir())?;

    let n = 300usize;
    let mut ts: Vec<String> = Vec::with_capacity(n);
    let mut open_v = Vec::with_capacity(n);
    let mut high_v = Vec::with_capacity(n);
    let mut low_v = Vec::with_capacity(n);
    let mut close_v = Vec::with_capacity(n);
    let mut volume_v = Vec::with_capacity(n);

    let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let mut price = 42_000.0_f64;
    for i in 0..n {
        let t = start + chrono::Duration::hours(i as i64);
        let o = price;
        let h = price * 1.005;
        let l = price * 0.995;
        let drift = ((i % 7) as f64 - 3.0) * 0.001;
        let c = price * (1.0 + drift);
        let v = 100.0 + i as f64;
        ts.push(t.to_rfc3339());
        open_v.push(o);
        high_v.push(h);
        low_v.push(l);
        close_v.push(c);
        volume_v.push(v);
        price = c;
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("timestamp", DataType::Utf8, false),
        Field::new("open", DataType::Float64, false),
        Field::new("high", DataType::Float64, false),
        Field::new("low", DataType::Float64, false),
        Field::new("close", DataType::Float64, false),
        Field::new("volume", DataType::Float64, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(ts)),
            Arc::new(Float64Array::from(open_v)),
            Arc::new(Float64Array::from(high_v)),
            Arc::new(Float64Array::from(low_v)),
            Arc::new(Float64Array::from(close_v)),
            Arc::new(Float64Array::from(volume_v)),
        ],
    )?;
    let mut file = std::fs::File::create(&path)?;
    let mut writer = ArrowWriter::try_new(&mut file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_default_test_fixture_creates_file() {
        let path = ensure_test_fixture("test-fixture-btc-2024-01").unwrap();
        assert!(path.exists());
        // Verify it loads back.
        let bars = load_ohlcv_fixture("test-fixture-btc-2024-01", "BTC/USD", 50).unwrap();
        assert_eq!(bars.len(), 50);
    }

    /// bars_content_hash is stable across multiple loads of the same file.
    #[test]
    fn bars_content_hash_is_stable_across_loads() {
        ensure_test_fixture("test-fixture-btc-2024-01").unwrap();
        let (_, h1) = load_ohlcv_fixture_with_hash("test-fixture-btc-2024-01", "BTC/USD", 50).unwrap();
        let (_, h2) = load_ohlcv_fixture_with_hash("test-fixture-btc-2024-01", "BTC/USD", 50).unwrap();
        assert_eq!(
            h1, h2,
            "bars_content_hash must be identical across loads of the same file"
        );
        assert_eq!(h1.len(), 64, "sha256 hex must be 64 chars");
    }

    /// load_ohlcv_fixture and load_ohlcv_fixture_with_hash return the same bars.
    #[test]
    fn with_hash_variant_returns_same_bars() {
        ensure_test_fixture("test-fixture-btc-2024-01").unwrap();
        let bars_plain = load_ohlcv_fixture("test-fixture-btc-2024-01", "BTC/USD", 30).unwrap();
        let (bars_hashed, _) =
            load_ohlcv_fixture_with_hash("test-fixture-btc-2024-01", "BTC/USD", 30).unwrap();
        assert_eq!(bars_plain.len(), bars_hashed.len());
        for (a, b) in bars_plain.iter().zip(bars_hashed.iter()) {
            assert_eq!(a.timestamp, b.timestamp);
            assert_eq!(a.open, b.open);
        }
    }
}
