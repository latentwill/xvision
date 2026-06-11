#!/usr/bin/env python3
"""Orderly testnet onboarding (MANUAL.md M6, automated).

Registers the EVM wallet with Orderly's testnet gateway, announces a fresh
ed25519 trading key, requests faucet USDC, and verifies a signed read.

Inputs (env):
  EVM_PRIVATE_KEY   0x-prefixed EVM private key (signs EIP-712 only; no funds move)
  ORDERLY_BROKER_ID default "woofi_dex"
  ORDERLY_CHAIN_ID  default "421614" (Arbitrum Sepolia)
  ORDERLY_TESTNET_BASE default "https://testnet-api-evm.orderly.org"

Output: JSON on stdout with account_id / orderly_key / orderly_secret in the
exact formats crates/xvision-execution/src/orderly.rs `from_env` expects.
Save them to 1Password under xvision-orderly-testnet (MANUAL.md M6 step 2).
"""

import base64
import json
import os
import sys
import time

import base58
import requests
from eth_account import Account
from eth_account.messages import encode_typed_data
from nacl.signing import SigningKey

BASE = os.environ.get("ORDERLY_TESTNET_BASE", "https://testnet-api-evm.orderly.org")
BROKER_ID = os.environ.get("ORDERLY_BROKER_ID", "woofi_dex")
CHAIN_ID = int(os.environ.get("ORDERLY_CHAIN_ID", "421614"))
FAUCET_URL = "https://testnet-operator-evm.orderly.org/v1/faucet/usdc"

OFFCHAIN_DOMAIN = {
    "name": "Orderly",
    "version": "1",
    "chainId": CHAIN_ID,
    "verifyingContract": "0xCcCCccccCCCCcCCCCCCcCcCccCcCCCcCcccccccC",
}


def log(msg: str) -> None:
    print(msg, file=sys.stderr)


def sign_typed(acct, primary_type: str, types: dict, message: dict) -> str:
    full_types = {
        "EIP712Domain": [
            {"name": "name", "type": "string"},
            {"name": "version", "type": "string"},
            {"name": "chainId", "type": "uint256"},
            {"name": "verifyingContract", "type": "address"},
        ],
        **types,
    }
    data = {
        "types": full_types,
        "primaryType": primary_type,
        "domain": OFFCHAIN_DOMAIN,
        "message": message,
    }
    signed = acct.sign_message(encode_typed_data(full_message=data))
    return "0x" + signed.signature.hex().removeprefix("0x")


def get(path: str, **params):
    r = requests.get(f"{BASE}{path}", params=params, timeout=15)
    return r.json()


def post(path: str, body: dict):
    r = requests.post(f"{BASE}{path}", json=body, timeout=15)
    return r.json()


