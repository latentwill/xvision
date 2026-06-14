#!/usr/bin/env bash
# scripts/deploy-mainnet.sh — deploy xvision marketplace contracts to Mantle
# MAINNET (chain 5000), hackathon-grade (operator EOA admin; V4 gate skipped).
#
# Mirrors scripts/deploy-testnet.sh. Reads the deployer key + OPERATOR_EOA from
# 1Password ("XVN Wallet" / Olympus). USDC.e + URIs come from env (defaults
# below). After a successful broadcast: parses the 8 deployed addresses, writes
# them into config/mantle.toml, verifies each with `cast code`, mints the
# platform agent NFT (token id 0), and prints the XVN_* runtime env that wakes
# the dashboard marketplace indexer.
#
# Usage:
#   bash scripts/deploy-mainnet.sh --dry-run   # simulate only (no broadcast, no funds) — prints gas estimate
#   bash scripts/deploy-mainnet.sh             # full deploy + plumb (REAL MNT gas)
#
# Requirements: forge, cast, op (1Password CLI), python3
#
# REAL-MONEY / IRREVERSIBLE: a broadcast deploys permanent mainnet contracts and
# spends real MNT. Run --dry-run first.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RPC_URL="${XVN_RPC_URL:-https://rpc.mantle.xyz}"
CHAIN_ID=5000
CONFIG="$REPO_ROOT/config/mantle.toml"
OP_ITEM="XVN Wallet"
OP_VAULT="Olympus"

# Non-secret config (override via env). USDC.e on Mantle mainnet — VERIFY before
# a real broadcast (must be the bridged USDC.e the Orderly/marketplace flow uses;
# EIP-3009 support is required only for the x402 buyWithAuthorization path).
USDC_ADDRESS="${USDC_ADDRESS:-0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9}"
LICENSE_URI="${LICENSE_URI:-https://xvnapp.com/api/licenses/{id}}"
PLATFORM_MANIFEST_URI="${PLATFORM_MANIFEST_URI:-https://xvnapp.com/.well-known/erc8004.json}"
PROTOCOL_FEE_BPS="${PROTOCOL_FEE_BPS:-500}"
# Minimum deployer balance (MNT) required before broadcasting. The --dry-run
# estimate for the full deploy is ~0.31 MNT; 1 MNT covers deploy + platform-agent
# mint + margin. Override with XVN_DEPLOY_MIN_MNT.
MIN_MNT="${XVN_DEPLOY_MIN_MNT:-1}"

BROADCAST=true
[[ "${1:-}" == "--dry-run" ]] && BROADCAST=false

die()  { echo "ERROR: $*" >&2; exit 1; }
step() { echo ""; echo "==> $*"; }
ok()   { echo "    ✓ $*"; }

# ---------------------------------------------------------------------------
# 1. Load secrets
# ---------------------------------------------------------------------------
step "Loading secrets from 1Password ('$OP_ITEM' / $OP_VAULT)..."
_op() { op item get "$OP_ITEM" --vault "$OP_VAULT" --field "$1" "${@:2}" 2>/dev/null | tr -d '"' | tr -d '\n' | xargs; }

PRIVATE_KEY="$(_op "private key" --reveal)"
OPERATOR_EOA="$(_op "address")"

[[ -n "$PRIVATE_KEY"  ]] || die "field 'private key' not found in OP item '$OP_ITEM'"
[[ -n "$OPERATOR_EOA" ]] || die "field 'address' not found in OP item '$OP_ITEM'"

ok "OPERATOR_EOA = $OPERATOR_EOA  (proxy admin + fee recipient — hackathon-grade)"
ok "USDC_ADDRESS = $USDC_ADDRESS"
echo "    ⚠ verify USDC_ADDRESS is the correct Mantle-mainnet USDC.e before broadcasting."

# ---------------------------------------------------------------------------
# 2. Pre-flight: deployer balance (broadcast only)
# ---------------------------------------------------------------------------
if $BROADCAST; then
  step "Checking deployer balance on Mantle mainnet..."
  BALANCE_WEI="$(cast balance "$OPERATOR_EOA" --rpc-url "$RPC_URL")"
  BALANCE_MNT="$(python3 -c "print(f'{int(\"$BALANCE_WEI\") / 1e18:.4f}')")"
  echo "    Balance: $BALANCE_MNT MNT  (min required: $MIN_MNT MNT)"
  python3 -c "
