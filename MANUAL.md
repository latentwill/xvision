# Manual operator tasks

> **2026-05-07 status (ADR 0011):** CV vector-extraction operator tasks
> (M1–M4) have been removed. They moved to xianvec-play with the rest of
> the CV substrate. The surviving tasks below are Tier 2 (forward-paper
> + on-chain identity) and Tier 3 (one-time setup) only.

Things that **cannot** be done from inside Claude Code / cargo / a clean repo.
Each entry: trigger, what's needed, exit criterion, FOLLOWUPS cross-ref.

Sorted by which milestone they unblock. Keep this file in sync with
`FOLLOWUPS.md` — that file tracks engineering follow-ups; this one tracks
operator-side prerequisites.

---

## Tier 2 — blocking forward-paper / on-chain (Phase 11)

### M5. Set up Alpaca paper account + creds (F5 alpha)

- **Trigger:** ready to start Phase 11.1.
- **What:**
  1. Sign up at <https://alpaca.markets>; switch to Paper Trading.
  2. Generate API key + secret.
  3. Store in 1Password under entry `xianvec/alpaca-paper`.
  4. Export at runtime:
     ```bash
     export APCA_API_KEY_ID=$(op read 'op://Personal/xianvec-alpaca-paper/api_key_id')
     export APCA_API_SECRET_KEY=$(op read 'op://Personal/xianvec-alpaca-paper/api_secret_key')
     export APCA_API_BASE_URL=https://paper-api.alpaca.markets
     ```
  5. Smoke the credentials with a read-only `/v2/account` round-trip
     (no submit-flow on `xvn` ships yet — `xvn run-setup` is the
     Intern → Risk slice; execution is added in Phase 11.1):
     ```bash
     curl -s \
       -H "APCA-API-KEY-ID: $APCA_API_KEY_ID" \
       -H "APCA-API-SECRET-KEY: $APCA_API_SECRET_KEY" \
       "$APCA_API_BASE_URL/v2/account" | jq '.id, .status, .buying_power'
     ```
- **Exit:** `/v2/account` returns the paper account id + `status: ACTIVE`.
- **Unblocks:** Phase 11.1.

### M6. Onboard to Orderly testnet + smoke trade (F5)

- **Trigger:** Phase 11.5 prep.
- **What:**
  1. Complete Orderly's brokered onboarding for an EVM (Mantle) wallet via
     the Orderly EVM gateway (web flow). The `xvn setup --orderly-onboard`
     subcommand specced in implementation-plan §6.3 is **not yet shipped**;
     onboarding is currently manual.
  2. Save `(orderly_key, orderly_secret, orderly_account_id)` in 1Password
     under `xianvec/orderly-testnet`.
  3. Export at runtime:
     ```bash
     export ORDERLY_KEY=$(op read 'op://Personal/xianvec-orderly-testnet/key')
     export ORDERLY_SECRET=$(op read 'op://Personal/xianvec-orderly-testnet/secret')
     export ORDERLY_ACCOUNT_ID=$(op read 'op://Personal/xianvec-orderly-testnet/account_id')
     export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org
     ```
  4. Smoke against testnet via the existing M0 probe — it exercises the
     full signed-request path used by `xianvec-execution`:
     ```bash
     cargo run --release --manifest-path probes/m0-orderly/Cargo.toml
     ```
     The probe places + cancels a tiny `PERP_BTC_USDC` order and verifies
     SDK errors map to `ExecutorError`. (When `xvn run-setup` grows
     `--executor orderly`, this step migrates to the CLI.)
- **Exit:** the M0 probe completes a round-trip submit+cancel against
  Orderly testnet without errors.
- **Unblocks:** Phase 11.5.
- **FOLLOWUPS:** F5.

### M7. Mint ERC-8004 per-strategy identity NFTs on Mantle (SLF3)

