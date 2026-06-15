# Manual operator tasks

> **2026-05-07 status (ADR 0011):** CV vector-extraction operator tasks
> (M1–M4) have been removed. They moved to xvision-play with the rest of
> the CV substrate. The surviving tasks below are Tier 2 (forward-paper
> + on-chain identity) and Tier 3 (one-time setup) only.

Things that **cannot** be done from inside Claude Code / cargo / a clean repo.
Each entry: trigger, what's needed, exit criterion, FOLLOWUPS cross-ref.

Sorted by which milestone they unblock. Keep this file in sync with
`FOLLOWUPS.md` — that file tracks engineering follow-ups; this one tracks
operator-side prerequisites.

Cross-references for operator-facing concepts that have their own docs:

- **Agent memory** — see `docs/v2d-memory-overview.md` for how the
  per-slot memory toggle works and why backtest replays don't leak
  future knowledge.
- **Inline filter DSL** — see `docs/operator/filter-dsl-catalog.md` for
  the exact indicators, operators, and JSON examples accepted by
  `xvn strategy set-filter`.

---

## Optimizer window cap — migration (B22)

PR B22 added a hard 120-day cap (`MAX_WINDOW_DAYS = 120`) on the
`day_window` and `baseline_untouched_window` fields in
`autooptimizer.toml`. This was a **breaking change**: any existing config
with either field set above 120 now fails `xvn optimize` at startup with a
field-level validation error.

**To fix,** choose one:

- **Shrink the window** to 120 days or below.
- **Set `max_window_days`** to your intended limit (must be >= 1) to
  explicitly acknowledge the memory tradeoff:

  ```toml
  max_window_days = 180   # opt-in to wider window

  day_window = 180
  baseline_untouched_window = 60
  ```

`max_window_days` only covers `day_window` and `baseline_untouched_window`;
`regime_set` and `scenario_pool` windows remain capped at 120. See the
full config reference in the dashboard wiki at `/docs?slug=autooptimizer-config`.

---

## Live-node remote control

If you need to drive a running xvision node over Tailscale, use the dashboard's
remote CLI job API or `scripts/xvn-remote.py`.

- Prefer `scripts/xvn-remote.py exec ...` for backtests and other long-running
  typed CLI work.
- Do not assume arbitrary SSH access or a general-purpose shell on the node.
- Use `GET /api/cli/jobs/:id` and `GET /api/cli/jobs/:id/output` to reconnect
  after disconnects.

---

## Scenario Backtest Workflow

Backtest means historical Alpaca bars plus simulated execution. It does not
send orders to Alpaca paper.

1. Create a scenario in `/scenarios/new` or with `xvn scenario create`.
2. Confirm the bar cache badge is `Fully cached`, or click `Fetch bars`.
3. Launch a run from `/eval-runs` with mode `Backtest`.
4. Open the run detail chart and inspect candles, decisions, equity, drawdown,
   and markers.

Paper mirror means the existing Alpaca paper route. It can place paper orders
with the configured Alpaca paper account.

---

## Tier 2 — blocking forward-paper / on-chain (Phase 11)

### M5. Set up Alpaca paper account + creds (F5 alpha)

