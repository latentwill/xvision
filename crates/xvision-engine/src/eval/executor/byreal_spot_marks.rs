//! `ByrealSpotPriceFetcher` — a `LivePollFetcher` that turns the latest
//! `byreal-cli` token price into a single synthetic OHLCV bar (o=h=l=c=price).
//!
//! Solana spot has no OHLCV history from `byreal-cli`, so v1 forward-test marks
//! are degenerate single-price bars: each poll fetches the current price and
//! emits one bar. This is the `byreal_spot` analogue of the Hyperliquid
//! `production_hl_fetcher` used for the HL-native venues; it lives in the engine
//! (not `xvision-execution`) because it bridges `ByrealSpotApi` (execution) and
//! `LivePollFetcher` (data) — the integration layer that depends on both.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use xvision_core::config::SpotAssetConfig;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live_poll::{AlpacaPollError, LivePollFetcher};
use xvision_execution::{ByrealSpotApi, SubprocessByrealSpotApi};

/// Poll-only mark source for `byreal_spot`: latest token price → one bar.
pub struct ByrealSpotPriceFetcher<A = SubprocessByrealSpotApi> {
    api: A,
    assets: SpotAssetConfig,
}

impl<A: ByrealSpotApi> ByrealSpotPriceFetcher<A> {
    pub fn new(api: A, assets: SpotAssetConfig) -> Self {
        Self { api, assets }
    }
}

#[async_trait]
impl<A: ByrealSpotApi + 'static> LivePollFetcher for ByrealSpotPriceFetcher<A> {
    async fn fetch_window(
        &self,
        asset: &str,
        _granularity: BarGranularity,
        _start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        let entry = self
            .assets
            .resolve(asset)
            .ok_or_else(|| AlpacaPollError::Rejected(format!("byreal_spot: '{asset}' not curated")))?;
        let price = self
            .api
            .token_price(&entry.mint)
            .await
            .map_err(|e| AlpacaPollError::Rejected(format!("byreal_spot price: {e}")))?;
        Ok(vec![MarketBar {
            timestamp: end,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0.0,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::config::{SpotAssetEntry, SpotAssetKind};
    use xvision_execution::executor::ExecutorError;
    use xvision_execution::{ByrealSpotMode, SwapResult};

    struct MockApi {
        price: f64,
    }

    #[async_trait]
    impl ByrealSpotApi for MockApi {
        async fn swap(
            &self,
            _i: &str,
            _o: &str,
            _a: f64,
            _s: u32,
            _m: ByrealSpotMode,
        ) -> Result<SwapResult, ExecutorError> {
            unreachable!("fetcher never swaps")
        }
        async fn token_price(&self, _mint: &str) -> Result<f64, ExecutorError> {
            Ok(self.price)
        }
        async fn token_balance(&self, _mint: &str) -> Result<f64, ExecutorError> {
            Ok(0.0)
        }
    }

    fn curated() -> SpotAssetConfig {
        SpotAssetConfig {
            usdc_mint: "USDC1111111111111111111111111111111111111111".into(),
            assets: vec![SpotAssetEntry {
                symbol: "SOL".into(),
                mint: "So11111111111111111111111111111111111111112".into(),
                kind: SpotAssetKind::Spl,
                decimals: 9,
            }],
        }
    }

    #[tokio::test]
    async fn returns_one_synthetic_bar_at_current_price() {
        let fetcher = ByrealSpotPriceFetcher::new(MockApi { price: 150.0 }, curated());
        let now = Utc::now();
        let bars = fetcher
            .fetch_window("SOL", BarGranularity::Minute1, now, now)
            .await
            .unwrap();
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].close, 150.0);
        assert_eq!(bars[0].open, 150.0);
        assert_eq!(bars[0].high, 150.0);
        assert_eq!(bars[0].low, 150.0);
    }

    #[tokio::test]
    async fn unknown_symbol_is_rejected() {
        let fetcher = ByrealSpotPriceFetcher::new(MockApi { price: 1.0 }, curated());
        let now = Utc::now();
        assert!(fetcher
            .fetch_window("DOGE", BarGranularity::Minute1, now, now)
            .await
            .is_err());
    }
}