- **Trigger:** Phase 11.5 prep, after M6.
- **What:**
  1. Decide whether to use Mantle testnet (Sepolia L2 testnet) or mainnet.
     Mint on testnet first; mainnet only after Phase 9 eval clears.
  2. Fund the deployer wallet with testnet MNT (faucet) or mainnet MNT
     (~$5–20 worth).
  3. For each strategy variant in the active loom set, prepare an
     `identity/<strategy_name>.agent.json` manifest:
     - `agent_id`: assigned at mint time
     - `strategy_name`: human-readable label (e.g., `trader_arm`, `buy_hold`)
     - `code_commit`: `git rev-parse HEAD` at the time of the run
     - `strategy_adapter_type`: identifier for the Strategy impl
     - `risk_preset`: matches `config/risk.toml`
     - `contact`: email or GitHub URL
  4. Set `identity.enabled = true` in `config/default.toml` (or per-env override).
  5. Mint. **`xianvec-identity` ships as a library only today** — no
     `mint-identity` binary. Until one lands, write a thin driver against
     `crates/xianvec-identity/src/client.rs`:
     - `RegistryAddresses::custom(identity, reputation)` — pass the
       deployed-on-Mantle contract addresses.
     - `IdentityClient::connect(rpc_url, addrs, chain_id).await?`
     - `client.register(&agent_uri, &signer).await?` returns a `TokenId`.
     Mantle testnet is `chain_id = 5003`; mainnet is `5000`.
     Then:
     ```bash
     export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # testnet
     export MANTLE_DEPLOYER_KEY=$(op read 'op://Personal/xianvec-mantle/deployer_pk')
     for manifest in identity/*.agent.json; do
       cargo run --release -p xianvec-identity \
         --example mint_identity -- "$manifest"
     done
     ```
     (`examples/mint_identity.rs` is the driver you write; the workspace
     doesn't ship it yet.)
  6. Save the resulting (token_id, contract_addr) pair into each manifest
     and commit.
- **Exit:** each strategy's NFT minted on the chosen network; per-strategy
  manifests have populated identity fields; `xvn` runs without `Mantle creds
  missing` errors when `identity.enabled = true`.
- **Unblocks:** Phase 11.5.
- **FOLLOWUPS:** SLF3. **xianvec-identity is opt-in** — keep it excluded from
  `default-members` in `Cargo.toml`; explicit `cargo build -p xianvec-identity`
  to compile.

---

## Tier 3 — secrets, models, datasets (one-time setup)

### M8. Anthropic API key (or alternative)

- **What:** sign up at <https://console.anthropic.com>; create a key.
- **Save:** `op://Personal/xianvec-anthropic/api_key`.
- **Export:**
  ```bash
  export ANTHROPIC_API_KEY=$(op read 'op://Personal/xianvec-anthropic/api_key')
  ```
- **Cost rough estimate:** Phase 9 backtest = 100–300 setups × 1 brief ≈
  $1–5 with Haiku; with Opus reasoning, $20–60. Prefer Haiku in CI.

### M9. OpenAI / OpenRouter / Together / Groq key (optional)

- **What:** any OpenAI-compat endpoint works. OpenRouter recommended for
  multi-model evaluation.
- **Save:** `op://Personal/xianvec-openai/api_key`.
- **Export:**
  ```bash
  export OPENAI_API_KEY=$(op read 'op://Personal/xianvec-openai/api_key')
  export OPENAI_BASE_URL=https://openrouter.ai/api/v1   # or stay on api.openai.com/v1
  ```

### M10. Download Qwen3-32B GGUF locally

- **What:** download the Q4_K_M GGUF for the dev loop and Q8_0 for the headline.
- **Suggested:**
  ```bash
  cd models
  huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q4_K_M.gguf \
    --local-dir qwen3-32b-q4-gguf --local-dir-use-symlinks False
  huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q8_0.gguf \
    --local-dir qwen3-32b-q8-gguf --local-dir-use-symlinks False
  ```
- **Disk:** Q4 ≈ 17 GB, Q8 ≈ 32 GB.
- **Verify:** `cargo run --release -p xianvec-trader --bin smoke-trader` loads
  the model and emits a `TraderDecision` JSON.

### M11. Download tokenizer.json

- **What:** the Qwen3-32B `tokenizer.json` (separate from the GGUF;
  identical content for Q4 and Q8 — copy/symlink it into whichever quant
  dirs you've downloaded).
- **Suggested:**
  ```bash
  huggingface-cli download Qwen/Qwen3-32B tokenizer.json \
    --local-dir models/qwen3-32b-q4-gguf --local-dir-use-symlinks False
  cp models/qwen3-32b-q4-gguf/tokenizer.json models/qwen3-32b-q8-gguf/
  ```
  (M4's headline run reads from the Q8 dir.)

### M11.5. Wire the MCP indicator server (only when `INTERN=acpx`)

- **Trigger:** running the Stage 1 Intern through the ACPX agent harness
  (`XVN_INTERN_PROVIDER=acpx`) and you want the agent to recompute
  indicators at parameter sets the snapshot doesn't pre-bake (e.g. RSI(7)
  when the snapshot only carries RSI(14)). Skip otherwise — the MCP server
  is irrelevant to the OpenAI-compat / Anthropic Intern paths.
- **What:** `crates/xianvec-mcp/` builds a stateless stdio MCP server,
  `xvn-mcp`, that exposes `xianvec-data`'s indicator surface (rsi · sma ·
  ema · bollinger · atr · macd · donchian · fib_retracements · health) as
  agent-callable tools. ACPX advertises it to every agent session via
  `mcpServers: [...]` in `acpx.config.json`.
- **Setup steps** (auto-run by `scripts/setup_runpod.sh` when
  `INTERN=acpx`):
  1. `cargo build --release -p xianvec-mcp` (produces `target/release/xvn-mcp`).
  2. Write `<acpx-workspace>/acpx.config.json` registering the binary as a
     stdio MCP server. The setup script does this for you; otherwise
     install ACPX (`npm install -g acpx@latest`) and add the stanza by
     hand. Each ACPX-driven agent (claude / codex / openclaw / hermes /
     etc.) has its own install + auth flow on top — see the relevant agent
     CLI for those.
- **Verify:** from inside the chosen ACPX agent session, ask it to call
  the `xvn_health` tool. Expected response: `{"ok": true, "name":
  "xianvec-mcp", "version": "<x.y.z>"}`. Any other indicator (`xvn_rsi` on
  a small synthetic price series) is a fine smoke too.
- **Exit:** `xvn_health` returns `ok: true` from the agent's tool channel.
- **Unblocks:** F21 (ACPX-driven Intern), and any future agent-harness
  caller that needs the indicator surface (F23 pluggable Trader).
- **Caveat:** live-data tools (funding rates, onchain panel reads) are not
  in this MCP yet — the agent must supply input series from prompt
  context. Determinism for backtest stays via that constraint.

### M12. Source paired setups + bars JSON for the backtest

- **What:** the `xvn ab-compare` runner needs:
  - `data/setups/<n>.json` — `Vec<MarketSnapshot>` covering 2022–2024 paired
    setups on BTC-USD (≥100 setups for the headline N).
  - `data/bars/btc_2022_2024.json` — `Vec<MarketBar>` (OHLCV) covering the
    span and granularity that the setups reference.
- **Sourcing options:**
  - Binance public data → polars Parquet → JSON via the existing
    `xianvec-data` pipeline.
  - Coinbase pro CSV → same.
  - The repo's `data/baselines/` may already have a starter dataset; check
    `data/` before sourcing fresh.
- **Setup-id assignment:** each `MarketSnapshot.setup_id` is a `Uuid::new_v4()`
  generated at dataset-build time and persisted alongside the row so re-runs
  pair correctly (Tier 1 fix #1).

---

## Tier 4 — non-blocking research / upstream

### M13. Open the upstream PR against ranger-finance/orderly-connector-rs (F20)

- **What:** ~30–50 LoC PR adding `[features] default = ["solana", "evm"]`,
  making `solana-sdk`/`solana-client`/`solana_vault_cpi` + `ed25519-dalek 1.x`
  optional behind `feature = "solana"`, switching the `evm` feature to
  `ed25519-dalek 2.x`, dropping the `zeroize = "=1.3.0"` exact pin.
- **Workflow:**
  1. Fork `https://github.com/ranger-finance/orderly-connector-rs`.
  2. Branch + apply the diff per FOLLOWUPS F20 scope.
  3. Run their existing tests under both `--features solana` and
     `--features evm`.
  4. Open PR; cite the workspace-side pin conflict (rustls 0.23 / reqwest 0.13
     wants `zeroize ≥ 1.7`) as motivation.
- **Exit:** PR merged + new release published. Then F19 collapses to a
  5-line workspace change.
- **FOLLOWUPS:** F20 (and its downstream, F19).

### M14. Curate `data/probes/` corpus (F13 / Phase 8.5)

- **What:** ~30–60 hand-picked historical market setups: ambiguous regime
  transitions, low-liquidity sessions, hardest historical decisions, flash-crash
  conditions, regulatory edge cases.
- **Workflow:**
  1. Pull candidate setups from a 4-year BTC history (2021–2024).
  2. Hand-tag each as one of the 5 buckets above.
  3. Save under `data/probes/<bucket>/<uuid>.json` as `MarketSnapshot`.
  4. Wire `ProbeRunner` in `xianvec-eval` per implementation-plan §8.5.
- **Trigger:** Phase 9.2 A/B runner stable + want a regression-detection net
  for strategy / prompt / model changes.
- **FOLLOWUPS:** F13.

### M15. Source onchain baselines data (F14 / Phase 7.5)

- **What:** Nansen smart-money copy-trader, funding-rate fader, stablecoin
  exchange-inflow risk-off, liquidation cascade fader. Each consumes
  `OnchainPanel` fields already on `MarketSnapshot`.
- **What's needed:** Nansen API access (paid tier), or DefiLlama-like
  aggregator credentials, or scraped public data.
- **Trigger:** post-headline result if onchain comparison is needed for the
  demo narrative.
- **FOLLOWUPS:** F14.

### M16. Bench rig for `target-cpu=native` measurement (F9)

- **What:** controlled thermal state + ≥10 trials per condition.
- **Why manual:** thermal throttling on Apple Silicon swings results 3.2×
  across 5 runs; need to actually pin CPU governor / let the box cool /
  re-measure.
- **Workflow:**
  1. Cold start; close all non-test apps.
  2. Run `cargo run --release -p xianvec-inference --bin smoke-qwen3` 10×
     with default `RUSTFLAGS`.
  3. Cool box; repeat 10× with `RUSTFLAGS="-C target-cpu=native"`.
  4. Compare median + p95 decode/prefill tok/s.
- **Exit:** if win is ≥1.5× and stable, codify in `.cargo/config.toml` (F10).
- **FOLLOWUPS:** F9, F10.

---

## Strategy authoring (MVP — see crates/xianvec-engine/README.md)

```bash
xvn strategy templates                 # list templates
xvn strategy new --template <t> --name <n>
xvn strategy validate <id>
xvn strategy show <id>
xvn strategy ls
xvn strategy run <id> --fixture <name> --decisions <N> [--mock]
```

End-to-end paths beyond this surface (web Wizard, marketplace publishing, live trading,
batch eval) land in subsequent plans (#2, #3) — they share this same bundle format.

---

## Quick env-var checklist (remote GPU box)

These exports belong on the remote server (RunPod / Vast.ai), not the
local dev machine. `scripts/setup_runpod.sh` persists most of them to
`$REPO_ROOT/.env.local` — `source .env.local` before running `xvn`.

```bash
# Hugging Face — required by setup_runpod.sh preflight + M2 model download
export HF_TOKEN=...                           # M1, M10
export HUGGING_FACE_HUB_TOKEN="$HF_TOKEN"     # huggingface-cli also reads this

# Stage 1 Intern — pick one provider and set the matching key
# Persisted by setup_runpod.sh based on the INTERN= choice:
export XVN_INTERN_PROVIDER=anthropic          # | openai-compat | acpx
export XVN_INTERN_BASE_URL=https://api.anthropic.com
export XVN_INTERN_MODEL=claude-haiku-4-5
export XVN_INTERN_KEY_ENV=ANTHROPIC_API_KEY   # name of the var that holds the key
export ANTHROPIC_API_KEY=...                  # M8 (or OPENAI_API_KEY etc. — M9)
# ACPX path only (XVN_INTERN_PROVIDER=acpx):
export XVN_INTERN_ACPX_AGENT=claude           # | codex | openclaw | hermes | ...
# export XVN_INTERN_ACPX_CUSTOM_CMD="hermes acp"   # escape hatch for Hermes
# export XVN_INTERN_ACPX_BIN=acpx                  # override binary name
# export XVN_INTERN_ACPX_TIMEOUT_SECS=300          # default 300s
# export XVN_INTERN_ACPX_MAX_OUTPUT_BYTES=2097152  # default 2 MiB

# Local Trader (Stage 2) — persisted by setup_runpod.sh from the model menu
export XVN_MODEL_KIND=gguf                    # | fp16
export XVN_MODEL_PATH=$PWD/models/qwen3-32b-q8-gguf/Qwen_Qwen3-32B-Q8_0.gguf
export XVN_TOKENIZER=$PWD/models/qwen3-32b-q8-gguf/tokenizer.json
# (XVN_MODEL_KIND=fp16 uses XVN_MODEL_DIR instead of XVN_MODEL_PATH/_TOKENIZER.)

# Phase 11.1 Alpaca paper
export APCA_API_KEY_ID=...                    # M5
export APCA_API_SECRET_KEY=...                # M5
export APCA_API_BASE_URL=https://paper-api.alpaca.markets

# Phase 11.5 Orderly testnet
export ORDERLY_KEY=...                        # M6
export ORDERLY_SECRET=...                     # M6
export ORDERLY_ACCOUNT_ID=...                 # M6
export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org

# Phase 11.5 Mantle (only if identity.enabled = true)
export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # M7 (testnet, chain 5003)
export MANTLE_DEPLOYER_KEY=...                # M7

# Throughput (advisory; codify in .cargo/config.toml when F9 confirms a stable win)
# export RUSTFLAGS="-C target-cpu=native"     # F9 / F10
```

Pull values from 1Password with `op read 'op://...'` rather than pasting
secrets inline. The setup script writes `$REPO_ROOT/.env.local`
(gitignored); use `direnv` locally if you want auto-loading, otherwise
`source .env.local` per shell.

---

*Last updated: 2026-05-04. Cross-references: `FOLLOWUPS.md`,
`implementation-plan.md` Phases 9–12.*
