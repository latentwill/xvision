//! `cycle_features.parquet` — one row per decision cycle, written as a
//! sidecar next to the other run artifacts under
//! `~/.xvn/runs/<run_id>/cycle_features.parquet`.
//!
//! Column semantics (V2E trace-surface foundation, 2026-05-21):
//!
//! | Column                  | Type     | Notes |
//! |-------------------------|----------|-------|
//! | `cycle_id`              | Utf8     | Decision cycle ULID |
//! | `decision_index`        | UInt32   | 0-based position in the run |
//! | `model_id`              | Utf8     | LLM model identifier |
//! | `prompt_template_hash`  | Utf8     | SHA-256 hex of the prompt template |
//! | `regime_tag`            | Utf8?    | Scenario regime label (nullable) |
//! | `position_units`        | Float64  | Net position at decision time |
//! | `equity`                | Float64  | Portfolio equity at decision time |
//! | `drawdown_pct`          | Float64  | Running drawdown percentage |
//! | `prior_decision_action` | Utf8?    | Previous decision's action (nullable) |
//! | `tokens_in`             | UInt32   | Input tokens consumed |
//! | `tokens_out`            | UInt32   | Output tokens generated |
//! | `inference_cost_quote`  | Float64? | USD inference cost (nullable; populated by `eval-net-of-inference-cost-metric`) |
//! | `latency_ms`            | UInt32   | LLM round-trip latency in ms |
//!
//! ## Usage
//!
//! 1. Create a `CycleFeaturesWriter` at run start.
//! 2. Call `push_row` for every decision cycle.
//! 3. Call `flush` at run finalize — writes the parquet file to disk.
//!    A no-op when there are no rows.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{ArrayRef, Float64Array, RecordBatch, StringArray, UInt32Array};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// One row of cycle-feature data.
#[derive(Debug, Clone, Default)]
pub struct CycleFeatureRow {
    /// Decision cycle ULID.
    pub cycle_id: String,
    /// 0-based position in the run.
    pub decision_index: u32,
    /// LLM model identifier (empty string when not available).
    pub model_id: String,
    /// SHA-256 hex of the prompt template (empty string when not available).
    pub prompt_template_hash: String,
    /// Scenario regime label (None when not classified).
    pub regime_tag: Option<String>,
    /// Net position at decision time (base-asset units).
    pub position_units: f64,
    /// Portfolio equity at decision time (USD).
    pub equity: f64,
    /// Running drawdown percentage at decision time.
    pub drawdown_pct: f64,
    /// The action from the previous decision cycle (None for the first cycle).
    pub prior_decision_action: Option<String>,
    /// Input tokens consumed for this decision.
    pub tokens_in: u32,
    /// Output tokens generated for this decision.
    pub tokens_out: u32,
    /// USD inference cost (None until `eval-net-of-inference-cost-metric`
    /// populates it; this track only reserves the column).
    pub inference_cost_quote: Option<f64>,
    /// LLM round-trip latency in milliseconds.
    pub latency_ms: u32,
}

/// Accumulates per-decision rows and writes them as a parquet sidecar on
/// finalize.
///
/// Thread-safety: not `Send`; intended for single-executor use within one
/// run. Wrap in `tokio::sync::Mutex` if concurrency is needed.
pub struct CycleFeaturesWriter {
    path: PathBuf,
    rows: Vec<CycleFeatureRow>,
}

impl CycleFeaturesWriter {
    /// Create a writer that will flush to `<runs_dir>/<run_id>/cycle_features.parquet`.
    ///
    /// The directory must exist before `flush` is called; the executor's
    /// run-dir creation happens before the writer is needed.
    pub fn new(run_dir: PathBuf) -> Self {
        let path = run_dir.join("cycle_features.parquet");
        Self {
            path,
            rows: Vec::new(),
        }
    }

    /// Append a row. Called once per decision cycle inside the executor loop.
    pub fn push_row(&mut self, row: CycleFeatureRow) {
        self.rows.push(row);
    }

    /// Write all accumulated rows to the parquet file and return the row count.
    ///
    /// Returns `Ok(0)` without touching disk when there are no rows (a no-op
    /// that is safe to call even on empty / aborted runs).
    pub fn flush(self) -> Result<usize> {
        let n = self.rows.len();
        if n == 0 {
            return Ok(0);
        }
        let schema = cycle_features_schema();
        let batch = rows_to_record_batch(&schema, &self.rows).context("build cycle_features record batch")?;
        write_parquet(&self.path, schema, batch).context("write cycle_features.parquet")?;
        Ok(n)
    }

