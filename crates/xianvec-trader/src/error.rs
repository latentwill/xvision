//! Trader error type. Mirrors `xianvec_intern::InternError` shape so callers
//! can pattern-match parse vs validation vs engine failures uniformly.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TraderError {
    #[error("inference engine error: {0}")]
    Engine(#[from] xianvec_inference::EngineError),
    #[error("parse error after retry: {0}")]
    Parse(String),
    #[error("validation error: {0}")]
    Validation(garde::Report),
    #[error("trader produced no output")]
    Empty,
}
