//! EIP-3009 (`transferWithAuthorization`) typed-data: the off-chain crypto for
//! the x402 `exact` scheme. Pure — no network, no chain. Mirrors the EIP-712
//! pattern in `xvision-execution/src/virtuals.rs`.

use alloy::primitives::{Address, Signature, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use alloy::sol;
use alloy::sol_types::{eip712_domain, Eip712Domain, SolStruct};

use crate::error::MarketplaceError;

sol! {
    /// EIP-3009 TransferWithAuthorization payload (the EIP-712 message body).
    struct TransferWithAuthorization {
        address from;
        address to;
        uint256 value;
        uint256 validAfter;
        uint256 validBefore;
        bytes32 nonce;
    }
}

/// Build the USDC EIP-712 domain. `name`/`version` are invariant for Circle
/// FiatTokenV2 USDC ("USD Coin"/"2") on every chain, so only the per-network
/// values vary: `chain_id` (Mantle mainnet 5000, Sepolia 5003) and the USDC
/// `verifyingContract` address. Hardcoding name/version removes a footgun where
/// a wrong literal would silently produce a domain the contract rejects.
pub fn usdc_domain(chain_id: u64, usdc: Address) -> Eip712Domain {
    eip712_domain! {
        name: "USD Coin",
        version: "2",
        chain_id: chain_id,
        verifying_contract: usdc,
    }
}

/// The unsigned EIP-3009 authorization (host-friendly field names).
#[derive(Debug, Clone)]
pub struct Authorization {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub valid_after: U256,
    pub valid_before: U256,
    pub nonce: B256,
}

/// Legacy-`v` (27/28) signature parts.
#[derive(Debug, Clone, Copy)]
pub struct SignedParts {
    pub v: u8,
    pub r: B256,
    pub s: B256,
}

impl Authorization {
    fn to_sol(&self) -> TransferWithAuthorization {
        TransferWithAuthorization {
            from: self.from,
            to: self.to,
            value: self.value,
            validAfter: self.valid_after,
            validBefore: self.valid_before,
            nonce: self.nonce,
        }
    }
}

/// EIP-712 digest the buyer signs.
pub fn signing_hash(auth: &Authorization, domain: &Eip712Domain) -> B256 {
    auth.to_sol().eip712_signing_hash(domain)
}

/// Sign locally with the buyer's key (non-custodial). Never sends the key — only (v, r, s).
pub fn sign_authorization(
    signer: &PrivateKeySigner,
    auth: &Authorization,
    domain: &Eip712Domain,
) -> Result<SignedParts, MarketplaceError> {
    let hash = signing_hash(auth, domain);
    let sig = signer
        .sign_hash_sync(&hash)
        .map_err(|e| MarketplaceError::Signing(format!("eip3009 sign: {e}")))?;
    Ok(SignedParts {
        // alloy-primitives 1.5.7: v()->bool, r()/s()->U256 (no Into<B256>).
        v: 27 + sig.v() as u8,
        r: B256::from(sig.r().to_be_bytes::<32>()),
        s: B256::from(sig.s().to_be_bytes::<32>()),
    })
}

/// Off-chain `ecrecover` for the facilitator `/verify` step.
pub fn recover_authorizer(
    auth: &Authorization,
    domain: &Eip712Domain,
    v: u8,
    r: B256,
    s: B256,
) -> Result<Address, MarketplaceError> {
    let hash = signing_hash(auth, domain);
    // USDC transferWithAuthorization requires v ∈ {27, 28}; reject anything else
    // rather than silently coercing (e.g. v=29) to a wrong parity.
    let parity = match v {
        27 => false,
        28 => true,
        other => return Err(MarketplaceError::Signing(format!("bad v: {other}"))),
    };
    let sig = Signature::from_scalars_and_parity(r, s, parity);
    sig.recover_address_from_prehash(&hash)
        .map_err(|e| MarketplaceError::Signing(format!("ecrecover: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // Canonical EIP-3009 typehash (verified on-chain on Mantle mainnet USDC.e).
    const CANON_TYPEHASH: &str = "0x7c7c6cdb67a18743f49ec6fa9b35f50d52ed05cbed4cc592e13b44501c1a2267";
    // On-chain DOMAIN_SEPARATOR() of Mantle mainnet USDC.e (chainId 5000).
    const MANTLE_USDC_DOMAIN_SEP: &str = "0x213af627bcb897cb58330ea735c1dceb19deed319fd39bbb200b6fc6bd5450cd";
    const MANTLE_USDC: &str = "0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9";

    #[test]
    fn typehash_matches_canonical_eip3009() {
        use alloy::primitives::keccak256;
        // Hash `eip712_encode_type()` directly so the test string-asserts the
        // canonical type string before taking its keccak — auditable without a
        // live contract.
        let encoded = <TransferWithAuthorization as SolStruct>::eip712_encode_type();
        assert_eq!(
            encoded,
            "TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)"
        );
        let got = keccak256(encoded.as_bytes());
        assert_eq!(format!("0x{:x}", got), CANON_TYPEHASH);
    }

    #[test]
    fn domain_separator_matches_mantle_mainnet() {
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain(5000, usdc);
        let sep = domain.separator();
        assert_eq!(format!("0x{:x}", sep), MANTLE_USDC_DOMAIN_SEP);
    }

    #[test]
    fn sign_then_recover_round_trips() {
        let signer = PrivateKeySigner::random();
        let from = signer.address();
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain(5000, usdc);

        let auth = Authorization {
            from,
            to: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
            value: U256::from(49_000_000u64),
            valid_after: U256::ZERO,
            valid_before: U256::from(9_999_999_999u64),
            nonce: B256::repeat_byte(0x11),
        };

        let signed = sign_authorization(&signer, &auth, &domain).unwrap();
        let recovered = recover_authorizer(&auth, &domain, signed.v, signed.r, signed.s).unwrap();
        assert_eq!(recovered, from);
    }

    #[test]
    fn recover_rejects_tampered_value() {
        let signer = PrivateKeySigner::random();
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain(5000, usdc);
        let mut auth = Authorization {
            from: signer.address(),
            to: Address::ZERO,
            value: U256::from(1u64),
            valid_after: U256::ZERO,
            valid_before: U256::from(9_999_999_999u64),
            nonce: B256::ZERO,
        };
        let signed = sign_authorization(&signer, &auth, &domain).unwrap();
        auth.value = U256::from(999u64); // tamper
        let recovered = recover_authorizer(&auth, &domain, signed.v, signed.r, signed.s).unwrap();
        assert_ne!(recovered, signer.address());
    }

    #[test]
    fn recover_rejects_bad_v() {
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain(5000, usdc);
        let auth = Authorization {
            from: Address::ZERO,
            to: Address::ZERO,
            value: U256::ZERO,
            valid_after: U256::ZERO,
            valid_before: U256::from(9_999_999_999u64),
            nonce: B256::ZERO,
        };
        // v outside {27, 28} must error, not silently coerce.
        assert!(recover_authorizer(&auth, &domain, 29, B256::ZERO, B256::ZERO).is_err());
        assert!(recover_authorizer(&auth, &domain, 26, B256::ZERO, B256::ZERO).is_err());
    }
}
