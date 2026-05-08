//! Test-fixture loader and generator for OHLCV parquet files.
//!
//! Fixtures live at `data/probes/<fixture>.parquet` under the workspace root.
//! The workspace root is located at compile time via `CARGO_MANIFEST_DIR`
//! (this crate is at `<workspace>/crates/xianvec-data`), so we go two levels
//! up to reach the workspace root.

use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use polars::prelude::*;
use xianvec_core::market::Ohlcv;

/// Absolute path to the workspace-root `data/probes` directory.
/// Built from `CARGO_MANIFEST_DIR` at compile time so it is correct
/// regardless of the cwd at test-run time.
fn workspace_probe_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <workspace>/crates/xianvec-data  (two levels deep)
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..") // crates/
        .join("..") // workspace root
        .join("data")
        .join("probes")
}

/// Absolute path to a named fixture parquet file.
pub fn fixture_path(fixture: &str) -> PathBuf {
    workspace_probe_dir().join(format!("{fixture}.parquet"))
}

/// Load a parquet OHLCV fixture. Returns the LAST `lookback_bars` bars in
/// chronological order. `asset` is currently informational — fixtures are
/// per-asset so the caller picks the right file.
pub fn load_ohlcv_fixture(fixture: &str, _asset: &str, lookback_bars: usize) -> anyhow::Result<Vec<Ohlcv>> {
    let path = fixture_path(fixture);
    let file =
        std::fs::File::open(&path).map_err(|e| anyhow::anyhow!("opening {}: {}", path.display(), e))?;
    let df = ParquetReader::new(file).finish()?;

    let ts_col = df.column("timestamp")?.str()?;
    let open = df.column("open")?.f64()?;
    let high = df.column("high")?.f64()?;
    let low = df.column("low")?.f64()?;
    let close = df.column("close")?.f64()?;
    let volume = df.column("volume")?.f64()?;

    let n = df.height();
    let start = n.saturating_sub(lookback_bars);
    let mut bars = Vec::with_capacity(n - start);
    for i in start..n {
        let ts: DateTime<Utc> = ts_col
            .get(i)
            .ok_or_else(|| anyhow::anyhow!("null timestamp at row {i}"))?
            .parse()?;
        bars.push(Ohlcv {
            timestamp: ts,
            open: open.get(i).unwrap_or(0.0),
            high: high.get(i).unwrap_or(0.0),
            low: low.get(i).unwrap_or(0.0),
            close: close.get(i).unwrap_or(0.0),
            volume: volume.get(i).unwrap_or(0.0),
        });
    }
    Ok(bars)
}

/// Generate a deterministic synthetic fixture (300 hourly bars, mean-reverting
/// walk around 42 000 USD). Idempotent: writes only if the file doesn't already
/// exist.
pub fn ensure_test_fixture(fixture: &str) -> anyhow::Result<PathBuf> {
    let path = fixture_path(fixture);
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(workspace_probe_dir())?;

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

    let mut df = df![
        "timestamp" => ts,
        "open"      => open_v,
        "high"      => high_v,
        "low"       => low_v,
        "close"     => close_v,
        "volume"    => volume_v,
    ]?;

    let mut file = std::fs::File::create(&path)?;
    ParquetWriter::new(&mut file).finish(&mut df)?;
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
}
