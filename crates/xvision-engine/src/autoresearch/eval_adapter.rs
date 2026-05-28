//! AR-2 Task 1 placeholder — BacktestExecutor adapter lands in Task 1.
use async_trait::async_trait;

use crate::eval::{MetricsSummary, Scenario};
use crate::strategies::Strategy;

#[async_trait]
pub trait PaperTestRunner: Send + Sync {
    async fn run(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
    ) -> anyhow::Result<MetricsSummary>;
}