import sys
b = int('$BALANCE_WEI'); need = float('$MIN_MNT') * 1e18
sys.exit(0 if b >= need else 1)
" || die "Insufficient MNT for gas (have $BALANCE_MNT, need $MIN_MNT). Fund $OPERATOR_EOA on Mantle, or lower XVN_DEPLOY_MIN_MNT."
  ok "Balance sufficient"
else
  echo "    (dry-run: skipping balance check)"
fi

# ---------------------------------------------------------------------------
# 3. Build contracts
# ---------------------------------------------------------------------------
step "Building contracts..."
cd "$REPO_ROOT/contracts"
forge build --quiet
ok "Build clean"

# ---------------------------------------------------------------------------
# 4. Run deploy script (simulate, or broadcast)
# ---------------------------------------------------------------------------
BROADCAST_FLAG=""
$BROADCAST && BROADCAST_FLAG="--broadcast"

step "Running DeployMainnet.s.sol${BROADCAST:+ (BROADCASTING to chain $CHAIN_ID — REAL MNT)}..."

DEPLOY_OUTPUT="$(
  OPERATOR_EOA="$OPERATOR_EOA" \
  USDC_ADDRESS="$USDC_ADDRESS" \
  LICENSE_URI="$LICENSE_URI" \
  PROTOCOL_FEE_BPS="$PROTOCOL_FEE_BPS" \
  forge script script/DeployMainnet.s.sol \
    --rpc-url "$RPC_URL" \
    --private-key "$PRIVATE_KEY" \
    $BROADCAST_FLAG \
    2>&1
)"
echo "$DEPLOY_OUTPUT"

if ! $BROADCAST; then
  echo ""
  echo "DRY RUN complete (no broadcast). The 'Estimated amount required' line above"
  echo "is the MNT gas cost. Re-run without --dry-run to broadcast."
  exit 0
fi

# ---------------------------------------------------------------------------
# 5. Parse addresses from forge log output
# ---------------------------------------------------------------------------
step "Parsing deployed addresses..."
_addr() { echo "$DEPLOY_OUTPUT" | grep -i "$1" | grep -oE '0x[0-9a-fA-F]{40}' | tail -1; }

XVN_DEPLOYER="$(_addr "XvnDeployer")"
IDENTITY_REGISTRY="$(_addr "IdentityRegistry")"
REPUTATION_REGISTRY="$(_addr "ReputationRegistry")"
VALIDATION_REGISTRY="$(_addr "ValidationRegistry")"
LICENSE_TOKEN="$(_addr "LicenseToken")"
LISTING_REGISTRY="$(_addr "ListingRegistry")"
EVAL_ATTESTATION="$(_addr "EvalAttestation")"
MARKETPLACE_ADDR="$(_addr "Marketplace (proxy)")"

for var in XVN_DEPLOYER IDENTITY_REGISTRY REPUTATION_REGISTRY VALIDATION_REGISTRY \
           LICENSE_TOKEN LISTING_REGISTRY EVAL_ATTESTATION MARKETPLACE_ADDR; do
  val="${!var}"
  [[ -n "$val" && "$val" != "0x0000000000000000000000000000000000000000" ]] \
    || die "Failed to parse $var — check forge output above"
  ok "$var = $val"
done

# ---------------------------------------------------------------------------
# 6. Verify bytecode on-chain
# ---------------------------------------------------------------------------
step "Verifying bytecode on Mantle mainnet..."
for addr in "$XVN_DEPLOYER" "$IDENTITY_REGISTRY" "$REPUTATION_REGISTRY" \
            "$VALIDATION_REGISTRY" "$LICENSE_TOKEN" "$LISTING_REGISTRY" \
            "$EVAL_ATTESTATION" "$MARKETPLACE_ADDR"; do
  code="$(cast code "$addr" --rpc-url "$RPC_URL")"
  [[ "$code" != "0x" ]] || die "No bytecode at $addr — deploy may have reverted"
  ok "$addr"
done

