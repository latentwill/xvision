//! Phase 4.3 sync FAISS vector loader. v1 is a stub — the Phase 0.3 spike
//! operates on raw `Vec<f32>` saved from MLX extraction (see
//! `tools/extract_vectors/`); the FAISS path lands when Phase 4.2 produces
//! the `.index` files.

use std::path::Path;

use thiserror::Error;
use xianvec_core::Manifest;

#[derive(Debug, Error)]
pub enum SubstrateError {
    #[error("not implemented in v1: {0}")]
    NotImplemented(String),
}

pub struct VectorBundle {
    pub manifest: Manifest,
    /// Flat tensor data — Phase 4.3 will type this against `candle::Tensor`.
    pub data: Vec<f32>,
}

pub fn load_vector(_path: &Path, _expected: &Manifest) -> Result<VectorBundle, SubstrateError> {
    Err(SubstrateError::NotImplemented(
        "FAISS loader lands in Phase 4.3 (see implementation-plan.md §4.3)".into(),
    ))
}
