//! xianvec-core — schemas, config, persistence.
//!
//! The module split (`substrate` / `trading`) previews the v2 lodestar-core /
//! xianvec-core boundary: substrate types are domain-agnostic and would lift
//! cleanly into a future `lodestar-core` crate; trading types are
//! xianvec-specific.

pub mod config;
pub mod market;
pub mod store;
pub mod substrate;
pub mod trading;

pub use market::{IndicatorPanel, MarketSnapshot, Ohlcv, OnchainPanel, SkillRef};

pub use substrate::{
    FinishReason, GenParams, Generation, InferenceError, LayerIndex, Manifest, TokenLogprob,
    VectorRef,
};
pub use trading::{
    Action, AssetSymbol, Direction, DispositionAxis, EvidenceTag, InternBriefing, Regime,
    RiskDecision, TraderDecision, VetoReason,
};
