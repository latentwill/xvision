//! Non-custodial x402 client: loads the agent's OWN key locally (never sent to
//! the platform), signs EIP-3009 authorizations, and drives the dashboard's
//! public x402 endpoint. The handshake (browse/buy/import) is added in Task 2.2.

use alloy::signers::local::PrivateKeySigner;

/// Resolve the buyer signer from the local environment only (`XVN_AGENT_PK`,
/// 0x-hex). Errors if unset — non-custodial: the operator provides the key
/// locally; the platform never holds it.
pub fn load_agent_signer() -> Result<PrivateKeySigner, String> {
    let pk = std::env::var("XVN_AGENT_PK")
        .map_err(|_| "XVN_AGENT_PK not set (non-custodial: provide the buyer key locally)".to_string())?;
    pk.trim()
        .parse::<PrivateKeySigner>()
        .map_err(|e| format!("XVN_AGENT_PK invalid: {e}"))
}

/// Dashboard base URL the MCP client talks to. Default localhost dev server.
pub fn api_base() -> String {
    std::env::var("XVN_MARKETPLACE_API").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_signer_errors_without_env() {
        if std::env::var("XVN_AGENT_PK").is_err() {
            assert!(load_agent_signer().is_err());
        }
    }

    #[test]
    fn api_base_has_default() {
        std::env::remove_var("XVN_MARKETPLACE_API");
        assert!(api_base().starts_with("http"));
    }
}
