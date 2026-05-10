//! Trader error type. Mirrors `xvision_intern::InternError` shape so callers
//! can pattern-match parse vs validation vs backend failures uniformly.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TraderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error: status {status} — {body}")]
    Api { status: u16, body: String },
    #[error("parse error after retry: {0}")]
    Parse(String),
    #[error("validation error: {0}")]
    Validation(garde::Report),
    #[error("missing api key in env: {0}")]
    MissingApiKey(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("trader produced no output")]
    Empty,
}
