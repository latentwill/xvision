#!/usr/bin/env bash
# scripts/deploy-testnet.sh — deploy xvision contracts to Mantle Sepolia + plumb addresses
#
# Reads secrets from 1Password ("XVN Wallet" item, Olympus vault).
# After a successful broadcast: parses the 8 deployed addresses, writes them
# into config/mantle-sepolia.toml, verifies each with cast code, and mints
# the platform agent NFT (token id 0) via RegisterPlatformAgent.
#
# Usage:
#   bash scripts/deploy-testnet.sh             # full deploy + plumb
#   bash scripts/deploy-testnet.sh --dry-run   # simulate only (no broadcast)
#
# Requirements: forge, cast, op (1Password CLI), python3

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RPC_URL="https://rpc.sepolia.mantle.xyz"
CHAIN_ID=5003
CONFIG="$REPO_ROOT/config/mantle-sepolia.toml"
OP_ITEM="XVN Wallet"
OP_VAULT="Olympus"

# Non-secret config (from the OP item's text field reference)
LICENSE_URI="https://xvnapp.com/api/licenses/{id}"
PLATFORM_MANIFEST_URI="https://xvnapp.com/.well-known/erc8004.json"
PROTOCOL_FEE_BPS="500"

BROADCAST=true
[[ "${1:-}" == "--dry-run" ]] && BROADCAST=false

die()  { echo "ERROR: $*" >&2; exit 1; }
step() { echo ""; echo "==> $*"; }
ok()   { echo "    ✓ $*"; }

# ---------------------------------------------------------------------------
# 1. Load secrets
# ---------------------------------------------------------------------------
step "Loading secrets from 1Password ('$OP_ITEM' / $OP_VAULT)..."

# op may return values wrapped in extra quotes — strip them
_op() { op item get "$OP_ITEM" --vault "$OP_VAULT" --field "$1" "${@:2}" 2>/dev/null | tr -d '"' | tr -d '\n' | xargs; }

PRIVATE_KEY="$(_op "private key" --reveal)"
OPERATOR_EOA="$(_op "address")"
USDC_ADDRESS="$(_op "USDC_ADDRESS")"

[[ -n "$PRIVATE_KEY"  ]] || die "field 'private key' not found in OP item '$OP_ITEM'"
[[ -n "$OPERATOR_EOA" ]] || die "field 'address' not found in OP item '$OP_ITEM'"
[[ -n "$USDC_ADDRESS" ]] || die "field 'USDC_ADDRESS' not found in OP item '$OP_ITEM'"

ok "OPERATOR_EOA = $OPERATOR_EOA"
ok "USDC_ADDRESS  = $USDC_ADDRESS"

# ---------------------------------------------------------------------------
# 2. Pre-flight: wallet balance
# ---------------------------------------------------------------------------
step "Checking deployer balance..."
BALANCE_WEI="$(cast balance "$OPERATOR_EOA" --rpc-url "$RPC_URL")"
BALANCE_MNT="$(python3 -c "print(f'{int('$BALANCE_WEI') / 1e18:.4f}')")"
echo "    Balance: $BALANCE_MNT MNT"
python3 -c "
b = int('$BALANCE_WEI')
assert b >= 50_000_000_000_000_000, f'Need at least 0.05 MNT for gas, have {b/1e18:.4f}'
" || die "Insufficient balance"
ok "Balance sufficient"

# ---------------------------------------------------------------------------
# 3. Build contracts
# ---------------------------------------------------------------------------
step "Building contracts..."
cd "$REPO_ROOT/contracts"
forge build --quiet
ok "Build clean"

# ---------------------------------------------------------------------------
# 4. Run deploy script
# ---------------------------------------------------------------------------
BROADCAST_FLAG=""
$BROADCAST && BROADCAST_FLAG="--broadcast"

step "Running DeployTestnet.s.sol${BROADCAST:+ (broadcasting to chain $CHAIN_ID)}..."

DEPLOY_OUTPUT="$(
  OPERATOR_EOA="$OPERATOR_EOA" \
  USDC_ADDRESS="$USDC_ADDRESS" \
  LICENSE_URI="$LICENSE_URI" \
  PROTOCOL_FEE_BPS="$PROTOCOL_FEE_BPS" \
  forge script script/DeployTestnet.s.sol \
    --rpc-url "$RPC_URL" \
    --private-key "$PRIVATE_KEY" \
    $BROADCAST_FLAG \
    2>&1
)"

echo "$DEPLOY_OUTPUT"

if ! $BROADCAST; then
  echo ""
  echo "DRY RUN complete. Re-run without --dry-run to broadcast."
  exit 0
fi

# ---------------------------------------------------------------------------
# 5. Parse addresses from forge log output
# ---------------------------------------------------------------------------
step "Parsing deployed addresses..."

# Each console2.log line: "  <Label>   <address>"
_addr() {
  local label="$1"
  echo "$DEPLOY_OUTPUT" \
    | grep -i "$label" \
    | grep -oE '0x[0-9a-fA-F]{40}' \
    | tail -1
}

