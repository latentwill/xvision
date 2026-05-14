//! xvision-core — schemas, config, persistence.
//!
//! Post-CV-extraction (ADR 0011) the substrate types (steering tensor +
//! manifest) live in xvision-play. This crate is now trading-domain only:
//! market snapshots, briefings, decisions, risk verdicts, persistence.

pub mod config;
pub mod market;
pub mod risk;
pub mod slot;
pub mod store;
pub mod trading;

pub use market::{IndicatorPanel, MarketSnapshot, Ohlcv, OnchainPanel, SkillRef};

pub use risk::{Capital, RiskCaps};

pub use trading::{
    Action, AssetSymbol, Direction, EvidenceTag, InternBriefing, OpenPosition, PortfolioState,
    Regime, RiskDecision, TraderDecision, VetoReason,
};
