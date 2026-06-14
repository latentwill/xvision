//! xvision-core — schemas, config, persistence.
//!
//! Post-CV-extraction (ADR 0011) the substrate types (steering tensor +
//! manifest) live in xvision-play. This crate is now trading-domain only:
//! market snapshots, decisions, risk verdicts, persistence.

pub mod agent_profiles;
pub mod asset_registry;
pub mod config;
pub mod market;
pub mod providers;
pub mod risk;
pub mod slot;
pub mod store;
pub mod trading;

pub use asset_registry::{DataSource, RegistryEntry};

pub use market::{IndicatorPanel, MarketSnapshot, Ohlcv, OnchainPanel, SkillRef};

pub use risk::{Capital, RiskCaps};

pub use trading::{
    Action, AssetSymbol, Direction, OpenPosition, PortfolioState, Regime, RiskDecision, TraderDecision,
    VetoReason,
};
