# Operator Manual

Tasks that cannot be completed from inside a clean repo or a CI pipeline. Each
section describes what to do, what env vars to set, and what success looks like.

## Live-node remote control

To drive a running xvision node over Tailscale:

- Use `scripts/xvn-remote.py exec ...` for backtests and other long-running CLI
  work. It wraps the dashboard's remote CLI job API.
- Use `GET /api/cli/jobs/:id` and `GET /api/cli/jobs/:id/output` to reconnect
  after a disconnect.
- Do not assume general SSH access or an interactive shell on the node.

## Alpaca paper account setup

Required before running paper-mirror eval runs.

1. Sign up at <https://alpaca.markets> and switch to **Paper Trading**.
2. Generate an API key + secret.
3. Store in 1Password under `xvision/alpaca-paper`.
4. Export at runtime:

```bash
export APCA_API_KEY_ID=$(op read 'op://Personal/xvision-alpaca-paper/api_key_id')
export APCA_API_SECRET_KEY=$(op read 'op://Personal/xvision-alpaca-paper/api_secret_key')
export APCA_API_BASE_URL=https://paper-api.alpaca.markets
```

5. Smoke the credentials:

```bash
curl -s \
  -H "APCA-API-KEY-ID: $APCA_API_KEY_ID" \
  -H "APCA-API-SECRET-KEY: $APCA_API_SECRET_KEY" \
  "$APCA_API_BASE_URL/v2/account" | jq '.id, .status, .buying_power'
```

Success: `/v2/account` returns the paper account id with `status: ACTIVE`.

## Orderly testnet onboarding

Required before running Orderly-venue eval runs against testnet.

1. Complete Orderly's brokered onboarding for an EVM (Mantle) wallet via the
   Orderly EVM gateway web flow. This step is manual; there is no `xvn`
   subcommand for brokered onboarding.
2. Save `(orderly_key, orderly_secret, orderly_account_id)` in 1Password under
   `xvision/orderly-testnet`.
3. Export at runtime:

```bash
export ORDERLY_KEY=$(op read 'op://Personal/xvision-orderly-testnet/key')
export ORDERLY_SECRET=$(op read 'op://Personal/xvision-orderly-testnet/secret')
export ORDERLY_ACCOUNT_ID=$(op read 'op://Personal/xvision-orderly-testnet/account_id')
export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org
```

4. Smoke the maintained signed-request path with the CLI executor:

```bash
xvn fire-trade --venue orderly --side buy --size-bps 1 --asset BTC \
  --summary "orderly testnet signed-request smoke"
xvn close-position --venue orderly --asset BTC
```

The smoke submits a tiny testnet Orderly order through the same direct signed
HTTP executor used by runtime code, then closes the position. The legacy
`probes/m0-orderly` SDK probe was removed because `orderly-connector-rs` pins
stale Solana/TLS dependencies that are not used by production execution.

## On-chain identity (opt-in)

`xvision-identity` is excluded from the default workspace build (`default-members`)
to keep the `alloy v2` dependency out of the standard compile path. Only proceed
if you need ERC-8004 per-strategy NFTs on Mantle.

1. Decide: Mantle testnet (chain 5003, `rpc.sepolia.mantle.xyz`) or mainnet
   (chain 5000). Use testnet first.
2. Fund the deployer wallet with testnet MNT from the faucet, or mainnet MNT
   (~$5–20).
3. For each strategy variant, prepare an `identity/<strategy_name>.agent.json`
   manifest with: `agent_id`, `strategy_name`, `code_commit`
   (`git rev-parse HEAD`), `strategy_adapter_type`, `risk_preset`, `contact`.
4. Set `identity.enabled = true` in `config/default.toml`.
5. Build and mint (requires writing `examples/mint_identity.rs` — the workspace
   does not ship this driver):

```bash
export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # testnet
export MANTLE_DEPLOYER_KEY=$(op read 'op://Personal/xvision-mantle/deployer_pk')
for manifest in identity/*.agent.json; do
  cargo run --release -p xvision-identity \
    --example mint_identity -- "$manifest"
done
```

6. Save the resulting `(token_id, contract_addr)` pair into each manifest and
   commit.

Success: each strategy's NFT is minted; `xvn` runs without `Mantle creds missing`
errors when `identity.enabled = true`.

## One-time setup: API keys

### Anthropic

Sign up at <https://console.anthropic.com> and create a key.

```bash
# Store
op item create --vault Personal --title xvision-anthropic api_key=<value>

# Export
export ANTHROPIC_API_KEY=$(op read 'op://Personal/xvision-anthropic/api_key')
```

Cost reference: a Phase 9 backtest (100–300 setups × 1 briefing) is roughly
$1–5 with Haiku and $20–60 with Opus-class reasoning. Prefer Haiku in CI.

### OpenAI-compatible (OpenRouter / Together / Groq)

Any OpenAI-compatible endpoint works. OpenRouter is recommended for multi-model
evaluation.

```bash
export OPENAI_API_KEY=$(op read 'op://Personal/xvision-openai/api_key')
export OPENAI_BASE_URL=https://openrouter.ai/api/v1   # or api.openai.com/v1
```

## One-time setup: local model files

### Qwen3-32B GGUF

Download the Q4_K_M quant for the dev loop and Q8_0 for headline runs:

