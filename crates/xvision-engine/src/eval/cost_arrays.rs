//! Per-bar cost arrays — optional `fee_bps`, `slip_bps`, `spread_bps` columns
//! that may be present alongside OHLCV data in the same Parquet file.
//!
//! When a column is present the simulator consumes it per-bar; when absent the
//! scenario default is used. This is the architectural unlock for regime-aware,
//! volatility-aware, and time-of-day-aware cost modelling: populate the columns
//! offline and the simulator picks them up verbatim.
//!
//! V2E acceptance item 19 (research doc §4.2).

use chrono::{DateTime, Utc};

/// Per-bar cost overrides loaded from optional Parquet columns.
///
/// A `None` value means the column was absent for that bar (fallback to
/// scenario default). A `Some(f64)` value means the column was present
/// and provides a per-bar override in basis points.
#[derive(Debug, Clone, PartialEq)]
pub struct BarCostEntry {
    pub timestamp: DateTime<Utc>,
    /// Fee override in bps (taker fee). `None` → use scenario default.
    pub fee_bps: Option<f64>,
    /// Slippage override in bps. `None` → use scenario default.
    pub slip_bps: Option<f64>,
    /// Half-spread override in bps. `None` → use scenario default.
    pub spread_bps: Option<f64>,
}

/// Indexed table of per-bar cost overrides, keyed by bar timestamp.
///
/// Built once at run start from the optional Parquet columns. During the
/// decision loop the simulator calls `lookup` to resolve the effective
/// cost values for each bar.
#[derive(Debug, Clone, Default)]
pub struct BarCostTable {
    entries: Vec<BarCostEntry>,
}

impl BarCostTable {
    /// Construct from a pre-sorted list of entries.
    pub fn from_entries(entries: Vec<BarCostEntry>) -> Self {
        Self { entries }
    }

    /// Look up the cost entry for a given bar timestamp. Returns `None` if no
    /// entry exists for that timestamp (columns were absent or the bar is
    /// outside the cost-array range — caller falls back to scenario default).
    pub fn lookup(&self, ts: &DateTime<Utc>) -> Option<&BarCostEntry> {
        // Binary search by timestamp for O(log n) lookup.
        self.entries
            .binary_search_by_key(ts, |e| e.timestamp)
            .ok()
            .map(|idx| &self.entries[idx])
    }

    /// Return `true` when the table has no entries (all columns absent).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Load per-bar cost columns from an already-parsed Parquet batch.
///
/// Called by the fixture loader after reading the OHLCV columns. Returns
/// an empty `BarCostTable` when none of the three cost columns are present.
///
/// The function tolerates partial column presence: a file may have `slip_bps`
/// but not `fee_bps` — entries will have `fee_bps = None` for every bar.
pub fn load_bar_cost_table_from_batches(
    batches: &[arrow_array::RecordBatch],
) -> anyhow::Result<BarCostTable> {
    use arrow_array::{Array, Float64Array};

    let mut entries: Vec<BarCostEntry> = Vec::new();

    for batch in batches {
        let schema = batch.schema();

        // Check which cost columns exist.
        let fee_idx = schema.index_of("fee_bps").ok();
        let slip_idx = schema.index_of("slip_bps").ok();
        let spread_idx = schema.index_of("spread_bps").ok();

        // If none of the three are present, skip this batch.
        if fee_idx.is_none() && slip_idx.is_none() && spread_idx.is_none() {
            continue;
        }

        // Timestamp column is always present (OHLCV loader guarantees it).
        let ts_idx = schema.index_of("timestamp")?;

        let fee_col = fee_idx.and_then(|i| batch.column(i).as_any().downcast_ref::<Float64Array>());
        let slip_col = slip_idx.and_then(|i| batch.column(i).as_any().downcast_ref::<Float64Array>());
        let spread_col = spread_idx.and_then(|i| batch.column(i).as_any().downcast_ref::<Float64Array>());

        let ts_array = batch.column(ts_idx);

        for row in 0..batch.num_rows() {
            let ts_str = read_string_value(ts_array.as_ref(), row, "timestamp")?;
            let ts: DateTime<Utc> = ts_str.parse()?;

            let fee_bps = fee_col.and_then(|a| if a.is_null(row) { None } else { Some(a.value(row)) });
            let slip_bps = slip_col.and_then(|a| if a.is_null(row) { None } else { Some(a.value(row)) });
            let spread_bps = spread_col.and_then(|a| if a.is_null(row) { None } else { Some(a.value(row)) });

            entries.push(BarCostEntry {
                timestamp: ts,
                fee_bps,
                slip_bps,
                spread_bps,
            });
        }
    }

    Ok(BarCostTable::from_entries(entries))
}

/// Read a string value from an Arrow array column (supports Utf8, LargeUtf8,
/// Utf8View variants).
fn read_string_value<'a>(
    array: &'a dyn arrow_array::Array,
    row: usize,
    col: &str,
) -> anyhow::Result<&'a str> {
    use arrow_array::{LargeStringArray, StringArray, StringViewArray};

    if let Some(a) = array.as_any().downcast_ref::<StringArray>() {
        return Ok(a.value(row));
    }
    if let Some(a) = array.as_any().downcast_ref::<LargeStringArray>() {
        return Ok(a.value(row));
    }
    if let Some(a) = array.as_any().downcast_ref::<StringViewArray>() {
        return Ok(a.value(row));
    }
    anyhow::bail!("column {col} has unsupported arrow type {:?}", array.data_type())
}

/// Load per-bar cost arrays directly from a Parquet file path.
///
/// Opens the file, reads all record batches, extracts optional cost columns,
/// and returns a `BarCostTable`. The OHLCV columns are ignored here; they are
/// loaded separately by `load_ohlcv_fixture`.
pub fn load_bar_cost_table_from_path(path: &std::path::Path) -> anyhow::Result<BarCostTable> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("opening {} for cost arrays: {}", path.display(), e))?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.build()?;

    let batches: Vec<arrow_array::RecordBatch> = reader.collect::<Result<Vec<_>, _>>()?;
    load_bar_cost_table_from_batches(&batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn lookup_finds_entry_by_timestamp() {
        let entries = vec![
            BarCostEntry {
                timestamp: ts(2024, 1, 1),
                fee_bps: Some(10.0),
                slip_bps: Some(5.0),
                spread_bps: None,
            },
            BarCostEntry {
                timestamp: ts(2024, 1, 2),
                fee_bps: Some(12.0),
                slip_bps: None,
                spread_bps: Some(2.0),
            },
        ];
        let table = BarCostTable::from_entries(entries);
        let e = table.lookup(&ts(2024, 1, 1)).unwrap();
        assert_eq!(e.fee_bps, Some(10.0));
        assert_eq!(e.slip_bps, Some(5.0));
        assert_eq!(e.spread_bps, None);

        let e2 = table.lookup(&ts(2024, 1, 2)).unwrap();
        assert_eq!(e2.fee_bps, Some(12.0));
        assert_eq!(e2.spread_bps, Some(2.0));
    }

    #[test]
    fn lookup_returns_none_for_missing_timestamp() {
        let table = BarCostTable::default();
        assert!(table.lookup(&ts(2024, 1, 1)).is_none());
    }

    #[test]
    fn empty_table_is_empty() {
        let table = BarCostTable::default();
        assert!(table.is_empty());
    }
}