    /// Number of rows accumulated so far.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when no rows have been accumulated.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn cycle_features_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("cycle_id", DataType::Utf8, false),
        Field::new("decision_index", DataType::UInt32, false),
        Field::new("model_id", DataType::Utf8, false),
        Field::new("prompt_template_hash", DataType::Utf8, false),
        Field::new("regime_tag", DataType::Utf8, true),
        Field::new("position_units", DataType::Float64, false),
        Field::new("equity", DataType::Float64, false),
        Field::new("drawdown_pct", DataType::Float64, false),
        Field::new("prior_decision_action", DataType::Utf8, true),
        Field::new("tokens_in", DataType::UInt32, false),
        Field::new("tokens_out", DataType::UInt32, false),
        Field::new("inference_cost_quote", DataType::Float64, true),
        Field::new("latency_ms", DataType::UInt32, false),
    ]))
}

fn rows_to_record_batch(schema: &SchemaRef, rows: &[CycleFeatureRow]) -> Result<RecordBatch> {
    let cycle_ids: ArrayRef = Arc::new(StringArray::from(
        rows.iter().map(|r| r.cycle_id.as_str()).collect::<Vec<_>>(),
    ));
    let decision_indices: ArrayRef = Arc::new(UInt32Array::from(
        rows.iter().map(|r| r.decision_index).collect::<Vec<_>>(),
    ));
    let model_ids: ArrayRef = Arc::new(StringArray::from(
        rows.iter().map(|r| r.model_id.as_str()).collect::<Vec<_>>(),
    ));
    let prompt_hashes: ArrayRef = Arc::new(StringArray::from(
        rows.iter()
            .map(|r| r.prompt_template_hash.as_str())
            .collect::<Vec<_>>(),
    ));
    let regime_tags: ArrayRef = Arc::new(StringArray::from(
        rows.iter().map(|r| r.regime_tag.as_deref()).collect::<Vec<_>>(),
    ));
    let position_units: ArrayRef = Arc::new(Float64Array::from(
        rows.iter().map(|r| r.position_units).collect::<Vec<_>>(),
    ));
    let equity: ArrayRef = Arc::new(Float64Array::from(
        rows.iter().map(|r| r.equity).collect::<Vec<_>>(),
    ));
    let drawdown_pct: ArrayRef = Arc::new(Float64Array::from(
        rows.iter().map(|r| r.drawdown_pct).collect::<Vec<_>>(),
    ));
    let prior_actions: ArrayRef = Arc::new(StringArray::from(
        rows.iter()
            .map(|r| r.prior_decision_action.as_deref())
            .collect::<Vec<_>>(),
    ));
    let tokens_in: ArrayRef = Arc::new(UInt32Array::from(
        rows.iter().map(|r| r.tokens_in).collect::<Vec<_>>(),
    ));
    let tokens_out: ArrayRef = Arc::new(UInt32Array::from(
        rows.iter().map(|r| r.tokens_out).collect::<Vec<_>>(),
    ));
    let inference_costs: ArrayRef = Arc::new(Float64Array::from(
        rows.iter().map(|r| r.inference_cost_quote).collect::<Vec<_>>(),
    ));
    let latency_ms: ArrayRef = Arc::new(UInt32Array::from(
        rows.iter().map(|r| r.latency_ms).collect::<Vec<_>>(),
    ));

    RecordBatch::try_new(
        schema.clone(),
        vec![
            cycle_ids,
            decision_indices,
            model_ids,
            prompt_hashes,
            regime_tags,
            position_units,
            equity,
            drawdown_pct,
            prior_actions,
            tokens_in,
            tokens_out,
            inference_costs,
            latency_ms,
        ],
    )
    .context("create cycle_features RecordBatch")
}

