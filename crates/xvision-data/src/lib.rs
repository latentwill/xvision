//! xvision-data — OHLCV ingest + indicators + onchain signals.

pub mod alpaca;
pub mod alpaca_live;
pub mod alpaca_live_poll;
pub mod asset_whitelist;
pub mod elfa;
pub mod fixtures;
pub mod hl_bars;
pub mod indicators;
pub mod manifest;
pub mod nansen;
pub mod perp_feed;
pub mod validate;

pub use indicators::*;
pub use perp_feed::{apply_to_onchain, fetch_perp_snapshot, parse_perp_snapshot, PerpSnapshot};
