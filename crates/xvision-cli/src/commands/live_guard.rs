//! Real-money acknowledgement guard for manual CLI tools (`fire-trade`,
//! `close-position`).
//!
//! Provides [`require_real_money_ack`] — a pure, unit-testable helper that
//! refuses to proceed when the venue is Byreal on mainnet and the operator has
//! not passed `--i-understand-real-money`.
//!
//! The caller is responsible for reading `BYREAL_NETWORK` from the environment
//! and passing it in as `byreal_network`. This keeps the helper free of I/O
//! so it can be tested without env manipulation.
//!
//! # FAST-FOLLOW
//! This guard is a lightweight Phase 4 checkpoint (explicit ack flag only).
//! The full Phase 5 gate routes through BrokerSurface / SafetyManager and
//! requires new CLI→DB plumbing to persist the ack before submitting. Wire
//! that up when the SafetyManager pause-gate lands.

use anyhow::{bail, Result};

use super::venue::Venue;

/// Return `Ok(())` if the venue / network combination is safe to proceed, or
/// `Err` if it is a real-money Byreal mainnet call and `ack` is false.
///
/// Decision table:
///
/// | venue  | byreal_network         | ack   | result |
/// |--------|------------------------|-------|--------|
/// | Byreal | None / "" / "mainnet"  | false | Err    |
/// | Byreal | None / "" / "mainnet"  | true  | Ok     |
/// | Byreal | contains "testnet"     | any   | Ok     |
/// | Alpaca | any                    | any   | Ok     |
/// | Orderly| any                    | any   | Ok     |
///
/// `byreal_network` is the value of `$BYREAL_NETWORK` (pass
/// `std::env::var("BYREAL_NETWORK").ok().as_deref()` at the call site).
pub fn require_real_money_ack(venue: Venue, byreal_network: Option<&str>, ack: bool) -> Result<()> {
    match venue {
        Venue::Alpaca | Venue::Orderly => Ok(()),
        Venue::Byreal => {
            // Fail-safe: anything that is not explicitly "testnet" is treated
            // as mainnet (None, empty string, or any unrecognised value).
            let is_testnet = byreal_network
                .map(|n| n.to_ascii_lowercase().contains("testnet"))
                .unwrap_or(false);

            if is_testnet {
                return Ok(());
            }

            // Mainnet path: require the explicit ack.
            if ack {
                return Ok(());
            }

            bail!(
                "BYREAL_NETWORK is mainnet — this command will move REAL funds. \
                 Re-run with --i-understand-real-money to proceed."
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests — written BEFORE implementation (TDD red → green).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. byreal + mainnet (None) + no ack ⇒ Err mentioning --i-understand-real-money
    #[test]
    fn byreal_mainnet_none_no_ack_is_err() {
        let err = require_real_money_ack(Venue::Byreal, None, false).unwrap_err();
        assert!(
            err.to_string().contains("--i-understand-real-money"),
            "error must mention --i-understand-real-money; got: {err}"
        );
    }

    // 2. byreal + explicit "mainnet" + no ack ⇒ Err
    #[test]
    fn byreal_explicit_mainnet_no_ack_is_err() {
        let err = require_real_money_ack(Venue::Byreal, Some("mainnet"), false).unwrap_err();
        assert!(
            err.to_string().contains("--i-understand-real-money"),
            "error must mention --i-understand-real-money; got: {err}"
        );
    }

    // 3. byreal + mainnet + ack ⇒ Ok
    #[test]
    fn byreal_mainnet_with_ack_is_ok() {
        require_real_money_ack(Venue::Byreal, None, true).expect("ack should be accepted");
        require_real_money_ack(Venue::Byreal, Some("mainnet"), true).expect("ack should be accepted");
    }

    // 4. byreal + testnet + no ack ⇒ Ok
    #[test]
    fn byreal_testnet_no_ack_is_ok() {
        require_real_money_ack(Venue::Byreal, Some("testnet"), false).expect("testnet must not require ack");
    }

    // 5. alpaca + no ack ⇒ Ok (paper trading, never real money)
    #[test]
    fn alpaca_no_ack_is_ok() {
        require_real_money_ack(Venue::Alpaca, None, false).expect("alpaca must not require ack");
    }

    // 6. orderly + no ack ⇒ Ok (testnet, never real money via this path)
    #[test]
    fn orderly_no_ack_is_ok() {
        require_real_money_ack(Venue::Orderly, None, false).expect("orderly must not require ack");
    }

    // 7. byreal + empty string (unset env) + no ack ⇒ Err (fail-safe mainnet)
    #[test]
    fn byreal_empty_network_no_ack_is_err() {
        let err = require_real_money_ack(Venue::Byreal, Some(""), false).unwrap_err();
        assert!(
            err.to_string().contains("--i-understand-real-money"),
            "empty BYREAL_NETWORK must be treated as mainnet; got: {err}"
        );
    }

    // 8. byreal + "TESTNET" (uppercase) + no ack ⇒ Ok (case-insensitive)
    #[test]
    fn byreal_testnet_uppercase_no_ack_is_ok() {
        require_real_money_ack(Venue::Byreal, Some("TESTNET"), false)
            .expect("uppercase testnet must be accepted");
    }
}
