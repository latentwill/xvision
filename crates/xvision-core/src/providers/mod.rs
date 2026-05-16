//! Provider-side metadata shared across the workspace. Today this is just
//! `model_metadata` (per-model token limits + reasoning class) but the
//! module exists so future provider-side helpers have a stable home in
//! `xvision-core::providers`.

pub mod model_metadata;

pub use model_metadata::{lookup_model, ModelClass, ModelMetadata};
