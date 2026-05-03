//! xianvec-data — OHLCV ingest + indicators + onchain signals.
//!
//! v1 keeps indicators as pure `Vec<f64>` functions; `polars` enters in the
//! ingest pipeline (TODO Phase 1.x) where lazy frames pull their weight.

pub mod indicators;

pub use indicators::*;
