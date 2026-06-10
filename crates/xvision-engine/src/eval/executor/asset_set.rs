//! Resolve the active asset set for a run from the strategy universe and an
//! optional per-run subset. v1 is pure/static — a future cross-asset selector
//! agent would be consulted here, but the resolver signature stays the same.

use anyhow::{anyhow, Result};
use std::str::FromStr;
use xvision_core::trading::AssetSymbol;

/// `universe` is `Strategy.manifest.asset_universe` (e.g. `["BTC/USD","ETH/USD"]`).
/// `subset` is an optional `--assets` narrowing; every entry must be in the universe.
pub fn active_assets(universe: &[String], subset: Option<&[AssetSymbol]>) -> Result<Vec<AssetSymbol>> {
    if universe.is_empty() {
        return Err(anyhow!("strategy asset_universe is empty"));
    }
    let parsed: Vec<AssetSymbol> = universe
        .iter()
        .map(|s| AssetSymbol::from_str(s).map_err(|e| anyhow!("{e}")))
        .collect::<Result<_>>()?;
    match subset {
        None => Ok(parsed),
        Some(sub) => {
            for a in sub {
                if !parsed.contains(a) {
                    return Err(anyhow!("asset {a} is not in the strategy universe"));
                }
            }
            Ok(parsed.into_iter().filter(|a| sub.contains(a)).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::trading::AssetSymbol;

    #[test]
    fn resolves_full_universe_when_no_subset() {
        let got = active_assets(&["BTC/USD".into(), "ETH/USD".into()], None).unwrap();
        assert_eq!(got, vec![AssetSymbol::Btc, AssetSymbol::Eth]);
    }

    #[test]
    fn subset_must_be_subset_of_universe() {
        let err = active_assets(&["BTC/USD".into()], Some(&[AssetSymbol::Eth])).unwrap_err();
        assert!(err.to_string().contains("not in the strategy universe"));
    }

    #[test]
    fn rejects_unparseable_universe_symbol() {
        // Format-invalid symbol (contains '!'): from_str rejects it regardless
        // of registry contents.
        let err = active_assets(&["FOO!BAR".into()], None).unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }
}
