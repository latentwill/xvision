//! Provider-side metadata shared across the workspace.
//!
//! Two layers live here:
//!
//! - `model_metadata` — xvision's *editorial* per-model hints
//!   (reasoning class, recommended budgets). Hand-curated. Used as the
//!   ceiling-of-last-resort when neither the operator nor the provider's
//!   catalog has anything to say.
//!
//! - `catalog` — pure data types for the provider-supplied catalogs
//!   (what `/v1/models` returned, when, with what fields). The actual
//!   HTTP fetching, on-disk cache, and in-memory map live in
//!   `xvision-engine::providers` so this crate stays free of network
//!   dependencies.

pub mod catalog;
pub mod model_metadata;

pub use catalog::{Catalog, ModelEntry};
pub use model_metadata::{lookup_model, ModelClass, ModelMetadata};
