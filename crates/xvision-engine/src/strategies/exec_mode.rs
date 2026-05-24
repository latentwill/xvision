//! Strategy-level pipeline-shape config. Per the multi-asset design spec,
//! pipeline shape is Strategy data with a default, not a harness invariant —
//! so prompt-optimization can vary it without engine edits. v1 implements
//! only the default arms; other arms parse + validate but the executor
//! returns a clear not-implemented error.

use serde::{Deserialize, Serialize};

/// How the harness drives a multi-asset universe per bar.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// v1: run the pipeline once per active asset each bar.
    #[default]
    PerAsset,
    /// Reserved: one cycle sees all assets; trader reasons as a book.
    Portfolio,
    /// Open hatch for optimizer-authored modes.
    Custom(String),
}

/// How capital is shared across assets in a run.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapitalMode {
    /// v1: one capital pool, per-asset positions, shared equity.
    #[default]
    Pooled,
    /// Reserved: segregated per-asset sub-portfolios.
    PerAsset,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_mode_default_is_per_asset() {
        assert_eq!(ExecutionMode::default(), ExecutionMode::PerAsset);
    }

    #[test]
    fn capital_mode_default_is_pooled() {
        assert_eq!(CapitalMode::default(), CapitalMode::Pooled);
    }

    #[test]
    fn execution_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(ExecutionMode::PerAsset).unwrap(),
            serde_json::json!("per_asset")
        );
        assert_eq!(
            serde_json::to_value(ExecutionMode::Portfolio).unwrap(),
            serde_json::json!("portfolio")
        );
        // Custom round-trips in both directions.
        assert_eq!(
            serde_json::to_value(ExecutionMode::Custom("rotate".into())).unwrap(),
            serde_json::json!({"custom": "rotate"})
        );
        let c: ExecutionMode = serde_json::from_value(serde_json::json!({"custom": "rotate"})).unwrap();
        assert_eq!(c, ExecutionMode::Custom("rotate".into()));
    }

    #[test]
    fn capital_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(CapitalMode::Pooled).unwrap(),
            serde_json::json!("pooled")
        );
        assert_eq!(
            serde_json::to_value(CapitalMode::PerAsset).unwrap(),
            serde_json::json!("per_asset")
        );
        let back: CapitalMode = serde_json::from_value(serde_json::json!("per_asset")).unwrap();
        assert_eq!(back, CapitalMode::PerAsset);
    }
}
