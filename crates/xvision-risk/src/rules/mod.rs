//! Individual risk rules, one struct per file.

pub mod asset_whitelist;
pub mod daily_loss_circuit;
pub mod max_open_positions;
pub mod max_position_size;
pub mod max_total_exposure;
pub mod min_notional;
pub mod stop_loss_present;
pub mod take_profit_rr;

pub use asset_whitelist::AssetWhitelist;
pub use daily_loss_circuit::DailyLossCircuit;
pub use max_open_positions::MaxOpenPositions;
pub use max_position_size::MaxPositionSize;
pub use max_total_exposure::MaxTotalExposure;
pub use min_notional::MinNotional;
pub use stop_loss_present::StopLossPresent;
pub use take_profit_rr::TakeProfitRR;