- **Trigger:** ready to start Phase 11.1.
- **What:**
  1. Sign up at <https://alpaca.markets>; switch to Paper Trading.
  2. Generate API key + secret.
  3. Store in 1Password under entry `xvision/alpaca-paper`.
  4. Export at runtime:
     ```bash
     export APCA_API_KEY_ID=$(op read 'op://Personal/xvision-alpaca-paper/api_key_id')
     export APCA_API_SECRET_KEY=$(op read 'op://Personal/xvision-alpaca-paper/api_secret_key')
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
     the Orderly EVM gateway (web flow). Onboarding is currently manual; the
     shipped CLI does not expose a brokered onboarding subcommand.
  2. Save `(orderly_key, orderly_secret, orderly_account_id)` in 1Password
     under `xvision/orderly-testnet`.
  3. Export at runtime:
     ```bash
     export ORDERLY_KEY=$(op read 'op://Personal/xvision-orderly-testnet/key')
     export ORDERLY_SECRET=$(op read 'op://Personal/xvision-orderly-testnet/secret')
     export ORDERLY_ACCOUNT_ID=$(op read 'op://Personal/xvision-orderly-testnet/account_id')
     export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org
     ```
  4. Smoke against testnet via the existing M0 probe — it exercises the
     full signed-request path used by `xvision-execution`:
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
  5. Mint. **`xvision-identity` ships as a library only today** — no
     `mint-identity` binary. Until one lands, write a thin driver against
     `crates/xvision-identity/src/client.rs`:
     - `RegistryAddresses::custom(identity, reputation)` — pass the
       deployed-on-Mantle contract addresses.
     - `IdentityClient::connect(rpc_url, addrs, chain_id).await?`
     - `client.register(&agent_uri, &signer).await?` returns a `TokenId`.
     Mantle testnet is `chain_id = 5003`; mainnet is `5000`.
     Then:
     ```bash
     export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # testnet
     export MANTLE_DEPLOYER_KEY=$(op read 'op://Personal/xvision-mantle/deployer_pk')
     for manifest in identity/*.agent.json; do
       cargo run --release -p xvision-identity \
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
- **FOLLOWUPS:** SLF3. **xvision-identity is opt-in** — keep it excluded from
  `default-members` in `Cargo.toml`; explicit `cargo build -p xvision-identity`
  to compile.

---

## Tier 3 — secrets, models, datasets (one-time setup)

### M8. Anthropic API key (or alternative)

- **What:** sign up at <https://console.anthropic.com>; create a key.
- **Save:** `op://Personal/xvision-anthropic/api_key`.
- **Export:**
  ```bash
  export ANTHROPIC_API_KEY=$(op read 'op://Personal/xvision-anthropic/api_key')
  ```
- **Cost rough estimate:** Phase 9 backtest = 100–300 setups × 1 brief ≈
  $1–5 with Haiku; with Opus reasoning, $20–60. Prefer Haiku in CI.

### M9. OpenAI / OpenRouter / Together / Groq key (optional)

- **What:** any OpenAI-compat endpoint works. OpenRouter recommended for
  multi-model evaluation.
- **Save:** `op://Personal/xvision-openai/api_key`.
- **Export:**
  ```bash
  export OPENAI_API_KEY=$(op read 'op://Personal/xvision-openai/api_key')
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
- **Verify:** `cargo run --release -p xvision-trader --bin smoke-trader` loads
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
  4. Wire `ProbeRunner` in `xvision-eval` per implementation-plan §8.5.
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
  2. Run `cargo run --release -p xvision-inference --bin smoke-qwen3` 10×
     with default `RUSTFLAGS`.
  3. Cool box; repeat 10× with `RUSTFLAGS="-C target-cpu=native"`.
  4. Compare median + p95 decode/prefill tok/s.
- **Exit:** if win is ≥1.5× and stable, codify in `.cargo/config.toml` (F10).
- **FOLLOWUPS:** F9, F10.

---

## Strategy authoring (Plan 2a — see crates/xvision-engine/README.md)

```bash
xvn doctor [--json]
xvn strategy templates [--json]        # list templates
xvn strategy create --template <t> --name <n> [--json]
xvn strategy create --name <n> --prompt @prompt.md \
  --provider <provider> --model <model> --asset BTC/USD --timeframe 4h
xvn strategy create --from-file strategy.json [--json]
xvn strategy validate <id>
xvn strategy validate <id> --scenario <scenario_id> [--json]
xvn strategy edit <id> [--no-filter-warning | --clear-no-filter-warning]
xvn strategy clone <id> --name <n> [--provider <provider> --model <model>] [--json]
xvn strategy show <id>
xvn strategy ls [--json]
xvn strategy run <id> --fixture <name> --decisions <N> [--mock]
```

Dashboard strategy editing lives at `/strategies/:id`; `/authoring/:id` is
kept only for older links. The inspector lets operators edit display name,
description, asset universe, cadence, filter, attached agents, and risk while
keeping the strategy ID stable for eval history. Strategy validation is
explicit: use **Check eval readiness** in the inspector or
`xvn strategy validate`, instead of treating draft-load warnings as blocking
form errors.

### AI agent drives xvn (Plan 2a)

External AI agents (Claude Code, Hermes, Cursor, Codex) can author the
same `Strategy` artifacts over MCP without the operator-CLI round-trip:

```bash
cargo build --release -p xvision-mcp        # produces target/release/xvn-mcp
```

Authoring verbs the server advertises over `tools/list`:
`xvn_list_templates`, `xvn_create_strategy`, `xvn_get_strategy`,
`xvn_update_slot`, `xvn_update_manifest`, `xvn_set_risk_config`,
`xvn_validate_draft` — alongside the indicator surface (`xvn_health`,
`xvn_sma`, `xvn_rsi`, ...) that has shipped since v0.1. State lives in
`$XVN_HOME/strategies/<id>.json`, the same path `xvn strategy ls` reads
from.

> The Plan 2b in-app skills surface (`xvn skill …`, the `xvn_*_skill*`
> MCP verbs, the `xvision-skills` crate) was removed per ADR 0012 — the
> Agents page (`engine::agents`, `/agents`) replaces it as the
> reusable-prompt authoring surface.

In Claude Code, register the binary in `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "xvn": { "command": "/path/to/target/release/xvn-mcp" }
  }
}
```

The web wizard at `/setup` (Plan 2d) drives the same authoring verbs
internally via `xvision_engine::authoring`, so a draft authored in the
chat UI is immediately visible to both `xvn strategy ls` and a connected
MCP agent.

End-to-end paths beyond this surface (marketplace publishing, live
trading, batch eval) land in subsequent plans (2c, 3, 5) — they share
this same saved `Strategy` artifact shape.

### Eval runs

The shipped eval surface is available through both the dashboard and the CLI:

```bash
xvn doctor --json
xvn provider list
xvn provider check --name <provider>
xvn provider models --name <provider>
xvn strategy diagnostics <id> --json
xvn eval validate --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval list [--json]
xvn eval get <run_id> [--json]
xvn eval watch <run_id> [--once] [--json]
xvn eval results <run_id> [--json]
xvn eval compare <run_id_a> <run_id_b>
```

For agent-facing operation, use that order: provider readiness, then strategy
diagnostics, then eval validation, then eval launch. `strategy diagnostics`
checks whether required capabilities are launchable; `eval validate` checks the
specific strategy/scenario/mode without enqueueing a run.

Execution labels:

- **Filter-gated agent** is the default filtered LLM path: a saved filter
  artifact gates whether the configured agent/model is called.
- **Rules-only mechanical** is advanced deterministic execution with no model
  call. This is intentional no-agent mode, not a broken missing-agent state.
- **Agent-direct** is legacy/discouraged model execution without a saved filter
  gate. Use only for explicit comparisons or old-run interpretation.

Add `--auto-fire-review` to `xvn eval run` when a completed run should
immediately write a deterministic review and chart annotations. Optional
review metadata can be recorded with `--review-provider`,
`--review-model`, and `--max-review-annotations`; `xvn eval show <run_id>`
prints the stored auto-review state. The dashboard eval launcher exposes
the same auto-run review checkbox, and `/charts/annotated?run_id=<run_id>`
renders annotations from the newest completed review for that run.

Compare labels prefer the strategy display name when the run's strategy
manifest is available, while keeping the run id and strategy id visible in
CLI output and dashboard secondary text. The run-centric dashboard is
`/eval-runs/compare?ids=<run-a>,<run-b>`; the Charts v2 comparison dashboard
is `/charts/compare?ids=<run-a>,<run-b>`.

`xvn eval run` is part of the current surface. Use `xvn scenario ls` to find
scenario ids; `xvn eval scenarios` remains available but is deprecated.

### Exit codes

`xvn strategy *` and `xvn eval *` follow Printing-Press-style typed exit
codes so AI agents can dispatch on the *number*, not the error text:

| Code | Meaning | Agent should |
|------|---------|--------------|
| 0 | Success | continue |
| 2 | Usage / malformed input / unknown enum variant | re-read `--help`, fix the invocation |
| 3 | Auth (missing or invalid credential) | prompt operator for `ANTHROPIC_API_KEY` or `--mock` |
| 4 | Resource not found (strategy id, run id) | re-fetch with `xvn <verb> ls`; the id is stale |
| 5 | Upstream / network / disk / database error | retry with backoff |
| 7 | State conflict (e.g. duplicate name on rename) | inspect the resource and reconcile state |

Other verbs (`fire-trade`, `venue`, `dashboard`, `eod`, …)
default to exit 5 on any error pending per-command opt-in.

```bash
xvn strategy show 01BAD; echo $?      # 4
xvn eval get 01BAD; echo $?           # 4
```

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
export XVN_INTERN_PROVIDER=anthropic          # | openai-compat
export XVN_INTERN_BASE_URL=https://api.anthropic.com
export XVN_INTERN_MODEL=claude-haiku-4-5
export XVN_INTERN_KEY_ENV=ANTHROPIC_API_KEY   # name of the var that holds the key
export ANTHROPIC_API_KEY=...                  # M8 (or OPENAI_API_KEY etc. — M9)

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

# ── Live perps venues (all gated by the SafetyGate; mainnet = real money) ──
# Hyperliquid perps — TWO NATIVE paths (preferred: EIP-712 signed in Rust, no npm,
# no fund-capable key in env):
#   --venue hyperliquid : HL_API_KEY / HL_ACCOUNT_ADDRESS / HL_NETWORK;
#                         mainnet requires HL_ALLOW_MAINNET=1.
#   --venue degen_arena : DEGEN_HL_API_KEY / DEGEN_HL_ACCOUNT_ADDRESS /
#                         DEGEN_HL_NETWORK; mainnet requires DEGEN_ALLOW_MAINNET=1.
#
# Byreal — Solana ecosystem venue (SPL spot / CLMM-LP / RFQ / xStocks) that ALSO
# routes perps to Hyperliquid via npx @byreal-io/byreal-perps-cli. The CLI reads the
# key from env (custody trade-off) — prefer the native HL paths above for plain perps.
export BYREAL_PRIVATE_KEY=$(op read 'op://Personal/xvision-byreal/private-key')
export BYREAL_NETWORK=mainnet                 # or testnet; defaults to mainnet
export BYREAL_ACCOUNT=...                      # optional account id
export BYREAL_LEVERAGE=5                       # optional; per-coin leverage before each entry
# Alpaca creds (APCA_*) are required for byreal/orderly (Alpaca supplies the bar
# stream); Hyperliquid/Degen Arena use HL-native candles, no Alpaca needed.
#
# Manual one-shot CLI tools (mainnet byreal needs the explicit ack flag):
#   xvn fire-trade --venue byreal --asset BTC --side buy --size-bps 100 --i-understand-real-money
#   xvn close-position --venue byreal BTC --i-understand-real-money
#   xvn portfolio --venue byreal            # read-only, no ack
#
# Agent-driven live runs (the parity path — SafetyGate + venue_label):
#   Testnet: broker_creds_ref + the venue's *_NETWORK=testnet  → venue_label=Testnet
#   Mainnet: xvn live --venue <byreal|hyperliquid|degen_arena> --network mainnet \
#              --i-understand-real-money     (sets venue_label=Live; real money)
#   A non-Live-labelled run can NEVER reach a Live (mainnet) broker — the gate blocks it.

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

## Enable Cortex memory on an agent (verify end-to-end)

Cortex memory (the in-process `xvision-memory` layer) lets a memory-enabled
agent recall its salient prior decisions before acting and write new ones
back. It is **default-off** on every surface and requires an embedder.

**1. Provision an embedder.** Set one of (see `docker/README.md` for the
full resolution order):

```bash
# Reuse a registered OpenAI-compatible provider's key (no hard OpenAI dep):
export XVN_MEMORY_EMBEDDER_PROVIDER=openai      # a provider with an /embeddings endpoint
# …or fall back to the plain OpenAI env path:
export OPENAI_API_KEY=sk-...                     # optional: OPENAI_BASE_URL for a proxy
# …or, for an offline/dev box with no embedding API (low recall quality):
export XVN_MEMORY_EMBEDDER=local
```

**1b. Local embeddings via Ollama (no API key, pick your model).** Run
embeddings on a local Ollama server and choose the model from the dashboard
— no OpenAI dependency. Ollama exposes an OpenAI-compatible
`/v1/embeddings` endpoint, so the existing embedder transport just works:

```bash
ollama pull nomic-embed-text        # or qwen3-embedding, mxbai-embed-large, bge-m3, …
```

Then in **Settings → Providers**, add an **Ollama** provider with base_url
`http://localhost:11434/v1`. The trailing **`/v1` is required** — the
embedder POSTs `{base_url}/embeddings`, so the URL must resolve to
`http://localhost:11434/v1/embeddings`. (Ollama is a no-auth kind; leave the
API key blank.)

Finally, in **Settings → General → Memory**:
- **Embedder source** = your Ollama provider.
- **Embedding model** = `nomic-embed-text` (or `qwen3-embedding`, etc.) — or
  pick **Custom…** to type any model name your server serves.

**1c. One-step local embeddings (Custom endpoint — no provider needed).** The
fastest path: skip provider registration entirely and point memory straight at
a local OpenAI-compatible server from the Memory card.

```bash
ollama pull nomic-embed-text        # or qwen3-embedding
```

Then in **Settings → General → Memory**:
- **Embedder source** = **Custom endpoint (OpenAI-compatible)**.
- **Custom endpoint base URL** = `http://localhost:11434/v1` (include the
  trailing **`/v1`** — the embedder POSTs `{base_url}/embeddings`).
- **Embedding model** = `nomic-embed-text` (or `qwen3-embedding`).

The custom endpoint is **no-auth only** — no API key is stored (the base URL
lives in `memory.toml`, which is not a secrets file). Works for Ollama,
llama.cpp, LM Studio, and vLLM. **For an authenticated endpoint, register it as
a provider in the Providers tab instead** (paths 1b above). To force a custom
endpoint from the environment, set `XVN_MEMORY_EMBEDDER_BASE_URL=http://host/v1`
(wins over the card; honors `OPENAI_API_KEY` if set).

The embedding dimension differs per model (nomic = 768, openai-3-small =
1536); the store records each observation's real vector length, so this is
handled automatically. The resolved embedder id is **model-aware**
(`openaicompat:nomic-embed-text`), so switching models keeps the vector
spaces separate — but recall only matches within the same id. **Don't switch
embedders mid-corpus**; if you do, `xvn memory forget --namespace <ns>` the
affected namespaces and re-embed.

The env override `XVN_MEMORY_EMBEDDER_MODEL` still wins over the dashboard
pick, for operators who script it.

**2. Confirm the substrate is healthy.**

```bash
xvn memory status        # store path + writable?, embedder present + id, grace days, namespaces
xvn doctor --json | jq .memory
```

`embedder_present: true` is required for recall to do anything. With no
embedder the agent path still runs — recall just no-ops (you'll see
`memory_disabled_no_embedder` in traces).

**3. Enable memory on a slot.** In the dashboard, open the agent, and on a
slot set **Memory** to `Global` (shared across agents) or `Agent-scoped`
(this agent only). Save. (CLI/API equivalent: persist the slot with
`memory_mode = "global" | "agent_scoped"`.)

**4. Verify recall across runs.** Run two eval cycles over the same agent:

```bash
xvn memory status        # observation count for the slot's namespace should grow after run 1
# run a second eval; in the trace dock (or obs stream) the second run shows a
# `memory_recall` event with k>0 hits, and the trader prompt carries a
# <prior_observations> block.
```

Backtest temporal-safety holds automatically: recall in an eval/backtest
context excludes future-dated Patterns (the scenario-start filter), so a
replay can never leak knowledge from after its window.

**Disable / clean up.** Set the slot's Memory back to `Off`, or clear a
namespace with `xvn memory forget --namespace <ns>` (soft-delete; restore
within the grace window via `xvn memory undo-forget`).

---

## Scale tiers

xvision's design assumes a single operator at v1. Several architectural
breakpoints surface at specific user/agent counts; this section documents them
so capital + ops decisions can be planned, not stumbled into.

### N = 1 (single-operator, today)

- **Custody:** single env-var `CREDENTIAL_SECRET` encrypts the operator's
  trading key. Acceptable.
- **Operations:** operator runs `tail -f` on tracing, fires `xvn` commands
  manually. Acceptable; ~2-6 hrs/day.
- **Compliance:** open-source code, self-hosted. No OFAC screening obligation
  on the maintainers (operator's jurisdiction is operator's responsibility).
- **Storage:** single SQLite file. Backups via `sqlite3 .backup` once a day if
  trades > $0.

### N = 10 (multiple users on one operator-managed instance)

Three things break here:

- **Custody:** the env-var-derived single secret encrypts every user's trading
  key. One env var compromise → 10 keys lost. **Migrate to:** per-user HKDF-
  derived key (already implemented in `TradingKeyStore`); rotate the master
  secret quarterly.
- **Operator load:** 6 hrs/day becomes 12. **Migrate to:** scheduled `xvn eod`
  reports + alert routing (Item E of this plan); operator-on-call rotation if
  > 1 person.
- **Compliance:** the moment xvision's marketplace contract takes fees from
  10 distinct EVM addresses, OFAC screening becomes load-bearing for the
  hosting entity (not the open-source code itself). **Migrate to:** OFAC
  screening at the marketplace contract event handler. Tracked in FOLLOWUPS.

### N = 100

- **Storage:** SQLite write throughput hits its ceiling around hundreds of
  concurrent writes/sec. Reservations + audit-log + ledger all serialize. WAL
  mode helps to ~thousands; beyond that, evaluate Postgres.
- **Autoresearcher cost:** at N=100 with each agent generating 100 mutator
  variants/night × 50K-token briefings × Sonnet-class evaluation, the LLM bill
  is ~$15K/month. **Migrate to:** subscription tier or hosted-runtime line
  (research Theme G).
- **Reputation governance:** when 100+ agents have attestations, the question
  "who can attest?" becomes load-bearing. v1 gates attestations to operator +
  judges. **Migrate to:** explicit governance ladder before this scale.
- **Custody (continued):** at N=100, single-process key custody becomes a real
  concentration risk. **Migrate to:** MPC or smart-account paths (FOLLOWUPS).

### N = 1000

- **Storage:** Postgres mandatory.
- **Operations:** 24/7 on-call. Incident-response runbook required (see
  `## Incident response` below).
- **Distribution:** one operator/instance no longer scales; multi-tenant
  deployment with per-tenant isolation. Effectively a v3 architecture.

### Where the breakpoints come from

- N=1 → N=10 ops break: research Run 8 (operator daily journal — daily review
  becomes full-time at N=10).
- N=10 → N=100 storage + autoresearcher cost: research Run 11 (scaling tree).
- N=100 → N=1000 distribution: research Run 11 + Run 4 (mutation-loop cost).

### Default cadence

- Run `xvn eod` daily (scheduled via Plan 2c when it lands; manual until then).
- Read MANUAL.md once a quarter to confirm the scale tier still matches reality.
- Review FOLLOWUPS.md monthly for items that have become load-bearing.

---

## Incident response

Use this checklist when something is wrong or might be wrong. The order is
fixed: contain first, diagnose second, communicate third, post-mortem fourth.

### 1. Contain (≤ 5 min)

- [ ] Disable the affected scheduler/process supervisor or dashboard-triggered
      run source. The shipped CLI does not yet expose global halt/unhalt
      commands.
- [ ] If exposure is open, inspect it with `xvn portfolio --venue <venue>` and
      close the affected asset with `xvn close-position --venue <venue> --asset <asset>`.
      Default to closing when "wrong direction" exposure is suspected; avoid
      closing when investigating a tooling glitch with no exposure component.
- [ ] Post a one-line status to wherever your status channel is: "Halt at
      <UTC time>; investigating <one-line>." Don't wait for completeness.

#### Emergency rollback: Cline runtime → legacy LlmDispatch

As of the Stage 3 Cline runtime unification, **Cline (the `xvision-agentd`
sidecar) is the unconditional routine runtime** — every normal run drives
LLM slots through the sidecar. The per-config `agent_runtime` selector no
longer chooses the routine path.

If a sidecar-side regression is suspected (the agent loop misbehaves, the
sidecar wedges, a Cline SDK upgrade broke decisions), you can roll the
routine LLM path back to the legacy raw-reqwest `LlmDispatch` for the
incident by setting one env var on the affected process:

```bash
export XVN_EMERGENCY_LLM_DISPATCH=1   # or =true
# restart the affected scheduler / xvn process so it re-reads the env
```

- **Blast radius:** the single process that sets the var (opt-in only).
  Other processes / hosts are unaffected.
- **Logged loudly:** the engine emits a `warn!` on every run naming the
  rollback and the env var to unset; the rollback is never silent.
- **Restore Cline:** `unset XVN_EMERGENCY_LLM_DISPATCH` and restart.
- **Time-boxed:** this off-ramp is a temporary incident lever, not a
  supported steady state. Removal is planned after the Cline path bakes in
  (target: one release after Stage 3 ships); track the removal under the
  runtime-unification spec (inheritance item 6).

### 2. Diagnose (≤ 30 min)

- [ ] Inspect recent eval history with `xvn eval list` and
      `xvn eval get <run_id>`.
- [ ] Cross-check venue state with `xvn portfolio --venue <venue>`.
- [ ] Identify whether the issue is:
      - **Strategy bug** (specific agent producing wrong decisions)
      - **Risk engine miss** (decision passed risk that shouldn't have)
      - **Execution glitch** (signed payload mismatched, fill mismatched)
      - **Broker outage** (Orderly returned 5xx)
      - **Operator error** (wrong CLI command run)
- [ ] If the issue is constrained to one strategy, pause that strategy in the
      scheduler/operator process that launched it. Per-strategy halt/unhalt
      commands are not part of the shipped CLI surface yet.

### 3. Communicate (≤ 60 min after detection)

- [ ] Update status channel with what you've found.
- [ ] If user funds are or were at risk, the open-source disclosure SLA is:
      a public summary within 7 days of containment (not 30 — sooner is more
      credible). Post-launch, this can be a `SECURITY.md` policy.

### 4. Post-mortem (within 7 days)

- [ ] Write up: timeline, root cause, what worked, what didn't, what changes.
- [ ] If the post-mortem identifies a missing safety check, add a task to a
      plan that addresses it. Don't leave the gap open.
- [ ] If the post-mortem reveals a policy or runbook gap, update MANUAL.md.

---

*Last updated: 2026-05-04. Cross-references: `FOLLOWUPS.md`,
`implementation-plan.md` Phases 9–12.*
