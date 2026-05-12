pub mod breakout;
pub mod custom;
pub mod mean_reversion;
pub mod momentum;
pub mod news_trader;
pub mod range_trade;
pub mod registry;
pub mod scalping;
pub mod trend_follower;

use crate::strategies::Strategy;

pub trait Template: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn plain_summary(&self) -> &'static str;
    /// Build a fresh draft bundle with default fields.
    /// `id` is the ULID assigned to the new draft.
    /// `name` is the human-readable name (e.g., "eth-mr-v1").
    fn new_draft(&self, id: String, name: String, creator: String) -> Strategy;
}
