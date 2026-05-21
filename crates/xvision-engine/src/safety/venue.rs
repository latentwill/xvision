//! Venue label — coarse classification of live vs. paper vs. testnet trading.
//!
//! `VenueLabel` appears on `Scenario` so the UI can badge every run row,
//! capsule, and detail surface (green/amber/red) and the gate can enforce the
//! confused-deputy rule: a Paper-labelled scenario must not be submitted to a
//! Live-configured broker.

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VenueLabel {
    /// Simulated / paper trading — no real money at risk.
    #[default]
    Paper,
    /// Testnet / devnet — on-chain but no real funds.
    Testnet,
    /// Live / mainnet — real money at risk.
    Live,
}

impl VenueLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            VenueLabel::Paper => "paper",
            VenueLabel::Testnet => "testnet",
            VenueLabel::Live => "live",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "paper" => Some(VenueLabel::Paper),
            "testnet" => Some(VenueLabel::Testnet),
            "live" => Some(VenueLabel::Live),
            _ => None,
        }
    }

    /// Returns `true` when this label represents a real-money venue.
    pub fn is_live(self) -> bool {
        matches!(self, VenueLabel::Live)
    }
}