def main() -> int:
    pk = os.environ.get("EVM_PRIVATE_KEY")
    if not pk:
        log("EVM_PRIVATE_KEY not set")
        return 1
    acct = Account.from_key(pk)
    address = acct.address
    log(f"wallet: {address}  broker: {BROKER_ID}  chain: {CHAIN_ID}  base: {BASE}")

    # 1. Existing account?
    existing = get("/v1/get_account", address=address, broker_id=BROKER_ID)
    if existing.get("success") and existing.get("data", {}).get("account_id"):
        account_id = existing["data"]["account_id"]
        log(f"account already registered: {account_id}")
    else:
        nonce_resp = get("/v1/registration_nonce")
        nonce = nonce_resp["data"]["registration_nonce"]
        msg = {
            "brokerId": BROKER_ID,
            "chainId": CHAIN_ID,
            "timestamp": int(time.time() * 1000),
            "registrationNonce": int(nonce),
        }
        sig = sign_typed(
            acct,
            "Registration",
            {
                "Registration": [
                    {"name": "brokerId", "type": "string"},
                    {"name": "chainId", "type": "uint256"},
                    {"name": "timestamp", "type": "uint64"},
                    {"name": "registrationNonce", "type": "uint256"},
                ]
            },
            msg,
        )
        reg = post(
            "/v1/register_account",
            {"message": msg, "signature": sig, "userAddress": address},
        )
        if not reg.get("success"):
            log(f"register_account failed: {json.dumps(reg)}")
            return 1
        account_id = reg["data"]["account_id"]
        log(f"registered account: {account_id}")

    # 2. Announce a fresh ed25519 orderly key (read+trading, 30 days).
    signing_key = SigningKey.generate()
    pub_b58 = base58.b58encode(bytes(signing_key.verify_key)).decode()
    seed_b58 = base58.b58encode(bytes(signing_key)).decode()
    orderly_key = f"ed25519:{pub_b58}"
    now_ms = int(time.time() * 1000)
    key_msg = {
        "brokerId": BROKER_ID,
        "chainId": CHAIN_ID,
        "orderlyKey": orderly_key,
        "scope": "read,trading",
        "timestamp": now_ms,
        "expiration": now_ms + 30 * 24 * 3600 * 1000,
    }
    key_sig = sign_typed(
        acct,
        "AddOrderlyKey",
        {
            "AddOrderlyKey": [
                {"name": "brokerId", "type": "string"},
                {"name": "chainId", "type": "uint256"},
                {"name": "orderlyKey", "type": "string"},
                {"name": "scope", "type": "string"},
                {"name": "timestamp", "type": "uint64"},
                {"name": "expiration", "type": "uint64"},
            ]
        },
        key_msg,
    )
    key_resp = post(
        "/v1/orderly_key",
        {"message": key_msg, "signature": key_sig, "userAddress": address},
    )
    if not key_resp.get("success"):
        log(f"orderly_key failed: {json.dumps(key_resp)}")
        return 1
    log(f"orderly key announced: {orderly_key}")

    # 3. Faucet USDC (best-effort; ignores 'already claimed' style errors).
    try:
        f = requests.post(
            FAUCET_URL,
            json={
                "broker_id": BROKER_ID,
                "chain_id": str(CHAIN_ID),
                "user_address": address,
            },
            timeout=20,
        ).json()
        log(f"faucet: {json.dumps(f)}")
    except Exception as e:  # noqa: BLE001
        log(f"faucet request errored (non-fatal): {e}")

    # 4. Verify a signed read, trying both base64 variants for the signature
    #    so we learn which one this gateway accepts (the Rust executor uses
    #    standard base64; Orderly docs say url-safe).
    def signed_get(path: str, urlsafe: bool):
        ts = str(int(time.time() * 1000))
        message = f"{ts}GET{path}"
        raw = signing_key.sign(message.encode()).signature
        enc = base64.urlsafe_b64encode(raw) if urlsafe else base64.b64encode(raw)
        headers = {
            "orderly-timestamp": ts,
            "orderly-account-id": account_id,
            "orderly-key": orderly_key,
            "orderly-signature": enc.decode(),
        }
        return requests.get(f"{BASE}{path}", headers=headers, timeout=15).json()

    holding = None
    sig_mode = None
    for urlsafe in (True, False):
        time.sleep(1)
        resp = signed_get("/v1/client/holding", urlsafe)
        if resp.get("success"):
            holding = resp
            sig_mode = "urlsafe" if urlsafe else "standard"
            break
        log(f"signed read ({'urlsafe' if urlsafe else 'standard'} b64) failed: {json.dumps(resp)}")
    if holding is None:
        log("signed holding read failed with both signature encodings")
        return 1
    log(f"signed read OK with {sig_mode} base64; holding: {json.dumps(holding['data'])}")

    print(
        json.dumps(
            {
                "address": address,
                "broker_id": BROKER_ID,
                "chain_id": CHAIN_ID,
                "base_url": BASE,
                "account_id": account_id,
                "orderly_key": orderly_key,
                "orderly_secret": seed_b58,
                "signature_b64_mode": sig_mode,
                "holding": holding["data"],
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
