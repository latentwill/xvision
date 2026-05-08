pub mod registry;

use crate::bundle::StrategyBundle;

pub trait Template: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn plain_summary(&self) -> &'static str;
    /// Build a fresh draft bundle with default fields.
    /// `id` is the ULID assigned to the new draft.
    /// `name` is the human-readable name (e.g., "eth-mr-v1").
    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle;
}
