//! `ReviewAnnotation` — structured chart annotation produced by the review
//! agent and persisted alongside the review result.
//!
//! The Rust type mirrors the frontend `Annotation` type defined in
//! `frontend/web/src/components/chart/v2/types.ts` so the wire shape is
//! byte-equivalent. ts-rs derives the TypeScript bindings automatically;
//! the generated file lands in
//! `frontend/web/src/api/types.gen/ReviewAnnotation.ts` (and the enum
//! companions) via `cargo test --features ts-export`.
//!
//! Spec: `docs/superpowers/specs/2026-05-23-live-annotation-producer-and-review-autofire.md`
//! §3 (schema) and §7 R1 (milestone scope).

use serde::{Deserialize, Serialize};

/// Maximum number of `ReviewAnnotation` items the review LLM may return per
/// review if the operator has not configured `max_annotations_per_review` on
/// the eval run. Matches the spec (§6.5) default of 8.
pub const DEFAULT_MAX_ANNOTATIONS_PER_REVIEW: u32 = 8;

/// A single structured annotation produced by the review agent. Persisted as
/// a JSON array in the `eval_reviews.annotations` column (migration 037).
///
/// Field semantics mirror `Annotation` in
/// `frontend/web/src/components/chart/v2/types.ts`; any rename here must be
/// reflected there and vice versa.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewAnnotation {
    /// Zero-based index into the candle array for the run. The review agent
    /// picks the most relevant candle; R3's parser validates
    /// `idx ∈ [0, candle_count)`.
    pub idx: u32,
    /// Which edge of the candle the callout renders on.
    pub side: AnnotationSide,
    /// Broad category of the signal.
    #[serde(rename = "type")]
    pub kind: AnnotationKind,
    /// Headline label rendered on the chart callout. ≤ 60 chars, headline
    /// form (title-case, no trailing period).
    pub title: String,
    /// Longer explanation rendered in the insight log. 12–25 words, plain
    /// language. Empty string is valid for compact callout-only annotations.
    pub body: String,
    /// Confidence score in [0.0, 1.0]. The review agent emits this; R3's
    /// parser clamps out-of-range values.
    pub conf: f32,
    /// Suggested trading action implied by the annotation.
    pub action: AnnotationAction,
    /// When `true` the chart surface tints the callout red (danger/risk
    /// signal). Defaults to `false`.
    #[serde(default)]
    pub danger: bool,
    /// Unix timestamp in seconds of the annotated candle. Populated from
    /// the candle's own timestamp at parse time (R3); the LLM emits `idx`
    /// and the parser fills `ts_sec` from the bar data.
    pub ts_sec: i64,
}

/// Which edge of the candle callout renders on.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationSide {
    Top,
    Bottom,
}

/// Broad category of the annotation signal. Serialised UPPERCASE to match the
/// frontend `AnnotationKind` union type.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AnnotationKind {
    Pattern,
    Flow,
    Risk,
    Reversion,
    Structure,
}

/// Suggested trading action the annotation implies. Serialised UPPERCASE to
/// match the frontend `AnnotationAction` union type.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AnnotationAction {
    Watch,
    Long,
    Short,
    Caution,
}
