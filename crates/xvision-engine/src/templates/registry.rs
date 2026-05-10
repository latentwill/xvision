use std::sync::OnceLock;

use crate::templates::breakout::Breakout;
use crate::templates::custom::Custom;
use crate::templates::mean_reversion::MeanReversion;
use crate::templates::momentum::Momentum;
use crate::templates::news_trader::NewsTrader;
use crate::templates::range_trade::RangeTrade;
use crate::templates::scalping::Scalping;
use crate::templates::trend_follower::TrendFollower;
use crate::templates::Template;

static REGISTRY: OnceLock<Vec<Box<dyn Template>>> = OnceLock::new();

fn registry() -> &'static [Box<dyn Template>] {
    REGISTRY.get_or_init(|| {
        vec![
            Box::new(TrendFollower) as Box<dyn Template>,
            Box::new(Breakout),
            Box::new(MeanReversion),
            Box::new(Momentum),
            Box::new(RangeTrade),
            Box::new(Scalping),
            Box::new(NewsTrader),
            Box::new(Custom),
            // Baseline registered as a marketplace seed listing.
            crate::baselines::ma_crossover::ma_crossover_template(),
        ]
    })
}

pub fn get(name: &str) -> Option<&'static dyn Template> {
    registry().iter().find(|t| t.name() == name).map(|t| t.as_ref())
}

pub fn list_template_names() -> Vec<String> {
    registry().iter().map(|t| t.name().to_string()).collect()
}
