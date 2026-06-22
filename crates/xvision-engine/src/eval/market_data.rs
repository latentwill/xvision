use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;
use xvision_data::alpaca::BarGranularity;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MarketDataKey {
    pub asset: AssetSymbol,
    pub timeframe: String,
}

#[derive(Debug, Clone, Default)]
pub struct MarketDataContext {
    series: BTreeMap<MarketDataKey, Vec<Ohlcv>>,
}

impl MarketDataContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_series(
        &mut self,
        asset: AssetSymbol,
        timeframe: BarGranularity,
        bars: Vec<Ohlcv>,
    ) -> Option<Vec<Ohlcv>> {
        self.series.insert(
            MarketDataKey {
                asset,
                timeframe: timeframe.canonical(),
            },
            bars,
        )
    }

    pub fn series(&self, asset: AssetSymbol, timeframe: BarGranularity) -> Option<&[Ohlcv]> {
        self.series
            .get(&MarketDataKey {
                asset,
                timeframe: timeframe.canonical(),
            })
            .map(|bars| bars.as_slice())
    }

    pub fn supported_timeframes(&self, asset: AssetSymbol) -> Vec<String> {
        self.series
            .keys()
            .filter(|key| key.asset == asset)
            .map(|key| key.timeframe.clone())
            .collect()
    }

    pub fn last_closed_at(
        &self,
        asset: AssetSymbol,
        timeframe: BarGranularity,
        as_of: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        self.closed_bars_as_of(asset, timeframe, as_of, 1)
            .and_then(|bars| bars.last().map(|bar| bar.timestamp))
    }

    pub fn closed_bars_as_of(
        &self,
        asset: AssetSymbol,
        timeframe: BarGranularity,
        as_of: DateTime<Utc>,
        lookback: usize,
    ) -> Option<&[Ohlcv]> {
        if lookback == 0 {
            return Some(&[]);
        }
        let bars = self.series(asset, timeframe)?;
        let cutoff = bars.partition_point(|bar| bar.timestamp <= as_of);
        if cutoff == 0 {
            return None;
        }
        let start = cutoff.saturating_sub(lookback);
        Some(&bars[start..cutoff])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDateTime};

    fn ts(input: &str) -> DateTime<Utc> {
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%SZ").unwrap(),
            Utc,
        )
    }

    fn bar(ts: &str, close: f64) -> Ohlcv {
        Ohlcv {
            timestamp: self::ts(ts),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn closed_bars_are_cropped_as_of() {
        let asset: AssetSymbol = "BTC/USD".parse().unwrap();
        let mut ctx = MarketDataContext::new();
        ctx.insert_series(
            asset,
            BarGranularity::Hour4,
            vec![
                bar("2025-01-01T04:00:00Z", 1.0),
                bar("2025-01-01T08:00:00Z", 2.0),
                bar("2025-01-01T12:00:00Z", 3.0),
            ],
        );

        let ten = self::ts("2025-01-01T10:00:00Z");
        let bars = ctx
            .closed_bars_as_of(asset, BarGranularity::Hour4, ten, 2)
            .expect("bars as of 10:00");
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[1].timestamp, self::ts("2025-01-01T08:00:00Z"));

        let last = ctx.last_closed_at(asset, BarGranularity::Hour4, ten).unwrap();
        assert_eq!(last, self::ts("2025-01-01T08:00:00Z"));
    }
}