XVN_DEPLOYER="$(_addr "XvnDeployer")"
IDENTITY_REGISTRY="$(_addr "IdentityRegistry")"
REPUTATION_REGISTRY="$(_addr "ReputationRegistry")"
VALIDATION_REGISTRY="$(_addr "ValidationRegistry")"
LICENSE_TOKEN="$(_addr "LicenseToken")"
LISTING_REGISTRY="$(_addr "ListingRegistry")"
EVAL_ATTESTATION="$(_addr "EvalAttestation")"
MARKETPLACE_ADDR="$(_addr "Marketplace (proxy)")"

# Validate all parsed
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
step "Verifying bytecode on Mantle Sepolia..."
for addr in "$XVN_DEPLOYER" "$IDENTITY_REGISTRY" "$REPUTATION_REGISTRY" \
            "$VALIDATION_REGISTRY" "$LICENSE_TOKEN" "$LISTING_REGISTRY" \
            "$EVAL_ATTESTATION" "$MARKETPLACE_ADDR"; do
  code="$(cast code "$addr" --rpc-url "$RPC_URL")"
  [[ "$code" != "0x" ]] || die "No bytecode at $addr — deploy may have reverted"
  ok "$addr"
done

# ---------------------------------------------------------------------------
# 7. Update config/mantle-sepolia.toml
# ---------------------------------------------------------------------------
step "Writing addresses to config/mantle-sepolia.toml..."
cd "$REPO_ROOT"

python3 - <<PYEOF
import re

with open('$CONFIG') as f:
    content = f.read()

def sub_field(text, key, value):
    # Replace: <key>   = "0x000...000"   (any amount of zeros, comment OK)
    return re.sub(
        r'(?m)^(' + re.escape(key) + r'[ \t]*=[ \t]*)"0x0{40}"',
        r'\1"' + value + '"',
        text
    )

content = sub_field(content, 'identity_registry',   '$IDENTITY_REGISTRY')
content = sub_field(content, 'reputation_registry', '$REPUTATION_REGISTRY')
content = sub_field(content, 'validation_registry', '$VALIDATION_REGISTRY')
content = sub_field(content, 'xvn_deployer',        '$XVN_DEPLOYER')
content = sub_field(content, 'listing_registry',    '$LISTING_REGISTRY')
content = sub_field(content, 'marketplace',         '$MARKETPLACE_ADDR')
content = sub_field(content, 'license_token',       '$LICENSE_TOKEN')
content = sub_field(content, 'eval_attestation',    '$EVAL_ATTESTATION')
content = sub_field(content, 'fee_recipient',       '$OPERATOR_EOA')
content = sub_field(content, 'admin',               '$OPERATOR_EOA')
# USDC in the [marketplace.usdc] section (key is just "address")
content = sub_field(content, 'address',             '$USDC_ADDRESS')

# Zero must be gone — fail loudly rather than silently leaving placeholders
remaining = re.findall(r'"0x0{40}"', content)
if remaining:
    raise SystemExit(f"ERROR: {len(remaining)} zero-address placeholder(s) remain in config")

with open('$CONFIG', 'w') as f:
    f.write(content)

print("    Config written.")
PYEOF

ok "config/mantle-sepolia.toml updated"

# ---------------------------------------------------------------------------
# 8. Mint platform agent (token id 0)
# ---------------------------------------------------------------------------
step "Minting platform agent NFT (RegisterPlatformAgent)..."
cd "$REPO_ROOT/contracts"

REGISTER_OUTPUT="$(
  IDENTITY_REGISTRY="$IDENTITY_REGISTRY" \
  PLATFORM_MANIFEST_URI="$PLATFORM_MANIFEST_URI" \
  forge script script/RegisterPlatformAgent.s.sol \
    --rpc-url "$RPC_URL" \
    --private-key "$PRIVATE_KEY" \
    --broadcast \
    2>&1
)"
echo "$REGISTER_OUTPUT"

TOKEN_ID="$(echo "$REGISTER_OUTPUT" | grep -i "token id" | grep -oE '[0-9]+' | tail -1 || true)"
[[ -n "$TOKEN_ID" ]] && ok "Platform agent minted as token id $TOKEN_ID" \
                      || echo "    (token id not parsed — check output above)"

# Update platform_agent_token_id if we got it
if [[ -n "$TOKEN_ID" ]]; then
  cd "$REPO_ROOT"
  sed -i '' "s/^platform_agent_token_id.*=.*0/platform_agent_token_id = $TOKEN_ID/" "$CONFIG"
fi

# ---------------------------------------------------------------------------
# 9. Summary
# ---------------------------------------------------------------------------
echo ""
echo "================================================================"
echo " Mantle Sepolia deploy complete"
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
echo " Mantlescan:  https://explorer.sepolia.mantle.xyz/address/$IDENTITY_REGISTRY"
echo ""
echo " Next: commit the config, then set runtime env on the demo host:"
echo "   export MANTLE_TESTNET_IDENTITY_REGISTRY=$IDENTITY_REGISTRY"
echo "   export MANTLE_TESTNET_REPUTATION_REGISTRY=$REPUTATION_REGISTRY"
echo "   export XVN_NETWORK=sepolia"
echo ""
echo " Then run:"
echo "   git add config/mantle-sepolia.toml"
echo "   git commit -m 'chore(contracts): deploy to Mantle Sepolia chain 5003'"
