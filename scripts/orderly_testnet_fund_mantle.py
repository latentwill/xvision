#!/usr/bin/env python3
"""Fund an Orderly testnet account via Mantle Sepolia (on-chain path).

Orderly's off-chain USDC faucet (`testnet-operator-evm`) accepts claims but
credits unreliably (2026-06-11: woofi_dex/woofi_pro claims never settled).
This script takes the reliable on-chain route instead:

  1. `faucet()` on Orderly's test USDC token (public, mints 1000 units)
  2. `approve()` the Orderly vault
  3. `vault.deposit()` → cross-chain manager credits the account ledger
     in ~1-3 minutes

Inputs (env):
  EVM_PRIVATE_KEY     0x-prefixed key; the wallet needs Mantle Sepolia MNT
                      for gas (faucet.sepolia.mantle.xyz)
  ORDERLY_CREDS_JSON  path to the onboarding output JSON
                      (from scripts/orderly_testnet_onboard.py); supplies
                      account_id + broker_id
  DEPOSIT_USDC        amount to deposit, default 5000

Deps: pip install web3 (plus the onboarding script's deps for the holding
check). The Vault/NativeUSDC ABIs are fetched from OrderlyNetwork/examples
if not present next to this script.
"""

import json
import os
import sys
import time
import urllib.request

from web3 import Web3

RPC = "https://rpc.sepolia.mantle.xyz/"
CHAIN_ID = 5003
TOKEN = "0xAcab8129E2cE587fD203FD770ec9ECAFA2C88080"  # USDC.e (test), 6 dp
VAULT = "0xfb0E5f3D16758984E668A3d76f0963710E775503"
ABI_BASE = (
    "https://raw.githubusercontent.com/OrderlyNetwork/examples/main/api/py/src/abi"
)
FAUCET_FN_ABI = [
    {"inputs": [], "name": "faucet", "outputs": [], "stateMutability": "nonpayable", "type": "function"}
]


def load_abi(name: str):
    local = os.path.join(os.path.dirname(__file__), f".orderly-abi-{name}.json")
    if not os.path.exists(local):
        with urllib.request.urlopen(f"{ABI_BASE}/{name}.json", timeout=30) as r:
            data = r.read()
        with open(local, "wb") as f:
            f.write(data)
    return json.load(open(local))


def main() -> int:
    w3 = Web3(Web3.HTTPProvider(RPC))
    if w3.eth.chain_id != CHAIN_ID:
        print(f"refusing: chain_id {w3.eth.chain_id} != {CHAIN_ID}", file=sys.stderr)
        return 1
    acct = w3.eth.account.from_key(os.environ["EVM_PRIVATE_KEY"])
    addr = acct.address
    creds = json.load(open(os.environ.get("ORDERLY_CREDS_JSON", "/tmp/orderly-demo-creds.json")))
    amount = int(float(os.environ.get("DEPOSIT_USDC", "5000")) * 10**6)

    usdc = w3.eth.contract(
        address=Web3.to_checksum_address(TOKEN),
        abi=load_abi("NativeUSDC") + FAUCET_FN_ABI,
    )
    vault = w3.eth.contract(address=Web3.to_checksum_address(VAULT), abi=load_abi("Vault"))

    def send(fn, value=0):
        tx = fn.build_transaction({
            "from": addr,
            "nonce": w3.eth.get_transaction_count(addr, "pending"),
            "value": value,
            "gasPrice": w3.eth.gas_price,
        })
        tx["gas"] = int(w3.eth.estimate_gas(tx) * 1.3)
        h = w3.eth.send_raw_transaction(acct.sign_transaction(tx).raw_transaction)
        r = w3.eth.wait_for_transaction_receipt(h, timeout=180)
        print(f"  tx {h.hex()} status={r.status}", file=sys.stderr)
        assert r.status == 1, "tx reverted"
        return r

    bal = usdc.functions.balanceOf(addr).call()
    if bal < amount:
        print("faucet() ...", file=sys.stderr)
        send(usdc.functions.faucet())
        for _ in range(10):
            bal = usdc.functions.balanceOf(addr).call()
            if bal >= amount:
                break
            time.sleep(3)
    if bal < amount:
        print(f"insufficient USDC after faucet: {bal}", file=sys.stderr)
        return 1

    print("approve ...", file=sys.stderr)
    send(usdc.functions.approve(Web3.to_checksum_address(VAULT), amount))

    data = (
        bytes.fromhex(creds["account_id"][2:]),
        Web3.keccak(text=creds["broker_id"]),
        Web3.keccak(text="USDC"),
        amount,
    )
    fee = vault.functions.getDepositFee(addr, data).call()
    print(
        f"deposit {amount/1e6} USDC → {creds['account_id'][:12]}… "
        f"(broker {creds['broker_id']}, fee {fee/1e18:.4f} MNT)",
        file=sys.stderr,
    )
    send(vault.functions.deposit(data), value=fee)
    print("deposited — ledger credit lands in ~1-3 min (poll /v1/client/holding)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
