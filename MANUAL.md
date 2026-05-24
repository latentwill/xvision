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

### M11.5. Wire the MCP indicator server (only when `INTERN=acpx`)

- **Trigger:** running the Stage 1 Intern through the ACPX agent harness
  (`XVN_INTERN_PROVIDER=acpx`) and you want the agent to recompute
  indicators at parameter sets the snapshot doesn't pre-bake (e.g. RSI(7)
  when the snapshot only carries RSI(14)). Skip otherwise — the MCP server
  is irrelevant to the OpenAI-compat / Anthropic Intern paths.
- **What:** `crates/xvision-mcp/` builds a stateless stdio MCP server,
  `xvn-mcp`, that exposes `xvision-data`'s indicator surface (rsi · sma ·
  ema · bollinger · atr · macd · donchian · fib_retracements · health) as
  agent-callable tools. ACPX advertises it to every agent session via
  `mcpServers: [...]` in `acpx.config.json`.
- **Setup steps** (auto-run by `scripts/setup_runpod.sh` when
  `INTERN=acpx`):
  1. `cargo build --release -p xvision-mcp` (produces `target/release/xvn-mcp`).
  2. Write `<acpx-workspace>/acpx.config.json` registering the binary as a
     stdio MCP server. The setup script does this for you; otherwise
     install ACPX (`npm install -g acpx@latest`) and add the stanza by
     hand. Each ACPX-driven agent (claude / codex / openclaw / hermes /
     etc.) has its own install + auth flow on top — see the relevant agent
     CLI for those.
- **Verify:** from inside the chosen ACPX agent session, ask it to call
  the `xvn_health` tool. Expected response: `{"ok": true, "name":
  "xvision-mcp", "version": "<x.y.z>"}`. Any other indicator (`xvn_rsi` on
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
    `xvision-data` pipeline.
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
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval validate --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval list [--json]
xvn eval get <run_id> [--json]
xvn eval watch <run_id> [--once] [--json]
xvn eval results <run_id> [--json]
xvn eval compare <run_id_a> <run_id_b>
```

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

Other verbs (`fire-trade`, `venue`, `ab-compare`, `dashboard`, `eod`, …)
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
