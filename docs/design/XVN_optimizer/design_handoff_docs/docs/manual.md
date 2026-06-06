# xvn operator manual

> Things that **cannot** be done from inside Claude Code, a clean repo, or `cargo`.

**Tiers** · 4 · **Milestones** · 16 · **Source** · `MANUAL.md` · **Updated** 2026-05-20.

Tiered by which milestone they unblock. Each entry: trigger, what, exit criterion, follow-up cross-ref. Keep this file in sync with `FOLLOWUPS.md` — that file tracks engineering follow-ups; this one tracks operator-side prerequisites.

## Context for AI agents

- **route**: `/docs/manual`
- **summary**: Operator-side prerequisites that block specific milestones. CV vector-extraction tasks (M1–M4) moved to xvision-play with ADR 0011. Surviving tiers: T2 (forward-paper + on-chain), T3 (one-time secrets/models/datasets), T4 (non-blocking research/upstream).
- **key terms**: Alpaca paper · Orderly testnet · ERC-8004 mint · 1Password CLI · Tailscale · `XVN_DASHBOARD_TOKEN` · danger-op typed phrases · OTel / Langfuse
- **do not**: skip the M0 probe before going live · commit secrets · hardcode keys · bind dashboard to `0.0.0.0` without `XVN_DASHBOARD_TOKEN` · run `xvision-identity` on mainnet before clearing Phase 9 eval

## Live-node remote control

If you need to drive a running xvn node over Tailscale, use the dashboard's remote CLI job API or `scripts/xvn-remote.py`.

- Prefer `scripts/xvn-remote.py exec …` for backtests and other long-running typed CLI work.
- Do **not** assume arbitrary SSH access or a general-purpose shell on the node.
- Use `GET /api/cli/jobs/:id` and `GET /api/cli/jobs/:id/output` to reconnect after disconnects.

## Scenario backtest workflow

Backtest means historical Alpaca bars plus simulated execution. It does **not** send orders to Alpaca paper.

1. Create a scenario in `/scenarios/new` or with `xvn scenario create`.
2. Confirm the bar-cache badge reads **Fully cached**, or click **Fetch bars**.
3. Launch a run from `/eval-runs` with mode **Backtest**.
4. Open the run-detail chart and inspect candles, decisions, equity, drawdown, markers.

Paper mirror means the existing Alpaca paper route. It places paper orders with the configured paper account.

## Tier 2 · blocking forward-paper / on-chain

### M5 · Alpaca paper account & creds

- **Trigger**: Ready to start Phase 11.1.
- **Exit**: `/v2/account` returns the paper account id + `status: ACTIVE`.
- **Unblocks**: Phase 11.1 (paper executor).

1. Sign up at `https://alpaca.markets`; switch to Paper Trading.
2. Generate API key + secret.
3. Store in 1Password under `xvision/alpaca-paper`.

```bash
export APCA_API_KEY_ID=$(op read 'op://Personal/xvision-alpaca-paper/api_key_id')
export APCA_API_SECRET_KEY=$(op read 'op://Personal/xvision-alpaca-paper/api_secret_key')
export APCA_API_BASE_URL=https://paper-api.alpaca.markets

curl -s \
  -H "APCA-API-KEY-ID: $APCA_API_KEY_ID" \
  -H "APCA-API-SECRET-KEY: $APCA_API_SECRET_KEY" \
  "$APCA_API_BASE_URL/v2/account" | jq '.id, .status, .buying_power'
```

### M6 · Orderly testnet onboarding + smoke trade

- **Trigger**: Phase 11.5 prep.
- **Exit**: M0 probe completes submit + cancel against Orderly testnet without errors.
- **Unblocks**: Phase 11.5.
- **Follow-ups**: F5.

1. Complete Orderly's brokered onboarding for an EVM (Mantle) wallet via the Orderly EVM gateway (web flow). Onboarding is manual; the shipped CLI does not expose a brokered onboarding subcommand.
2. Save `(orderly_key, orderly_secret, orderly_account_id)` in 1Password under `xvision/orderly-testnet`.
3. Smoke against testnet via the M0 probe.

```bash
export ORDERLY_KEY=$(op read 'op://Personal/xvision-orderly-testnet/key')
export ORDERLY_SECRET=$(op read 'op://Personal/xvision-orderly-testnet/secret')
export ORDERLY_ACCOUNT_ID=$(op read 'op://Personal/xvision-orderly-testnet/account_id')
export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org

cargo run --release --manifest-path probes/m0-orderly/Cargo.toml
```

### M7 · Mint ERC-8004 per-strategy identity NFTs on Mantle

- **Trigger**: Phase 11.5 prep, after M6.
- **Exit**: each strategy's NFT minted; manifests carry populated identity fields; `xvn` runs without "Mantle creds missing" errors when `identity.enabled = true`.
- **Networks**: testnet `chain_id = 5003` · mainnet `5000`.
- **Follow-ups**: SLF3.

`xvision-identity` is **opt-in**. Keep it excluded from `default-members` in `Cargo.toml`; build explicitly with `cargo build -p xvision-identity` only when wiring identity.

```bash
export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz
export MANTLE_DEPLOYER_KEY=$(op read 'op://Personal/xvision-mantle/deployer_pk')

for manifest in identity/*.agent.json; do
  cargo run --release -p xvision-identity \
    --example mint_identity -- "$manifest"
done
```

## Tier 3 · secrets, models, datasets

### M8 · Anthropic API key

Sign up at `https://console.anthropic.com`; create a key. Save under `op://Personal/xvision-anthropic/api_key`. Rough cost: Phase 9 backtest is 100–300 setups × 1 brief ≈ $1–5 with Haiku, $20–60 with Opus reasoning. Prefer Haiku in CI.