fn write_parquet(path: &PathBuf, schema: SchemaRef, batch: RecordBatch) -> Result<()> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("create cycle_features file at {}", path.display()))?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props)).context("create ArrowWriter")?;
    writer.write(&batch).context("write cycle_features batch")?;
    writer.close().context("close cycle_features writer")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use tempfile::TempDir;

    fn make_row(idx: u32, model: &str) -> CycleFeatureRow {
        CycleFeatureRow {
            cycle_id: format!("01CYCLE{idx:08}"),
            decision_index: idx,
            model_id: model.to_string(),
            prompt_template_hash: format!("hash{idx:04}"),
            regime_tag: Some("trend".to_string()),
            position_units: 0.5,
            equity: 10_000.0 + idx as f64 * 10.0,
            drawdown_pct: 1.5,
            prior_decision_action: if idx == 0 { None } else { Some("hold".to_string()) },
            tokens_in: 800 + idx * 10,
            tokens_out: 100 + idx * 5,
            inference_cost_quote: None,
            latency_ms: 300 + idx * 20,
        }
    }

    #[test]
    fn empty_flush_returns_zero_without_touching_disk() {
        let dir = TempDir::new().unwrap();
        let writer = CycleFeaturesWriter::new(dir.path().to_path_buf());
        let n = writer.flush().unwrap();
        assert_eq!(n, 0);
        assert!(!dir.path().join("cycle_features.parquet").exists());
    }

    #[test]
    fn flush_writes_correct_row_count() {
        let dir = TempDir::new().unwrap();
        let mut writer = CycleFeaturesWriter::new(dir.path().to_path_buf());

        for i in 0..5 {
            writer.push_row(make_row(i, "claude-opus-4-7"));
        }
        let n = writer.flush().unwrap();
        assert_eq!(n, 5);

        // Verify the parquet file has 5 rows by reading it back.
        let path = dir.path().join("cycle_features.parquet");
        assert!(path.exists(), "parquet file must exist after flush");

        let file = std::fs::File::open(&path).unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let total_rows: usize = reader.map(|batch| batch.unwrap().num_rows()).sum();
        assert_eq!(total_rows, 5, "parquet row count must match push count");
    }

    #[test]
    fn flush_writes_correct_column_values() {
        let dir = TempDir::new().unwrap();
        let mut writer = CycleFeaturesWriter::new(dir.path().to_path_buf());
        writer.push_row(make_row(0, "gpt-4o"));
        writer.flush().unwrap();

        let path = dir.path().join("cycle_features.parquet");
        let file = std::fs::File::open(&path).unwrap();
        let mut reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let batch = reader.next().unwrap().unwrap();

        // Verify model_id column.
        let model_col = batch
            .column_by_name("model_id")
            .expect("model_id column must exist");
        let model_arr = model_col
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("model_id must be StringArray");
        assert_eq!(model_arr.value(0), "gpt-4o");

        // Verify nullable regime_tag column.
        let regime_col = batch
            .column_by_name("regime_tag")
            .expect("regime_tag column must exist");
        let regime_arr = regime_col
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("regime_tag must be StringArray");
        assert_eq!(regime_arr.value(0), "trend");
        assert!(!regime_arr.is_null(0));

        // Verify inference_cost_quote is null (not yet populated).
        let cost_col = batch
            .column_by_name("inference_cost_quote")
            .expect("inference_cost_quote column must exist");
        let cost_arr = cost_col
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("inference_cost_quote must be Float64Array");
        assert!(
            cost_arr.is_null(0),
            "inference_cost_quote must be null when not set"
        );
    }

    #[test]
    fn nullable_regime_tag_and_prior_action_survive_none() {
        let dir = TempDir::new().unwrap();
        let mut writer = CycleFeaturesWriter::new(dir.path().to_path_buf());
        writer.push_row(CycleFeatureRow {
            cycle_id: "01NULLTEST".into(),
            decision_index: 0,
            regime_tag: None,
            prior_decision_action: None,
            ..Default::default()
        });
        writer.flush().unwrap();

        let path = dir.path().join("cycle_features.parquet");
        let file = std::fs::File::open(&path).unwrap();
        let mut reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let batch = reader.next().unwrap().unwrap();

        let regime_arr = batch
            .column_by_name("regime_tag")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(
            regime_arr.is_null(0),
            "None regime_tag must produce a null parquet cell"
        );

        let prior_arr = batch
            .column_by_name("prior_decision_action")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(
            prior_arr.is_null(0),
            "None prior_decision_action must produce a null parquet cell"
        );
    }
}
