//! Shared CLI helpers for parsing `AssetSymbol`.
//!
//! The parser accepts the same forgiving forms as `AssetSymbol::from_str` and
//! is used by commands that need a clap-compatible `value_parser`.

use std::str::FromStr;

use xvision_core::AssetSymbol;

pub fn parse_asset(s: &str) -> Result<AssetSymbol, String> {
    AssetSymbol::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_asset_normalizes_forgiving_forms() {
        // Trims surrounding whitespace, upper-cases, and strips the `/USD`
        // quote suffix down to the base ticker before interning.
        assert_eq!(parse_asset("AAVE").unwrap(), AssetSymbol::Aave);
        assert_eq!(parse_asset(" link/usd ").unwrap(), AssetSymbol::Link);
        assert_eq!(parse_asset("usdc/usd").unwrap(), AssetSymbol::Usdc);
    }

    #[test]
    fn parse_asset_is_open_world_but_rejects_invalid_format() {
        // Open-world interning (W2 whitelist registry): any well-formed ticker
        // parses, including symbols outside the legacy 15-symbol set. There is
        // no Alpaca-whitelist rejection at parse time anymore.
        assert_eq!(parse_asset("XRP").unwrap().as_str(), "XRP");
        // Format validation still rejects non-[A-Z0-9_] characters.
        let err = parse_asset("XR!P").unwrap_err();
        assert!(err.contains("invalid characters"), "got: {err}");
    }
}
