//! Shared CLI helpers for parsing `AssetSymbol`.
//!
//! `AssetSymbol` doesn't impl `FromStr`; clap's `value_parser!` wants one. The
//! parser here matches `as_str()` (uppercase) and is used by every command
//! that takes `--asset`.

use xvision_core::AssetSymbol;

pub fn parse_asset(s: &str) -> Result<AssetSymbol, String> {
    match s.to_ascii_uppercase().as_str() {
        "BTC" => Ok(AssetSymbol::Btc),
        "ETH" => Ok(AssetSymbol::Eth),
        "SOL" => Ok(AssetSymbol::Sol),
        other => Err(format!("unknown asset '{other}'; want BTC|ETH|SOL")),
    }
}
