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
    fn parse_asset_accepts_full_alpaca_crypto_whitelist() {
        assert_eq!(parse_asset("AAVE").unwrap(), AssetSymbol::Aave);
        assert_eq!(parse_asset(" link/usd ").unwrap(), AssetSymbol::Link);
        assert_eq!(parse_asset("USDCUSD").unwrap(), AssetSymbol::Usdc);
    }

    #[test]
    fn parse_asset_rejects_unsupported_symbols_with_whitelist_message() {
        let err = parse_asset("XRP").unwrap_err();
        assert!(err.contains("not in the Alpaca crypto whitelist"));
    }
}