```bash
cd models
huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q4_K_M.gguf \
  --local-dir qwen3-32b-q4-gguf --local-dir-use-symlinks False
huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q8_0.gguf \
  --local-dir qwen3-32b-q8-gguf --local-dir-use-symlinks False
```

Disk: Q4 ~17 GB, Q8 ~32 GB.

Verify the model loads and emits a `TraderDecision`:

```bash
cargo run --release -p xvision-trader --bin smoke-trader
```

### tokenizer.json

Download separately (the GGUF does not bundle the tokenizer):

```bash
huggingface-cli download Qwen/Qwen3-32B tokenizer.json \
  --local-dir models/qwen3-32b-q4-gguf --local-dir-use-symlinks False
cp models/qwen3-32b-q4-gguf/tokenizer.json models/qwen3-32b-q8-gguf/
```

### MCP indicator server (only when driving xvn from an external MCP client)

Build the stdio MCP binary:

```bash
cargo build --release -p xvision-mcp
```

Register in `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "xvn": { "command": "/path/to/target/release/xvn-mcp" }
  }
}
```

Smoke: call the `xvn_health` tool. Expected: `{"ok": true, "name": "xvision-mcp", "version": "<x.y.z>"}`.

## Remote GPU box env-var checklist

`scripts/setup_runpod.sh` persists most of these to `$REPO_ROOT/.env.local`;
run `source .env.local` before `xvn`.

```bash
# Hugging Face
export HF_TOKEN=...
export HUGGING_FACE_HUB_TOKEN="$HF_TOKEN"

# Default LLM provider (Intern stage retired, now single-stage agent)
# Use config/default.toml [[providers]] array instead
export XVN_DEFAULT_PROVIDER=anthropic          # | openai-compat
export XVN_DEFAULT_BASE_URL=https://api.anthropic.com
export XVN_DEFAULT_MODEL=claude-haiku-4-5
export ANTHROPIC_API_KEY=...                  # or OPENAI_API_KEY

# Stage 2 local Trader (GGUF)
export XVN_MODEL_KIND=gguf
export XVN_MODEL_PATH=$PWD/models/qwen3-32b-q8-gguf/Qwen_Qwen3-32B-Q8_0.gguf
export XVN_TOKENIZER=$PWD/models/qwen3-32b-q8-gguf/tokenizer.json

# Alpaca paper
export APCA_API_KEY_ID=...
export APCA_API_SECRET_KEY=...
export APCA_API_BASE_URL=https://paper-api.alpaca.markets

# Orderly testnet
export ORDERLY_KEY=...
export ORDERLY_SECRET=...
export ORDERLY_ACCOUNT_ID=...
export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org

# Mantle (only if identity.enabled = true)
export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # testnet, chain 5003
export MANTLE_DEPLOYER_KEY=...
```

Pull secrets with `op read 'op://...'` rather than pasting them inline. Use
`direnv` locally for auto-loading, or `source .env.local` per shell.

## Scale breakpoints

xvision v1 is designed for a single operator. Key breakpoints to plan around:

**N = 1 (current):** single `CREDENTIAL_SECRET` env var, SQLite store, manual
`xvn eod` review. Acceptable for single-operator use.

**N = 10:** the single env-var-derived secret encrypts every user's trading key —
one compromise loses all keys. Migrate to per-user HKDF-derived keys
(`TradingKeyStore` already implements this). OFAC screening becomes load-bearing
for the hosting entity once marketplace fees flow from multiple EVM addresses.

**N = 100:** SQLite write throughput becomes a bottleneck; evaluate Postgres.
LLM briefing costs scale to ~$15K/month at Sonnet class — plan subscription
tiers or budget accordingly.

**N = 1000:** Postgres required, 24/7 on-call, per-tenant isolation. Effectively
a v3 architecture.

## Incident response

### 1. Contain (target: under 5 minutes)

- [ ] Disable the affected scheduler or dashboard-triggered run source.
- [ ] If exposure is open, inspect with `xvn portfolio --venue <venue>` and
      close with `xvn close-position --venue <venue> --asset <asset>`. Default
      to closing when wrong-direction exposure is suspected.
- [ ] Post a one-line status to your status channel immediately.

### 2. Diagnose (target: under 30 minutes)

- [ ] Review recent eval history: `xvn eval list` and `xvn eval get <run_id>`.
- [ ] Cross-check venue state: `xvn portfolio --venue <venue>`.
- [ ] Classify the issue: strategy bug, risk engine miss, execution glitch,
      broker outage, or operator error.

### 3. Communicate (target: within 60 minutes of detection)

- [ ] Update your status channel with findings.
- [ ] If user funds were at risk, a public summary within 7 days of containment.

### 4. Post-mortem (within 7 days)

- [ ] Write up: timeline, root cause, what worked, what didn't, what changes.
- [ ] If a safety check is missing, add a task to the relevant plan.
- [ ] If a runbook gap is revealed, update this page.

## See also

- [CLI Reference](/docs?slug=cli-reference) — full `xvn` command surface.
- [Operator Runbook](/docs?slug=runbook) — dashboard auth and observability setup.
- [Why some commands aren't in xvn](/docs?slug=cli-non-surfaced) — deliberate CLI exclusions.