### M9 · OpenAI / OpenRouter / Together / Groq

Any OpenAI-compatible endpoint. OpenRouter is recommended for multi-model evaluation. Set `OPENAI_BASE_URL=https://openrouter.ai/api/v1` or stay on `api.openai.com/v1`.

### M10 · Qwen3-32B GGUF

Download Q4_K_M for the dev loop and Q8_0 for the headline. Disk: Q4 ≈ 17 GB, Q8 ≈ 32 GB. Verify with `cargo run --release -p xvision-trader --bin smoke-trader`.

```bash
cd models
huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q4_K_M.gguf \
  --local-dir qwen3-32b-q4-gguf --local-dir-use-symlinks False
huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q8_0.gguf \
  --local-dir qwen3-32b-q8-gguf --local-dir-use-symlinks False
```

### M11 · tokenizer.json

Separate from the GGUF; identical content for Q4 and Q8. Copy or symlink into whichever quant dirs you've downloaded.

### M11.5 · MCP indicator server (when `INTERN=acpx`)

`crates/xvision-mcp/` builds a stateless stdio MCP server, `xvn-mcp`, that exposes `xvision-data`'s indicator surface (rsi · sma · ema · bollinger · atr · macd · donchian · fib_retracements · health) as agent-callable tools. Auto-run by `scripts/setup_runpod.sh` when `INTERN=acpx`. Verify by asking the agent to call `xvn_health`.

### M12 · Paired setups + bars JSON

The `xvn ab-compare` runner needs `data/setups/<n>.json` (`Vec<MarketSnapshot>`, ≥100 setups on BTC-USD 2022–2024) and `data/bars/btc_2022_2024.json` (`Vec<MarketBar>` covering the same span/granularity). Each `MarketSnapshot.setup_id` is a `Uuid::new_v4()` generated at dataset-build time.

## Tier 4 · non-blocking research / upstream

### M13 · Upstream PR to `ranger-finance/orderly-connector-rs`

~30–50 LoC PR adding `[features] default = ["solana", "evm"]`, making `solana-sdk` / `solana-client` / `solana_vault_cpi` + `ed25519-dalek 1.x` optional behind `feature = "solana"`, switching `evm` to `ed25519-dalek 2.x`, dropping the `zeroize = "=1.3.0"` exact pin. Follow-up F20.

### M14 · Curate `data/probes/` corpus

~30–60 hand-picked historical setups across five buckets: ambiguous regime transitions, low-liquidity sessions, hardest historical decisions, flash-crash conditions, regulatory edge cases. Save under `data/probes/<bucket>/<uuid>.json`.

### M15 · Onchain baselines data

Nansen smart-money copy-trader, funding-rate fader, stablecoin exchange-inflow risk-off, liquidation cascade fader. Each consumes `OnchainPanel` fields already on `MarketSnapshot`. Requires Nansen API access or a DefiLlama-like aggregator.

### M16 · Bench rig for `target-cpu=native`

Controlled thermal state, ≥10 trials per condition. Thermal throttling on Apple Silicon swings results 3.2×; bench has to be manual until a known-stable rig is available.

## Runbook · dashboard auth gate

The dashboard inspects its bind address at startup:

| Bind | Auth posture |
|---|---|
| `127.0.0.1:<port>` or `::1` | Loopback-only. No token required. |
| `0.0.0.0:<port>`, `::`, public IPs | Non-loopback. `XVN_DASHBOARD_TOKEN` **must** be set; otherwise the process refuses to start. |

Loopback connections from the local machine bypass the gate even on a non-loopback bind — so SSH tunneling stays frictionless.

```bash
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
xvn dashboard serve --bind 0.0.0.0:8788
```

### Token presentation · four channels

Constant-time compared, equivalent priority — first match wins.

1. **Authorization header** · `Authorization: Bearer <token>`
2. **Dedicated header** · `X-Xvision-Token: <token>`
3. **Bootstrap cookie** · `xvn_dashboard_token=<token>`, scoped to `/`, `HttpOnly`, `SameSite=Lax`. Set automatically after a valid header or query-token request.
4. **Query parameter** · `?token=<token>` URL-encoded. Useful for SSE and download links.

### Failure response

```json
{ "code": "unauthorized", "message": "missing or invalid dashboard auth token" }
```

### Danger-op typed phrases

| Route | Required phrase |
|---|---|
| `/api/settings/danger/wipe-db` | `wipe my database` |
| `/api/settings/danger/factory-reset` | `reset everything` |
| `/api/settings/danger/regen-identity` | `regenerate identity` |

## Runbook · observability (OpenTelemetry)

v1 ships SQLite flight-recorder + `tracing` console only. Full OTel + Langfuse is deferred to v2 but the wire format is in place.

| Field | Value |
|---|---|
| Backend | Self-hosted Langfuse (Docker compose: Postgres + Clickhouse) |
| Export | `tracing-opentelemetry` → `opentelemetry-otlp` |
| Semantic conv. | OpenTelemetry GenAI |
| Dual-write | SQLite (replay) + OTLP (live) |
| Local-only mode | Default · `RUST_LOG=info` + flight recorder |

## Docker reference

```bash
docker pull ghcr.io/latentwill/xvision:latest
docker run --rm \
  -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  --env-file .env \
  ghcr.io/latentwill/xvision:latest \
  doctor
```

The published `:latest` defaults to running the dashboard on `:8788`. See `docker/README.md` for the full mount/env-var reference. Build the image on a build/control host — never on a server/deploy host.

---

Reconciled with `MANUAL.md` + `docs/runbook/dashboard-auth.md` at commit `a73b18f` on 2026-05-20.