# ---------------------------------------------------------------------------
# 7. Update config/mantle.toml
# ---------------------------------------------------------------------------
step "Writing addresses to config/mantle.toml..."
cd "$REPO_ROOT"
python3 - <<PYEOF
import re
with open('$CONFIG') as f:
    content = f.read()
def sub_field(text, key, value):
    return re.sub(r'(?m)^(' + re.escape(key) + r'[ \t]*=[ \t]*)"0x0{40}"', r'\1"' + value + '"', text)
for key, val in [
    ('identity_registry',   '$IDENTITY_REGISTRY'),
    ('reputation_registry', '$REPUTATION_REGISTRY'),
    ('validation_registry', '$VALIDATION_REGISTRY'),
    ('xvn_deployer',        '$XVN_DEPLOYER'),
    ('listing_registry',    '$LISTING_REGISTRY'),
    ('marketplace',         '$MARKETPLACE_ADDR'),
    ('license_token',       '$LICENSE_TOKEN'),
    ('eval_attestation',    '$EVAL_ATTESTATION'),
    ('fee_recipient',       '$OPERATOR_EOA'),
    ('admin',               '$OPERATOR_EOA'),
]:
    content = sub_field(content, key, val)
remaining = re.findall(r'"0x0{40}"', content)
if remaining:
    raise SystemExit(f"ERROR: {len(remaining)} zero-address placeholder(s) remain in config")
with open('$CONFIG', 'w') as f:
    f.write(content)
print("    Config written.")
PYEOF
ok "config/mantle.toml updated"

# ---------------------------------------------------------------------------
# 8. Mint platform agent (token id 0)
# ---------------------------------------------------------------------------
step "Minting platform agent NFT (RegisterPlatformAgent)..."
cd "$REPO_ROOT/contracts"
REGISTER_OUTPUT="$(
  IDENTITY_REGISTRY="$IDENTITY_REGISTRY" \
  PLATFORM_MANIFEST_URI="$PLATFORM_MANIFEST_URI" \
  forge script script/RegisterPlatformAgent.s.sol \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" --broadcast 2>&1
)"
echo "$REGISTER_OUTPUT"
TOKEN_ID="$(echo "$REGISTER_OUTPUT" | grep -i "token id" | grep -oE '[0-9]+' | tail -1 || true)"
if [[ -n "$TOKEN_ID" ]]; then
  ok "Platform agent minted as token id $TOKEN_ID"
  cd "$REPO_ROOT"
  sed -i '' "s/^platform_agent_token_id.*=.*0/platform_agent_token_id = $TOKEN_ID/" "$CONFIG"
else
  echo "    (token id not parsed — check output above)"
fi

# ---------------------------------------------------------------------------
# 9. Summary + runtime env
# ---------------------------------------------------------------------------
echo ""
echo "================================================================"
echo " Mantle MAINNET deploy complete (chain 5000)"
echo "================================================================"
echo " XvnDeployer:        $XVN_DEPLOYER"
echo " IdentityRegistry:   $IDENTITY_REGISTRY"
echo " ReputationRegistry: $REPUTATION_REGISTRY"
echo " ValidationRegistry: $VALIDATION_REGISTRY"
echo " LicenseToken:       $LICENSE_TOKEN"
echo " ListingRegistry:    $LISTING_REGISTRY"
echo " EvalAttestation:    $EVAL_ATTESTATION"
echo " Marketplace:        $MARKETPLACE_ADDR"
echo ""
echo " Mantlescan: https://explorer.mantle.xyz/address/$MARKETPLACE_ADDR"
echo ""
echo " Wake the dashboard marketplace indexer with these runtime env vars:"
echo "   export XVN_RPC_URL=$RPC_URL"
echo "   export XVN_CHAIN_ID=5000"
echo "   export XVN_IDENTITY_REGISTRY=$IDENTITY_REGISTRY"
echo "   export XVN_LISTING_REGISTRY=$LISTING_REGISTRY"
echo "   export XVN_LICENSE_TOKEN=$LICENSE_TOKEN"
echo "   export XVN_MARKETPLACE_CONTRACT=$MARKETPLACE_ADDR"
echo "   export XVN_MARKETPLACE_USDC=$USDC_ADDRESS"
echo ""
echo " Then commit the config:"
echo "   git add config/mantle.toml && git commit -m 'chore(contracts): deploy marketplace to Mantle mainnet (5000)'"
